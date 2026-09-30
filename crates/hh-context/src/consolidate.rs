//! `consolidate` — the C1 consolidation driver (§5c.3; ADR-0080 d4–d7).
//!
//! Memory consolidation is **supersession under a writer lease**: a
//! declared `ConsolidationRule` (cadence + guards — OQ-205 keeps the rule
//! inventory a `HarnessRule` matter, so the driver *executes a declared
//! rule*, never invents a cadence) reads `stale_candidates(scope)`, rebinds
//! the stale versions' names onto their live replacements through `bind` —
//! the only op that rewrites a manifest, always with
//! `supersedes{reason: consolidation}` — and emits a content-addressed
//! `ConsolidationRecord` carrying `{input_watermark,
//! last_success_watermark, rule_ref, binds, skipped, usage}` for the
//! caller's `control.budget.consumed{attribution:
//! harness_overhead.consolidation}` post.
//!
//! Determinism (CC4): the driver is pure over `(store, rule, ctx)` — the
//! candidate set is the store's `stale_candidates` view in write order,
//! names rebind in sorted order, and the record's content address is the
//! idp of its preimage. Guards are *declared* (`ConsolidationGuards`), and
//! a fenced writer (`ctx.lease_generation ≠ store.lease(scope)`) is the
//! typed `Fenced` refusal — the same fencing `put` enforces (ADR-0080 d5).

use std::collections::BTreeMap;

use hh_provenance::authority::PersistenceScope;
use hh_wire::json::Json;

use crate::memory::{MemoryError, MemoryStorePort, WriteContext};

/// `ConsolidationCadence` — the declared rule variants (§5c.3 C1:
/// "consolidation cadence/guard rules as variants"; OQ-205 — the rule
/// inventory is a `HarnessRule` matter, the driver only *executes* the
/// declared cadence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsolidationCadence {
    /// Run once at session start.
    SessionStart,
    /// Run when the caller declares the store idle.
    Idle,
    /// Run at most once per `floor_per_seq` seqs since the last successful
    /// consolidation — the rate-limit floor (the caller holds
    /// `last_success_watermark`; the driver is stateless).
    RateLimit {
        /// The minimum seq distance between successful runs.
        floor_per_seq: u64,
    },
}

impl ConsolidationCadence {
    /// The canonical spelling (record + report fields).
    pub fn as_str(&self) -> &'static str {
        match self {
            ConsolidationCadence::SessionStart => "session_start",
            ConsolidationCadence::Idle => "idle",
            ConsolidationCadence::RateLimit { .. } => "rate_limit",
        }
    }

    /// Whether the cadence admits a run at `input_watermark` given the
    /// caller-held `last_success_watermark` (None = never ran).
    pub fn admits(&self, input_watermark: u64, last_success_watermark: Option<u64>) -> bool {
        match self {
            ConsolidationCadence::SessionStart | ConsolidationCadence::Idle => true,
            ConsolidationCadence::RateLimit { floor_per_seq } => match last_success_watermark {
                None => true,
                Some(last) => input_watermark.saturating_sub(last) >= *floor_per_seq,
            },
        }
    }
}

/// `ConsolidationGuards` — the declared guard rules (§5c.3 C1; ADR-0080 d7
/// — the fault-injection rows "fenced consolidation writes" and
/// "concurrent extractors on one range" are lease/guard matters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationGuards {
    /// Refuse when `ctx.lease_generation` is not the scope's current lease
    /// (a fenced consolidation never writes — same rule `put` enforces).
    pub require_current_lease: bool,
    /// Skip stale versions that are members of a conflict set whose
    /// resolution is `escalated`/`withheld` (still under review — a
    /// consolidation never pre-empts the resolver).
    pub skip_conflicted: bool,
    /// `max_versions` — the budget guard: at most this many stale versions
    /// rebind per run (the remainder stays stale for the next run — never
    /// silently dropped; they are listed under `deferred`).
    pub max_versions: Option<u64>,
}

impl Default for ConsolidationGuards {
    fn default() -> Self {
        ConsolidationGuards {
            require_current_lease: true,
            skip_conflicted: true,
            max_versions: None,
        }
    }
}

/// `ConsolidationRule{rule_id, cadence, guards}` — the declared rule the
/// driver executes (a `HarnessRule` projection at the call site — the
/// driver receives the resolved rule, never the rule store).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationRule {
    /// The rule's identity (`HarnessRule` id at the call site).
    pub rule_id: String,
    /// The declared cadence.
    pub cadence: ConsolidationCadence,
    /// The declared guards.
    pub guards: ConsolidationGuards,
    /// The consolidator — the deterministic C1 shape (`dedupe`: rebind a
    /// stale version's names onto its live replacement; model consolidators
    /// are C2 and ride the same watermarks/lease rules).
    pub consolidator: ConsolidatorKind,
}

/// `ConsolidatorKind` — the closed set of consolidation mechanics. `Dedupe`
/// is the deterministic, model-free C1 consolidator: for each stale
/// version, the live replacement is the newest non-stale version on the
/// same `semantic_id` line (the version that superseded it); its bound
/// names rebind onto the replacement with `reason: consolidation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidatorKind {
    /// The deterministic rebind consolidator (no new content — a manifest
    /// supersedure, ADR-0080 d4's "manifests rewritten only here").
    Dedupe,
}

impl ConsolidatorKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConsolidatorKind::Dedupe => "dedupe",
        }
    }
}

/// One applied rebind — `{name, from_version, to_version}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationBind {
    /// The rebound name.
    pub name: String,
    /// The stale version the name pointed at.
    pub from_version: String,
    /// The live replacement the name now points at.
    pub to_version: String,
}

/// `ConsolidationRecord` — the content-addressed consolidation fact
/// (§5c.3; the diff the driver leaves in the ledger — `consolidation_id`
/// is the idp of `{rule_id, input_watermark, last_success_watermark,
/// binds, skipped}` — same inputs, same record).
#[derive(Debug, Clone)]
pub struct ConsolidationRecord {
    /// The record's content address.
    pub consolidation_id: String,
    /// The executed rule.
    pub rule_id: String,
    /// The consolidator.
    pub consolidator: String,
    /// `input_watermark` — `store.applied_seq()` when the stale set was
    /// read (the fold point the record answers for).
    pub input_watermark: u64,
    /// `last_success_watermark` — the caller-held watermark of the previous
    /// successful run (None on the first).
    pub last_success_watermark: Option<u64>,
    /// The applied rebinds (sorted by name — deterministic).
    pub binds: Vec<ConsolidationBind>,
    /// Stale versions skipped under the conflict guard.
    pub skipped_conflicted: Vec<String>,
    /// Stale versions skipped because no live replacement exists (CC3 —
    /// listed, never silently dropped).
    pub skipped_no_replacement: Vec<String>,
    /// Stale versions deferred under `max_versions` (the next run sees
    /// them — the watermark does not advance past them).
    pub deferred: Vec<String>,
    /// `usage` — the accounting payload the caller charges under
    /// `harness_overhead.consolidation` (deterministic shape:
    /// `{versions_examined, binds_written}` — the driver measures ops, the
    /// caller posts the cost).
    pub usage: Json,
}

/// `ConsolidateError` — the typed refusal set.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsolidateError {
    /// `Fenced` — the writer's lease generation is not the scope's current
    /// lease (ADR-0080 d5; the same fencing `put` enforces — a fenced
    /// consolidation refuses, never writes around the lease).
    Fenced {
        /// The scope.
        scope: PersistenceScope,
        /// The current lease.
        expected: u64,
        /// The caller's generation.
        got: u64,
    },
    /// `CadenceGate` — the `rate_limit` floor has not elapsed since
    /// `last_success_watermark` (a typed skip — the report still mints).
    Store {
        /// The underlying store error (a `bind` failure mid-run).
        error: MemoryError,
    },
}

impl std::fmt::Display for ConsolidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsolidateError::Fenced {
                scope,
                expected,
                got,
            } => write!(
                f,
                "ConsolidateError::Fenced{{scope:{scope:?}, expected:{expected}, got:{got}}}"
            ),
            ConsolidateError::Store { error } => write!(f, "ConsolidateError::Store{{{error}}}"),
        }
    }
}
impl std::error::Error for ConsolidateError {}

/// `consolidate(store, scope, rule, ctx, last_success_watermark)` — the C1
/// driver. Order: cadence gate → lease guard → `stale_candidates` →
/// conflict guard → `max_versions` → rebind (`dedupe`) → record.
///
/// Returns `Ok(None)` when the cadence gate is closed (a typed skip — the
/// caller does not advance `last_success_watermark`); `Ok(Some(record))`
/// on an applied run; `Err(Fenced)`/`Err(Store)` on refusal/failure.
///
/// `store` is taken through the `MemoryStorePort` surface — the same
/// driver runs against an out-of-process `memory_store` binding (T-LCD-12;
/// §5c.3 C1 "out-of-process stores").
pub fn consolidate(
    store: &mut dyn MemoryStorePort,
    scope: PersistenceScope,
    rule: &ConsolidationRule,
    ctx: &WriteContext,
    last_success_watermark: Option<u64>,
) -> Result<Option<ConsolidationRecord>, ConsolidateError> {
    let input_watermark = store.applied_seq();

    // (1) Cadence gate — a closed gate skips without a record (the caller
    // keeps `last_success_watermark`; nothing minted, nothing lost).
    if !rule.cadence.admits(input_watermark, last_success_watermark) {
        return Ok(None);
    }

    // (2) Lease guard — fenced consolidation writes refuse (ADR-0080 d5).
    if rule.guards.require_current_lease {
        let expected = store.lease(scope);
        if ctx.lease_generation != expected {
            return Err(ConsolidateError::Fenced {
                scope,
                expected,
                got: ctx.lease_generation,
            });
        }
    }

    // (3) The stale set — the store's own `stale_candidates` view (the same
    // view R-2.4.4 consumes; deterministic write order).
    let stale = store.stale_candidates(scope);

    // (4) Conflict guard — members of `escalated`/`withheld` sets stay put.
    let mut skipped_conflicted: Vec<String> = Vec::new();
    let mut candidates: Vec<String> = Vec::new();
    if rule.guards.skip_conflicted {
        // Conflict membership is read through `version`'s conflict_set_ref
        // — the port's `version` accessor answers it on either placement.
        let mut unresolved: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for vid in &stale {
            if let Some(v) = store.version(vid) {
                if let Some(set_ref) = &v.conflict_set_ref {
                    let _ = set_ref;
                    unresolved.insert(vid.clone());
                }
            }
        }
        // A set is "resolved" for consolidation purposes when every member
        // is already superseded by a `Superseded{head}` resolution — the
        // store's conflict records carry the resolution; `version` gives
        // membership, the stale set gives staleness. Any conflicted stale
        // member stays (the resolver owns it).
        for vid in stale.iter() {
            if unresolved.contains(vid) {
                skipped_conflicted.push(vid.clone());
            } else {
                candidates.push(vid.clone());
            }
        }
    } else {
        candidates = stale;
    }

    // (5) `max_versions` — the remainder defers (listed, never dropped).
    let mut deferred: Vec<String> = Vec::new();
    if let Some(max) = rule.guards.max_versions {
        if candidates.len() as u64 > max {
            deferred = candidates.split_off(max as usize);
        }
    }

    // (6) `dedupe` — per stale version, the live replacement is the newest
    // non-stale version on the same semantic line; every name bound to the
    // stale version rebinds onto it (the manifest rewrites only here —
    // ADR-0080 d4 — always `supersedes{reason: consolidation}`).
    let manifest = store.manifest(scope, input_watermark);
    // `name → bound version` inversion: `version_id → [names]` (sorted).
    let mut names_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, vid) in &manifest.entries {
        names_of.entry(vid.clone()).or_default().push(name.clone());
    }
    let stale_set: std::collections::BTreeSet<String> = candidates.iter().cloned().collect();
    let live_versions: std::collections::BTreeSet<String> = store
        .version_order()
        .into_iter()
        .filter(|vid| !stale_set.contains(vid))
        .collect();
    let mut binds: Vec<ConsolidationBind> = Vec::new();
    let mut skipped_no_replacement: Vec<String> = Vec::new();
    for vid in &candidates {
        let Some(v) = store.version(vid) else {
            skipped_no_replacement.push(vid.clone());
            continue;
        };
        // The replacement: the newest version on the same `semantic_id`
        // line that is not stale (the version that superseded this one —
        // walk the line's write order).
        let replacement = store
            .version_order()
            .into_iter()
            .filter(|id| live_versions.contains(id))
            .filter(|id| {
                store
                    .version(id)
                    .map(|lv| lv.semantic_id == v.semantic_id)
                    .unwrap_or(false)
            })
            .last();
        let Some(replacement) = replacement else {
            skipped_no_replacement.push(vid.clone());
            continue;
        };
        let names = names_of.remove(vid).unwrap_or_default();
        if names.is_empty() {
            // Unbound stale versions have nothing to rebind — their
            // supersedure is already the lineage fact; they land under
            // `skipped_no_replacement` (no consolidation output — listed,
            // never silently dropped, CC3).
            skipped_no_replacement.push(vid.clone());
            continue;
        }
        for name in names {
            match store.bind(
                scope,
                &name,
                &replacement,
                Some(vid),
                "consolidation",
                input_watermark.max(ctx.at_seq),
            ) {
                Ok(_) => binds.push(ConsolidationBind {
                    name: name.clone(),
                    from_version: vid.clone(),
                    to_version: replacement.clone(),
                }),
                Err(error) => return Err(ConsolidateError::Store { error }),
            }
        }
    }
    binds.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.from_version.cmp(&b.from_version))
    });

    // (7) The record — content-addressed over the applied diff (CC3/CC4).
    let preimage = Json::obj([
        ("rule_id", Json::str(rule.rule_id.clone())),
        ("consolidator", Json::str(rule.consolidator.as_str())),
        ("input_watermark", Json::Int(input_watermark as i64)),
        (
            "last_success_watermark",
            last_success_watermark
                .map(|w| Json::Int(w as i64))
                .unwrap_or(Json::Null),
        ),
        (
            "binds",
            Json::Arr(
                binds
                    .iter()
                    .map(|b| {
                        Json::obj([
                            ("name", Json::str(b.name.clone())),
                            ("from", Json::str(b.from_version.clone())),
                            ("to", Json::str(b.to_version.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
    .to_canonical_string();
    let usage = Json::obj([
        ("versions_examined", Json::Int(stale_set.len() as i64)),
        ("binds_written", Json::Int(binds.len() as i64)),
        ("attribution", Json::str("harness_overhead.consolidation")),
    ]);
    Ok(Some(ConsolidationRecord {
        consolidation_id: hh_identity::idp::idp_id("memory_consolidation.1", preimage.as_bytes()),
        rule_id: rule.rule_id.clone(),
        consolidator: rule.consolidator.as_str().to_string(),
        input_watermark,
        last_success_watermark,
        binds,
        skipped_conflicted,
        skipped_no_replacement,
        deferred,
        usage,
    }))
}
