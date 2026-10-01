//! `leaderboard(definition, view)` — the lab-internal L1–L5 admission and
//! ranking view (§6.5 §5.4/§2.2; ADR-0163). Lab-internal means: no
//! publication, no multi-tenant submission surface, no hosted service —
//! the snapshot is a `view_hash`-bound projection over the cell table and
//! the bundle catalogue, reproducible byte-for-byte at its watermark set.
//!
//! The admission ladder (each gate's failures stay visible in
//! `exclusions[]` with the gate id — nothing is silently dropped):
//!
//! - **L1 bundle** — a catalogue entry covering the run at `status ≥
//!   min_status` (`validated` by default) and, when the definition sets
//!   `min_claimed_level`, `claimed_level ≥ min_claimed_level`. No
//!   qualifying bundle, no rank — AC-R-2.10.5-3.
//! - **L2 eligibility** — the cell-table `included` flag (not excluded,
//!   not superseded, finished) plus the metric's declared applicability
//!   (`applies_to_classes`, `requires_observability`) and a concrete value.
//! - **L3 budget match** — the arm carries a `MatchSpec`; the stratum is
//!   the spec's canonical idp. Ranks never cross strata — unmatched arms
//!   (`mode = none` / absent) are exclusions, not rank-mates —
//!   AC-R-2.10.5-9's `unmatched_budget` arm.
//! - **L4 ordering** — deterministic: metric value under the declaration's
//!   `direction`, ties by `finished_at` then the row key.
//! - **L5 comparability** — the binding's `comparable` flag must hold
//!   (exploratory-kind bindings report, never rank).

use std::collections::BTreeMap;

use hh_experiment::docs::LabDocs;
use hh_identity::idp::idp_id;
use hh_ledger::store::Store;
use hh_ontology::eval::{Direction, LatticeValue, MetricValueKind};
use hh_wire::json::Json;

use crate::audit::AuditRef;
use crate::catalogue::BundleStatus;
use crate::cells;
use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::store::ResultsStore;
use crate::watermark::WatermarkSet;

/// `LeaderboardDefinition{experiment_run_id, metric_ref, split?,
/// min_status, min_claimed_level?}` — the lab-internal view's
/// declaration. `metric_ref` names the ranking column; `split` narrows to
/// one split label; `min_status`/`min_claimed_level` set the L1 rungs
/// (default `validated`, no claimed-level gate).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardDefinition {
    /// The experiment run id (or a `design_ref` `build_cells` resolves).
    pub experiment_run_id: String,
    /// The ranking metric's registry ref.
    pub metric_ref: String,
    /// The split-label restriction (`None` = every split).
    pub split: Option<String>,
    /// The L1 admission rung.
    pub min_status: BundleStatus,
    /// The L1 reproducibility rung — a covering bundle must claim at
    /// least this level (`None` = no claimed-level gate; AC-R-2.10.5-3's
    /// `min_claimed_level = R1` arm).
    pub min_claimed_level: Option<hh_bundle::manifest::ReproLevel>,
}

impl LeaderboardDefinition {
    /// The ordinary definition — `validated` admission, all splits, no
    /// claimed-level gate.
    pub fn new(experiment_run_id: impl Into<String>, metric_ref: impl Into<String>) -> Self {
        LeaderboardDefinition {
            experiment_run_id: experiment_run_id.into(),
            metric_ref: metric_ref.into(),
            split: None,
            min_status: BundleStatus::Validated,
            min_claimed_level: None,
        }
    }
}

/// One ranked row — `rank` is within the row's L3 stratum (strata never
/// interleave).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardEntry {
    /// The within-stratum rank (1-based, ties share a rank — dense).
    pub rank: u64,
    /// The L3 stratum id (the `MatchSpec` canonical idp).
    pub stratum: String,
    /// The fixed row key's pinned id.
    pub key: String,
    /// The row version ranked.
    pub version_id: String,
    /// The cell coordinate.
    pub cell_id: String,
    /// The arm.
    pub arm_id: String,
    /// The seedless configuration coordinate.
    pub configuration_id: String,
    /// The ranked metric value (canonical `MetricValueKind` JSON).
    pub value: Json,
    /// `finished_at` — the L4 tie-break input, surfaced.
    pub finished_at: Option<String>,
    /// The row's head citation.
    pub audit_ref: AuditRef,
    /// The covering validated bundles (the L1 evidence).
    pub bundle_refs: Vec<String>,
    /// Advisory flags — annotate, never hide (§6.5 §2.4 L7's posture at
    /// the lab-internal gate): `budget_imbalanced` when the arm's
    /// comparand group closed `imbalanced`, `under_utilised` when the
    /// engine's E-4 re-check named the arm (AC-R-2.10.5-9's "imbalance is
    /// a flag, not an exclusion").
    pub flags: Vec<String>,
}

/// One gated-out candidate — `{key, version_id?, gate, reason}`.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardExclusion {
    /// The row key (or `cell:<id>` for a planned-but-unbound cell).
    pub key: String,
    /// The version observed (`None` when the cell bound no row).
    pub version_id: Option<String>,
    /// The gate that excluded it (`L1|L2|L3|L5`).
    pub gate: String,
    /// The typed reason.
    pub reason: String,
}

/// `LeaderboardSnapshot{experiment_run_id, metric_ref, entries[],
/// exclusions[], watermark_set, snapshot_id}` — `snapshot_id = view_hash`.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardSnapshot {
    /// The experiment run ranked.
    pub experiment_run_id: String,
    /// The ranking column.
    pub metric_ref: String,
    /// The ranked entries (stratum-major, rank order within).
    pub entries: Vec<LeaderboardEntry>,
    /// The gated-out candidates.
    pub exclusions: Vec<LeaderboardExclusion>,
    /// The served watermark set.
    pub watermark_set: WatermarkSet,
    /// `idp/1` over the snapshot preimage — the reproducibility anchor.
    pub snapshot_id: String,
}

impl LeaderboardSnapshot {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-leaderboard-snapshot/1")),
            ("experiment_run_id", Json::str(&self.experiment_run_id)),
            ("metric_ref", Json::str(&self.metric_ref)),
            (
                "entries",
                Json::Arr(
                    self.entries
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("rank", Json::Int(e.rank as i64)),
                                ("stratum", Json::str(&e.stratum)),
                                ("key", Json::str(&e.key)),
                                ("version_id", Json::str(&e.version_id)),
                                ("cell_id", Json::str(&e.cell_id)),
                                ("arm_id", Json::str(&e.arm_id)),
                                ("configuration_id", Json::str(&e.configuration_id)),
                                ("value", e.value.clone()),
                                (
                                    "finished_at",
                                    e.finished_at.as_ref().map_or(Json::Null, Json::str),
                                ),
                                ("audit_ref", e.audit_ref.to_json()),
                                (
                                    "bundle_refs",
                                    Json::Arr(e.bundle_refs.iter().map(Json::str).collect()),
                                ),
                                ("flags", Json::Arr(e.flags.iter().map(Json::str).collect())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "exclusions",
                Json::Arr(
                    self.exclusions
                        .iter()
                        .map(|x| {
                            Json::obj([
                                ("key", Json::str(&x.key)),
                                (
                                    "version_id",
                                    x.version_id.as_ref().map_or(Json::Null, Json::str),
                                ),
                                ("gate", Json::str(&x.gate)),
                                ("reason", Json::str(&x.reason)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("watermark_set", self.watermark_set.to_json()),
            ("snapshot_id", Json::str(&self.snapshot_id)),
        ])
    }
}

/// A totally-ordered rank key for a concrete `MetricValueKind` — the L4
/// value ordering before `direction` applies. Kind tags order
/// `bool < decimal < verdict < vector`; within a kind the natural order
/// (verdicts by lattice height `C < I < P < N`).
fn rank_key(v: &MetricValueKind) -> (u8, i64, String) {
    match v {
        MetricValueKind::Bool(b) => (0, *b as i64, String::new()),
        MetricValueKind::Decimal(d) => (1, *d, String::new()),
        MetricValueKind::Verdict(l) => {
            let n = match l {
                LatticeValue::C => 0,
                LatticeValue::I => 1,
                LatticeValue::P => 2,
                LatticeValue::N => 3,
            };
            (2, n, String::new())
        }
        MetricValueKind::Vector(j) => (3, 0, j.to_canonical_string()),
        MetricValueKind::Na(_) => (255, 0, String::new()),
    }
}

/// `leaderboard(definition, view)` — see module docs. `view` (`at`) pins
/// the watermark set the cell table is folded at.
pub fn leaderboard(
    results: &ResultsStore,
    store: &Store,
    docs: &LabDocs,
    definition: &LeaderboardDefinition,
    at: Option<&WatermarkSet>,
) -> Result<LeaderboardSnapshot, ResultsError> {
    let decl = hh_eval::catalogue::metric(&definition.metric_ref).ok_or_else(|| {
        ResultsError::LeaderboardRefused {
            detail: format!("no metric declaration for {}", definition.metric_ref),
        }
    })?;
    let (table, projected) =
        cells::build_cells_inner(results, store, docs, &definition.experiment_run_id, at)?;
    let catalogue = results.catalogue()?;
    // The arm → MatchSpec stratum map (through the declared spec — the
    // plan cells name arms; the spec carries the match declarations).
    let view = hh_experiment::view::ExperimentView::fold(
        store
            .envelopes(&table.experiment_run_id)
            .map_err(ResultsError::from)?,
    );
    let declared = view.declared.ok_or_else(|| ResultsError::DesignNotFound {
        reference: table.experiment_run_id.clone(),
    })?;
    let spec = docs
        .spec(&declared.experiment_id)
        .map_err(|e| ResultsError::Store {
            detail: format!("labdocs spec: {e:?}"),
        })?
        .ok_or_else(|| ResultsError::DesignNotFound {
            reference: declared.experiment_id.clone(),
        })?;
    let mut arm_strata: BTreeMap<String, Option<String>> = BTreeMap::new();
    for arm in &spec.arms {
        arm_strata.insert(
            arm.arm_id.clone(),
            arm.match_spec
                .as_ref()
                .filter(|ms| ms.mode != hh_budget::MatchMode::None)
                .map(|ms| {
                    idp_id(
                        "results_stratum",
                        ms.to_json().to_canonical_string().as_bytes(),
                    )
                }),
        );
    }
    // The engine's E-4 close record — `budget_match[]{arms, status}` and
    // `under_utilised[]`. The leaderboard consumes the verdict (never
    // re-derives it): `unmatched` unadmits (AC-R-2.10.5-9's
    // `unmatched_budget`), `imbalanced`/`under_utilised` flag the entry.
    let mut arm_match: BTreeMap<String, String> = BTreeMap::new();
    let mut arm_flags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(closed) = &view.closed {
        if let Some(Json::Arr(groups)) = closed.get("budget_match") {
            for g in groups {
                let status = g.get("status").and_then(Json::as_str).unwrap_or("matched");
                if let Some(Json::Arr(arms)) = g.get("arms") {
                    for a in arms.iter().filter_map(Json::as_str) {
                        arm_match.insert(a.to_string(), status.to_string());
                        if status == "imbalanced" {
                            arm_flags
                                .entry(a.to_string())
                                .or_default()
                                .push("budget_imbalanced".to_string());
                        }
                    }
                }
            }
        }
        if let Some(Json::Arr(arms)) = closed.get("under_utilised") {
            for a in arms.iter().filter_map(Json::as_str) {
                arm_flags
                    .entry(a.to_string())
                    .or_default()
                    .push("under_utilised".to_string());
            }
        }
    }

    let mut exclusions: Vec<LeaderboardExclusion> = Vec::new();
    // stratum → [(rank_key, finished_at, key, entry parts)]
    // stratum → [(projected row, rank value)] — rows are borrowed from
    // `projected` (nothing in the leaderboard is persisted).
    let mut admitted: BTreeMap<String, Vec<(&ResultsRow, Json)>> = BTreeMap::new();

    for cell in &table.cells {
        if let Some(split) = &definition.split {
            if &cell.split_label != split {
                continue;
            }
        }
        if cell.rows.is_empty() {
            if let Some(r) = &cell.na_reason {
                exclusions.push(LeaderboardExclusion {
                    key: format!("cell:{}", cell.cell_id),
                    version_id: None,
                    gate: "L2".into(),
                    reason: format!("planned-not-eligible:{r}"),
                });
            }
            continue;
        }
        for row_ref in &cell.rows {
            // The cell-table row refs name the projections `build_cells`
            // just derived — they may never have been recorded, so resolve
            // against the projected set, not the store (projection purity).
            let row = projected
                .get(&row_ref.version_id)
                .ok_or_else(|| ResultsError::Store {
                    detail: format!("cell row ref {} has no projected row", row_ref.version_id),
                })?;
            let key = row.key.key_id();
            // L2 eligibility — the cell-table verdict plus the metric's
            // declared applicability and a concrete value.
            if !row_ref.included {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L2".into(),
                    reason: row_ref
                        .excluded_reason
                        .clone()
                        .unwrap_or_else(|| "open".into()),
                });
                continue;
            }
            if let Some(reason) = applicability_failure(&decl, row) {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L2".into(),
                    reason,
                });
                continue;
            }
            let value = match row
                .cells
                .iter()
                .find(|c| c.metric_ref == definition.metric_ref)
                .filter(|c| c.is_concrete())
            {
                Some(c) => c.value.to_json(),
                None => {
                    exclusions.push(LeaderboardExclusion {
                        key,
                        version_id: Some(row_ref.version_id.clone()),
                        gate: "L2".into(),
                        reason: format!("no concrete value for {}", definition.metric_ref),
                    });
                    continue;
                }
            };
            // L5 comparability — the binding must declare the attempt
            // comparable.
            let comparable = row
                .experiment
                .as_ref()
                .and_then(|e| e.get("comparable"))
                .and_then(|c| match c {
                    Json::Bool(b) => Some(*b),
                    _ => None,
                })
                .unwrap_or(false);
            if !comparable {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L5".into(),
                    reason: "binding comparable = false".into(),
                });
                continue;
            }
            // L3 budget match — the arm's MatchSpec is the stratum
            // (`mode = none` / absent → `unmatched_budget`, never a
            // rank-mate); a comparand group the engine's close re-check
            // marked `unmatched` unadmits the same way.
            let stratum = match arm_strata.get(&cell.arm_id) {
                Some(Some(s)) => s.clone(),
                _ => {
                    exclusions.push(LeaderboardExclusion {
                        key,
                        version_id: Some(row_ref.version_id.clone()),
                        gate: "L3".into(),
                        reason: format!(
                            "unmatched_budget: arm {} carries no usable MatchSpec",
                            cell.arm_id
                        ),
                    });
                    continue;
                }
            };
            if arm_match.get(&cell.arm_id).map(String::as_str) == Some("unmatched") {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L3".into(),
                    reason: format!(
                        "unmatched_budget: comparand group closed unmatched (arm {})",
                        cell.arm_id
                    ),
                });
                continue;
            }
            // L1 bundle — a covering entry at `status ≥ min_status` and,
            // when set, `claimed_level ≥ min_claimed_level`.
            let covering: Vec<String> = catalogue
                .entries
                .iter()
                .filter(|e| {
                    e.subject_runs.iter().any(|r| r == &row.key.run_id)
                        && e.status >= definition.min_status
                        && definition
                            .min_claimed_level
                            .is_none_or(|m| e.claimed_level.is_some_and(|c| c >= m))
                })
                .map(|e| e.bundle_id.clone())
                .collect();
            if covering.is_empty() {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L1".into(),
                    reason: format!(
                        "no bundle at status ≥ {}{} covers {}",
                        definition.min_status.as_str(),
                        definition
                            .min_claimed_level
                            .map(|m| format!(", claimed ≥ {}", m.name()))
                            .unwrap_or_default(),
                        row.key.run_id
                    ),
                });
                continue;
            }
            admitted.entry(stratum).or_default().push((row, value));
        }
    }

    // L4 ordering — per stratum: value under `direction`, ties by
    // `finished_at` then key id. Dense ranks (ties share).
    let mut entries: Vec<LeaderboardEntry> = Vec::new();
    let mut strata: Vec<String> = admitted.keys().cloned().collect();
    strata.sort();
    for stratum in strata {
        let mut rows = admitted.remove(&stratum).unwrap_or_default();
        let higher = decl.direction == Direction::Higher;
        rows.sort_by(|a, b| {
            let ka = rank_key_value(&a.1);
            let kb = rank_key_value(&b.1);
            let ord = if higher { kb.cmp(&ka) } else { ka.cmp(&kb) };
            ord.then_with(|| a.0.outcome.finished_at.cmp(&b.0.outcome.finished_at))
                .then_with(|| a.0.key.key_id().cmp(&b.0.key.key_id()))
        });
        let mut rank = 0u64;
        let mut prev: Option<(u8, i64, String)> = None;
        for (row, value) in rows {
            let k = rank_key_value(&value);
            if prev.as_ref() != Some(&k) {
                rank += 1;
                prev = Some(k);
            }
            let covering: Vec<String> = catalogue
                .entries
                .iter()
                .filter(|e| {
                    e.subject_runs.iter().any(|r| r == &row.key.run_id)
                        && e.status >= definition.min_status
                        && definition
                            .min_claimed_level
                            .is_none_or(|m| e.claimed_level.is_some_and(|c| c >= m))
                })
                .map(|e| e.bundle_id.clone())
                .collect();
            let exp = row.experiment.clone().unwrap_or(Json::obj([]));
            let arm_id = exp
                .get("arm_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let flags = arm_flags.get(&arm_id).cloned().unwrap_or_default();
            entries.push(LeaderboardEntry {
                rank,
                stratum: stratum.clone(),
                key: row.key.key_id(),
                version_id: row.version_id.clone(),
                cell_id: exp
                    .get("cell_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                arm_id,
                configuration_id: row.coordinates.configuration_id.clone().unwrap_or_default(),
                value,
                finished_at: row.outcome.finished_at.clone(),
                audit_ref: row.audit.head.clone(),
                bundle_refs: covering,
                flags,
            });
        }
    }
    exclusions.sort_by(|a, b| {
        a.gate
            .cmp(&b.gate)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.reason.cmp(&b.reason))
    });

    let mut snap = LeaderboardSnapshot {
        experiment_run_id: table.experiment_run_id.clone(),
        metric_ref: definition.metric_ref.clone(),
        entries,
        exclusions,
        watermark_set: table.watermark_set.clone(),
        snapshot_id: String::new(),
    };
    let mut pre = snap.to_json();
    if let Json::Obj(ref mut m) = pre {
        m.remove("snapshot_id");
    }
    snap.snapshot_id = idp_id("results_view", pre.to_canonical_string().as_bytes());
    Ok(snap)
}

/// The L2 applicability arm — `applies_to_classes` (non-empty bound) and
/// `requires_observability` must cover the row. Returns the reason string
/// on failure.
fn applicability_failure(
    decl: &hh_ontology::compliance::MetricDeclaration,
    row: &ResultsRow,
) -> Option<String> {
    if !decl.applies_to_classes.is_empty()
        && !decl
            .applies_to_classes
            .iter()
            .any(|c| c.as_str() == row.coordinates.participant_class)
    {
        return Some(format!(
            "metric {} does not apply to class {}",
            decl.name, row.coordinates.participant_class
        ));
    }
    let mut obs: std::collections::BTreeSet<&str> = row
        .coordinates
        .observability_level
        .iter()
        .map(String::as_str)
        .collect();
    // `ledger` entails the lower levels (the full ledger carries events,
    // model I/O, end state — projection.rs applies the same rule).
    if obs.contains("ledger") {
        obs.extend(["events", "model_io", "end_state"]);
    }
    let missing: Vec<String> = decl
        .requires_observability
        .iter()
        .filter(|o| !obs.contains(o.as_str()))
        .map(|o| o.as_str().to_string())
        .collect();
    if !missing.is_empty() {
        return Some(format!(
            "metric {} requires observability {} — row declares [{}]",
            decl.name,
            missing.join(","),
            row.coordinates.observability_level.join(",")
        ));
    }
    None
}

/// `rank_key` over the canonical value JSON — decode back to the kind.
fn rank_key_value(j: &Json) -> (u8, i64, String) {
    match MetricValueKind::from_json(j) {
        Some(v) => rank_key(&v),
        None => (254, 0, j.to_canonical_string()),
    }
}
