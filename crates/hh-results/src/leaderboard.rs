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

/// `disclosure_policy{require_experiment, show_registered_arms,
/// min_replicates?, require_budget_match?}` — the C1 L6/L7 gates (§6.5
/// §4): `require_experiment` unadmits unbound rows as `undisclosed`;
/// `min_replicates` unadmits cells below the replicate floor as
/// `insufficient_replicates`; `require_budget_match` refuses arms whose
/// close record carries no `matched` verdict as `unmatched_budget`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisclosurePolicy {
    /// Rows with no experiment binding are `undisclosed` — they never
    /// reach the public view.
    pub require_experiment: bool,
    /// Whether registered-but-unpublished arms surface in the view
    /// (they surface as exclusions either way — this flag controls
    /// whether their cell list is named in `unadmitted`).
    pub show_registered_arms: bool,
    /// The L6 replicate floor: cells whose admitted replicate count is
    /// below it fail `insufficient_replicates`.
    pub min_replicates: Option<u64>,
    /// The L7 gate: every admitted tuple must sit in a `matched`
    /// `budget_match` group on the close record.
    pub require_budget_match: bool,
}

impl DisclosurePolicy {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "require_experiment".into(),
            Json::Bool(self.require_experiment),
        );
        m.insert(
            "show_registered_arms".into(),
            Json::Bool(self.show_registered_arms),
        );
        if let Some(n) = self.min_replicates {
            m.insert("min_replicates".into(), Json::Int(n as i64));
        }
        m.insert(
            "require_budget_match".into(),
            Json::Bool(self.require_budget_match),
        );
        Json::Obj(m)
    }

    /// Strict decode (absent members take the struct defaults).
    pub fn from_json(j: &Json) -> DisclosurePolicy {
        let Json::Obj(m) = j else {
            return DisclosurePolicy::default();
        };
        let b = |k: &str| {
            m.get(k)
                .map(|v| matches!(v, Json::Bool(true)))
                .unwrap_or(false)
        };
        DisclosurePolicy {
            require_experiment: b("require_experiment"),
            show_registered_arms: b("show_registered_arms"),
            min_replicates: m
                .get("min_replicates")
                .and_then(Json::as_int)
                .map(|n| n as u64),
            require_budget_match: b("require_budget_match"),
        }
    }
}

/// `LeaderboardDefinition{experiment_run_id, metric_ref, split_label?,
/// min_status, min_claimed_level?, name?, title?, suite_manifest_ref?,
/// scope?, columns[], rank_rule, strata[], disclosure_policy, readers[],
/// contamination_policy_ref?, provenance?}` — the canonical C1
/// definition record (§6.5 §4 row 1; the Stage-3 members keep their
/// spelling and the publication contract lands the rest).
/// `definition_id = idp/1` over the canonical bytes — the record is
/// self-addressing (CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardDefinition {
    /// The experiment run id (or a `design_ref` `build_cells` resolves).
    pub experiment_run_id: String,
    /// The ranking metric's registry ref.
    pub metric_ref: String,
    /// The split-label restriction (`None` = every split; canonical
    /// member `split_label`).
    pub split: Option<String>,
    /// The L1 admission rung.
    pub min_status: BundleStatus,
    /// The L1 reproducibility rung — a covering bundle must claim at
    /// least this level (`None` = no claimed-level gate; AC-R-2.10.5-3's
    /// `min_claimed_level = R1` arm).
    pub min_claimed_level: Option<hh_bundle::manifest::ReproLevel>,
    /// The public `name` — the registry name-history binding target.
    pub name: Option<String>,
    /// A human-facing title.
    pub title: Option<String>,
    /// The suite the board reads (`suite_manifest_ref`).
    pub suite_manifest_ref: Option<String>,
    /// `scope{participant_classes[], granularity?, model_scope?}` — the
    /// L8 admission scope (`participant_classes` non-empty bounds the
    /// row classes that may rank).
    pub scope: Json,
    /// Additional metric columns the entries carry.
    pub columns: Vec<String>,
    /// `rank_rule.interval_rule ∈ {overlap, ceteris_paribus, fixed}` —
    /// how interval overlap contributes to rank uncertainty (L9/A9).
    pub interval_rule: String,
    /// `strata[] ⊆ {participant_class, observability_level, budget_tier,
    /// architecture_class, region}` — declared stratification dims.
    pub strata: Vec<String>,
    /// The disclosure policy — the L6/L7 gates.
    pub disclosure_policy: DisclosurePolicy,
    /// `readers[]` — the publication's declared read set.
    pub readers: Vec<String>,
    /// `contamination_policy_ref?` — when set, a `contaminated`
    /// annotation on a contributing row trips the veto (`veto_tripped`).
    pub contamination_policy_ref: Option<String>,
    /// The definition's provenance claims.
    pub provenance: Option<Json>,
}

impl LeaderboardDefinition {
    /// The ordinary definition — `validated` admission, all splits, no
    /// claimed-level gate, the permissive disclosure policy.
    pub fn new(experiment_run_id: impl Into<String>, metric_ref: impl Into<String>) -> Self {
        LeaderboardDefinition {
            experiment_run_id: experiment_run_id.into(),
            metric_ref: metric_ref.into(),
            split: None,
            min_status: BundleStatus::Validated,
            min_claimed_level: None,
            name: None,
            title: None,
            suite_manifest_ref: None,
            scope: Json::Null,
            columns: Vec::new(),
            interval_rule: "fixed".to_string(),
            strata: Vec::new(),
            disclosure_policy: DisclosurePolicy::default(),
            readers: Vec::new(),
            contamination_policy_ref: None,
            provenance: None,
        }
    }

    /// Canonical `LeaderboardDefinition/1` JSON — what
    /// `define_leaderboard` persists and `leaderboard_definition`
    /// reads back.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("hh-leaderboard-def/1"));
        m.insert(
            "experiment_run_id".into(),
            Json::str(&self.experiment_run_id),
        );
        m.insert("metric_ref".into(), Json::str(&self.metric_ref));
        if let Some(s) = &self.split {
            m.insert("split_label".into(), Json::str(s));
        }
        m.insert("min_status".into(), Json::str(self.min_status.as_str()));
        if let Some(c) = self.min_claimed_level {
            m.insert("min_claimed_level".into(), Json::str(c.name()));
        }
        if let Some(n) = &self.name {
            m.insert("name".into(), Json::str(n));
        }
        if let Some(t) = &self.title {
            m.insert("title".into(), Json::str(t));
        }
        if let Some(s) = &self.suite_manifest_ref {
            m.insert("suite_manifest_ref".into(), Json::str(s));
        }
        if !matches!(self.scope, Json::Null) {
            m.insert("scope".into(), self.scope.clone());
        }
        if !self.columns.is_empty() {
            m.insert(
                "columns".into(),
                Json::Arr(self.columns.iter().map(Json::str).collect()),
            );
        }
        let mut rank_rule = BTreeMap::new();
        rank_rule.insert("metric_ref".into(), Json::str(&self.metric_ref));
        rank_rule.insert("interval_rule".into(), Json::str(&self.interval_rule));
        m.insert("rank_rule".into(), Json::Obj(rank_rule));
        if !self.strata.is_empty() {
            m.insert(
                "strata".into(),
                Json::Arr(self.strata.iter().map(Json::str).collect()),
            );
        }
        m.insert("disclosure_policy".into(), self.disclosure_policy.to_json());
        if !self.readers.is_empty() {
            m.insert(
                "readers".into(),
                Json::Arr(self.readers.iter().map(Json::str).collect()),
            );
        }
        if let Some(c) = &self.contamination_policy_ref {
            m.insert("contamination_policy_ref".into(), Json::str(c));
        }
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        Json::Obj(m)
    }

    /// Strict decode of the canonical record (absent members take the
    /// constructor defaults).
    pub fn from_json(j: &Json) -> Result<LeaderboardDefinition, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(ResultsError::Store {
                detail: "leaderboard definition not an object".into(),
            });
        };
        let mut def = LeaderboardDefinition::new(
            m.get("experiment_run_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            m.get("metric_ref")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        );
        def.split = m
            .get("split_label")
            .or_else(|| m.get("split"))
            .and_then(Json::as_str)
            .map(String::from);
        if let Some(s) = m.get("min_status").and_then(Json::as_str) {
            def.min_status = BundleStatus::parse(s).unwrap_or(BundleStatus::Validated);
        }
        if let Some(s) = m.get("min_claimed_level").and_then(Json::as_str) {
            def.min_claimed_level = hh_bundle::manifest::ReproLevel::parse(s);
        }
        def.name = m.get("name").and_then(Json::as_str).map(String::from);
        def.title = m.get("title").and_then(Json::as_str).map(String::from);
        def.suite_manifest_ref = m
            .get("suite_manifest_ref")
            .and_then(Json::as_str)
            .map(String::from);
        def.scope = m.get("scope").cloned().unwrap_or(Json::Null);
        let strs = |k: &str| -> Vec<String> {
            match m.get(k) {
                Some(Json::Arr(v)) => v
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            }
        };
        def.columns = strs("columns");
        def.strata = strs("strata");
        def.readers = strs("readers");
        if let Some(ir) = m
            .get("rank_rule")
            .and_then(|rr| rr.get("interval_rule"))
            .and_then(Json::as_str)
        {
            def.interval_rule = ir.to_string();
        }
        if let Some(dp) = m.get("disclosure_policy") {
            def.disclosure_policy = DisclosurePolicy::from_json(dp);
        }
        def.contamination_policy_ref = m
            .get("contamination_policy_ref")
            .and_then(Json::as_str)
            .map(String::from);
        def.provenance = m.get("provenance").cloned();
        Ok(def)
    }

    /// The canonical definition id — `idp/1` over the canonical record
    /// (`definition_id`; name history and snapshot refs key to it).
    pub fn definition_id(&self) -> String {
        idp_id(
            "leaderboard_definition",
            self.to_json().to_canonical_string().as_bytes(),
        )
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
    /// a flag, not an exclusion"), `retracted` /
    /// `member_revoked`/`suite_retired`/`evidence_missing` when the C1
    /// annotation overlay marks it (AC-R-2.10.5-4).
    pub flags: Vec<String>,
    /// Whether the entry was retracted by `retract_entry` — the entry
    /// stays listed (annotation, never removal).
    pub retracted: bool,
    /// `selection{analysis_record_ref, n_candidates?, rank?}` — the L9
    /// analysis-record selection mark when the row's annotation carries
    /// one (best-of-N stays auditable).
    pub selection: Option<Json>,
    /// `disclosure ∈ {published, restricted, unbound}` — the row's
    /// disclosure class under the definition's disclosure policy.
    pub disclosure: Option<String>,
}

impl LeaderboardEntry {
    /// Canonical JSON member for the snapshot's `entries[]`.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("rank".into(), Json::Int(self.rank as i64));
        m.insert("stratum".into(), Json::str(&self.stratum));
        m.insert("key".into(), Json::str(&self.key));
        m.insert("version_id".into(), Json::str(&self.version_id));
        m.insert("cell_id".into(), Json::str(&self.cell_id));
        m.insert("arm_id".into(), Json::str(&self.arm_id));
        m.insert("configuration_id".into(), Json::str(&self.configuration_id));
        m.insert("value".into(), self.value.clone());
        m.insert(
            "finished_at".into(),
            self.finished_at.as_ref().map_or(Json::Null, Json::str),
        );
        m.insert("audit_ref".into(), self.audit_ref.to_json());
        m.insert(
            "bundle_refs".into(),
            Json::Arr(self.bundle_refs.iter().map(Json::str).collect()),
        );
        m.insert(
            "flags".into(),
            Json::Arr(self.flags.iter().map(Json::str).collect()),
        );
        if self.retracted {
            m.insert("retracted".into(), Json::Bool(true));
        }
        if let Some(s) = &self.selection {
            m.insert("selection".into(), s.clone());
        }
        if let Some(d) = &self.disclosure {
            m.insert("disclosure".into(), Json::str(d));
        }
        Json::Obj(m)
    }

    /// Strict decode of the canonical entry member.
    pub fn from_json(j: &Json) -> Option<LeaderboardEntry> {
        let Json::Obj(m) = j else { return None };
        let s = |k: &str| m.get(k).and_then(Json::as_str).map(String::from);
        let strs = |k: &str| -> Vec<String> {
            match m.get(k) {
                Some(Json::Arr(v)) => v
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            }
        };
        Some(LeaderboardEntry {
            rank: m.get("rank").and_then(Json::as_int)? as u64,
            stratum: s("stratum")?,
            key: s("key")?,
            version_id: s("version_id")?,
            cell_id: s("cell_id")?,
            arm_id: s("arm_id")?,
            configuration_id: s("configuration_id")?,
            value: m.get("value").cloned().unwrap_or(Json::Null),
            finished_at: s("finished_at"),
            audit_ref: AuditRef::from_json(m.get("audit_ref")?)?,
            bundle_refs: strs("bundle_refs"),
            flags: strs("flags"),
            retracted: matches!(m.get("retracted"), Some(Json::Bool(true))),
            selection: m.get("selection").cloned(),
            disclosure: s("disclosure"),
        })
    }
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

/// `LeaderboardSnapshot{definition_ref?, experiment_run_id, metric_ref,
/// entries[], exclusions[] (canonical `unadmitted[]`), watermark_set,
/// disclosure_summary?, contamination_state?, snapshot_id}` —
/// `snapshot_id = view_hash` (§6.5 §5.4 row: the reproducibility anchor —
/// persisted snapshots re-project to the identical bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardSnapshot {
    /// The experiment run ranked.
    pub experiment_run_id: String,
    /// The ranking column.
    pub metric_ref: String,
    /// The canonical definition this snapshot serves (`definition_id`
    /// when the definition was registered through `define_leaderboard`).
    pub definition_ref: Option<String>,
    /// The ranked entries (stratum-major, rank order within).
    pub entries: Vec<LeaderboardEntry>,
    /// The gated-out candidates.
    pub exclusions: Vec<LeaderboardExclusion>,
    /// The served watermark set.
    pub watermark_set: WatermarkSet,
    /// The `DisclosureSummary` JSON the snapshot was generated under
    /// (the selection/disclosure disclosure — §6.5 §4's snapshot member).
    pub disclosure_summary: Option<Json>,
    /// `contamination_state{policy_ref?, vetoed[]}` — the contamination
    /// posture the snapshot admitted under.
    pub contamination_state: Option<Json>,
    /// `idp/1` over the snapshot preimage — the reproducibility anchor.
    pub snapshot_id: String,
}

impl LeaderboardSnapshot {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("hh-leaderboard-snapshot/1"));
        if let Some(d) = &self.definition_ref {
            m.insert("definition_ref".into(), Json::str(d));
        }
        m.insert(
            "experiment_run_id".into(),
            Json::str(&self.experiment_run_id),
        );
        m.insert("metric_ref".into(), Json::str(&self.metric_ref));
        m.insert(
            "entries".into(),
            Json::Arr(self.entries.iter().map(|e| e.to_json()).collect()),
        );
        m.insert(
            "unadmitted".into(),
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
        );
        m.insert("watermark_set".into(), self.watermark_set.to_json());
        if let Some(d) = &self.disclosure_summary {
            m.insert("disclosure_summary".into(), d.clone());
        }
        if let Some(c) = &self.contamination_state {
            m.insert("contamination_state".into(), c.clone());
        }
        m.insert("snapshot_id".into(), Json::str(&self.snapshot_id));
        Json::Obj(m)
    }

    /// Strict decode of a persisted snapshot.
    pub fn from_json(j: &Json) -> Option<LeaderboardSnapshot> {
        let Json::Obj(m) = j else { return None };
        let s = |k: &str| m.get(k).and_then(Json::as_str).map(String::from);
        let entries = match m.get("entries") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(LeaderboardEntry::from_json)
                .collect(),
            _ => Vec::new(),
        };
        let mut exclusions = Vec::new();
        if let Some(Json::Arr(items)) = m.get("unadmitted").or_else(|| m.get("exclusions")) {
            for i in items {
                let Json::Obj(x) = i else { continue };
                let Some(key) = x.get("key").and_then(Json::as_str) else {
                    continue;
                };
                exclusions.push(LeaderboardExclusion {
                    key: key.to_string(),
                    version_id: x.get("version_id").and_then(Json::as_str).map(String::from),
                    gate: x
                        .get("gate")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    reason: x
                        .get("reason")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
        Some(LeaderboardSnapshot {
            experiment_run_id: s("experiment_run_id")?,
            metric_ref: s("metric_ref")?,
            definition_ref: s("definition_ref"),
            entries,
            exclusions,
            watermark_set: m.get("watermark_set").and_then(WatermarkSet::from_json)?,
            disclosure_summary: m.get("disclosure_summary").cloned(),
            contamination_state: m.get("contamination_state").cloned(),
            snapshot_id: s("snapshot_id")?,
        })
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
    let declared = view
        .declared
        .clone()
        .ok_or_else(|| ResultsError::DesignNotFound {
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
            // L6 disclosure — under `require_experiment` an unbound row
            // is `undisclosed`, never ranked (AC-R-2.10.5-8). The check
            // runs ahead of the L5 comparable flag: a row with no
            // experiment member at all reports `undisclosed`, not the
            // downstream symptom `binding comparable = false`.
            if definition.disclosure_policy.require_experiment && row.experiment.is_none() {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L6".into(),
                    reason: "undisclosed".into(),
                });
                continue;
            }
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
            // L7 matched budget — under `require_budget_match` every
            // admitted tuple must sit in a `matched` comparand group
            // (absent group = `unmatched_budget`, never silently admitted).
            if definition.disclosure_policy.require_budget_match
                && arm_match.get(&cell.arm_id).map(String::as_str) != Some("matched")
            {
                exclusions.push(LeaderboardExclusion {
                    key,
                    version_id: Some(row_ref.version_id.clone()),
                    gate: "L7".into(),
                    reason: format!(
                        "unmatched_budget: arm {} carries no matched comparand verdict",
                        cell.arm_id
                    ),
                });
                continue;
            }
            // L6 replicate floor — the arm's bound-replicate count below
            // `min_replicates` fails `insufficient_replicates`.
            if let Some(floor) = definition.disclosure_policy.min_replicates {
                let replicates: std::collections::BTreeSet<u32> = view
                    .plans
                    .values()
                    .filter(|p| p.arm_id == cell.arm_id)
                    .filter(|p| {
                        p.attempts
                            .iter()
                            .any(|a| a.run_id == row.key.run_id || a.bound)
                    })
                    .map(|p| p.replicate_index)
                    .collect();
                if (replicates.len() as u64) < floor {
                    exclusions.push(LeaderboardExclusion {
                        key,
                        version_id: Some(row_ref.version_id.clone()),
                        gate: "L6".into(),
                        reason: format!(
                            "insufficient_replicates: arm {} binds {} < floor {}",
                            cell.arm_id,
                            replicates.len(),
                            floor
                        ),
                    });
                    continue;
                }
            }
            // L8 scope — `participant_classes[]` bounds the classes that
            // may rank (`class_excluded`).
            if let Some(Json::Arr(classes)) = definition.scope.get("participant_classes") {
                if !classes.is_empty()
                    && !classes
                        .iter()
                        .filter_map(Json::as_str)
                        .any(|c| c == row.coordinates.participant_class)
                {
                    exclusions.push(LeaderboardExclusion {
                        key,
                        version_id: Some(row_ref.version_id.clone()),
                        gate: "L8".into(),
                        reason: format!(
                            "class_excluded: {} ∉ scope.participant_classes",
                            row.coordinates.participant_class
                        ),
                    });
                    continue;
                }
            }
            // L8 contamination veto — under a declared
            // `contamination_policy_ref`, a `contaminated` annotation
            // trips the veto (`veto_tripped`, never silent).
            if definition.contamination_policy_ref.is_some() {
                let contaminated = crate::annotations::annotate_row(results, store, row)
                    .flags
                    .iter()
                    .any(|f| f == "contaminated" || f.starts_with("contaminated:"));
                if contaminated {
                    exclusions.push(LeaderboardExclusion {
                        key,
                        version_id: Some(row_ref.version_id.clone()),
                        gate: "L8".into(),
                        reason: "veto_tripped: contaminated".into(),
                    });
                    continue;
                }
            }
            // L1 bundle — a covering entry at `status ≥ min_status` and,
            // when set, `claimed_level ≥ min_claimed_level`.
            let covered: Vec<&crate::catalogue::BundleCatalogueEntry> = catalogue
                .entries
                .iter()
                .filter(|e| e.subject_runs.iter().any(|r| r == &row.key.run_id))
                .collect();
            let covering: Vec<String> = covered
                .iter()
                .filter(|e| {
                    e.status.satisfies(definition.min_status)
                        && definition
                            .min_claimed_level
                            .is_none_or(|m| e.claimed_level.is_some_and(|c| c >= m))
                })
                .map(|e| e.bundle_id.clone())
                .collect();
            if covering.is_empty() {
                // Annotate, never hide (AC-R-2.10.5-4): a row whose
                // covering evidence all reached a lifecycle terminal
                // (`superseded`/`retracted`) keeps its entry — the
                // `member_revoked` flag marks it below. Revocation is an
                // overlay, never an admission change.
                let revoked_cover =
                    !covered.is_empty() && covered.iter().all(|e| e.status.is_terminal());
                if !revoked_cover {
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
            }
            admitted.entry(stratum).or_default().push((row, value));
        }
    }

    // The C1 overlays — retractions (annotate, never remove) and the
    // analysis-record selection marks (L9) read off the experiment run.
    let retractions = load_retractions(results, &definition.definition_id());
    let selections = selection_marks(store, &table.experiment_run_id);
    let restricted_arms = restricted_arms(&view, &spec, &catalogue);

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
            // `bundle_refs` names the satisfying cover; when the
            // cover is all-terminal the entry names the bundles it was
            // revoked on (the record, not just the flag).
            let covered: Vec<&crate::catalogue::BundleCatalogueEntry> = catalogue
                .entries
                .iter()
                .filter(|e| e.subject_runs.iter().any(|r| r == &row.key.run_id))
                .collect();
            let covering: Vec<String> = {
                let ok: Vec<String> = covered
                    .iter()
                    .filter(|e| {
                        e.status.satisfies(definition.min_status)
                            && definition
                                .min_claimed_level
                                .is_none_or(|m| e.claimed_level.is_some_and(|c| c >= m))
                    })
                    .map(|e| e.bundle_id.clone())
                    .collect();
                if ok.is_empty() {
                    covered.iter().map(|e| e.bundle_id.clone()).collect()
                } else {
                    ok
                }
            };
            let exp = row.experiment.clone().unwrap_or(Json::obj([]));
            let arm_id = exp
                .get("arm_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let mut flags = arm_flags.get(&arm_id).cloned().unwrap_or_default();
            // Annotate-never-hide overlays (AC-R-2.10.5-4): retractions
            // and revoked/retired/gc'd evidence mark the entry — the row
            // stays listed, `verify_row` still passes.
            let configuration_id = row.coordinates.configuration_id.clone().unwrap_or_default();
            let retracted = retractions.contains_key(&configuration_id)
                || retractions.contains_key(&row.key.key_id());
            if retracted {
                flags.push("retracted".to_string());
            }
            let mut flags_full = flags;
            let ann = crate::annotations::annotate_row(results, store, row);
            for f in &ann.flags {
                if f == "bundle_retracted" || f.starts_with("bundle_superseded") {
                    flags_full.push("member_revoked".to_string());
                }
                // `suite_retired` rides the entry verbatim — the C1
                // overlay mark the AC-R-2.10.5-4 diff reports as a flag
                // change, never a removal.
                if f == "suite_retired" {
                    flags_full.push("suite_retired".to_string());
                }
            }
            let evidence_missing = row
                .cells
                .iter()
                .flat_map(|c| c.evidence.iter())
                .flat_map(|a| a.content_refs.iter())
                .any(|r| !matches!(store.blob_status(r), hh_ledger::BlobStatus::Present));
            if evidence_missing {
                flags_full.push("evidence_missing".to_string());
            }
            flags_full.sort();
            flags_full.dedup();
            let selection = selections.get(&configuration_id).cloned();
            let disclosure = if row.experiment.is_none() {
                Some("unbound".to_string())
            } else if restricted_arms.contains(&arm_id) {
                Some("restricted".to_string())
            } else {
                Some("published".to_string())
            };
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
                configuration_id,
                value,
                finished_at: row.outcome.finished_at.clone(),
                audit_ref: row.audit.head.clone(),
                bundle_refs: covering,
                flags: flags_full,
                retracted,
                selection,
                disclosure,
            });
        }
    }
    exclusions.sort_by(|a, b| {
        a.gate
            .cmp(&b.gate)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.reason.cmp(&b.reason))
    });

    let disclosure_summary = crate::disclosure::disclosure_summary(&view, &spec, &catalogue);
    let contamination_state = definition.contamination_policy_ref.as_ref().map(|r| {
        Json::obj([
            ("policy_ref", Json::str(r)),
            (
                "vetoed",
                Json::Arr(
                    exclusions
                        .iter()
                        .filter(|x| x.reason.starts_with("veto_tripped"))
                        .map(|x| Json::str(&x.key))
                        .collect(),
                ),
            ),
        ])
    });

    let mut snap = LeaderboardSnapshot {
        experiment_run_id: table.experiment_run_id.clone(),
        metric_ref: definition.metric_ref.clone(),
        definition_ref: Some(definition.definition_id()),
        entries,
        exclusions,
        watermark_set: table.watermark_set.clone(),
        disclosure_summary: Some(disclosure_summary.to_json()),
        contamination_state,
        snapshot_id: String::new(),
    };
    let mut pre = snap.to_json();
    if let Json::Obj(ref mut m) = pre {
        m.remove("snapshot_id");
    }
    snap.snapshot_id = idp_id("results_view", pre.to_canonical_string().as_bytes());
    Ok(snap)
}

/// The arm ids whose covering bundles are all restricted (the
/// `restricted` disclosure class — shared by entries and the summary).
fn restricted_arms(
    view: &hh_experiment::view::ExperimentView,
    spec: &hh_lab::experiment::ExperimentSpec,
    catalogue: &crate::catalogue::BundleCatalogue,
) -> std::collections::BTreeSet<String> {
    let summary = crate::disclosure::disclosure_summary(view, spec, catalogue);
    // An arm counts as restricted when every covering bundle bounds its
    // reader set — the same derivation the summary counts.
    let mut arm_runs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in view.plans.values() {
        for a in &p.attempts {
            arm_runs
                .entry(p.arm_id.clone())
                .or_default()
                .push(a.run_id.clone());
        }
    }
    let mut restricted = std::collections::BTreeSet::new();
    for arm in &spec.arms {
        let runs = arm_runs.get(&arm.arm_id).cloned().unwrap_or_default();
        let covering: Vec<&crate::catalogue::BundleCatalogueEntry> = catalogue
            .entries
            .iter()
            .filter(|e| e.subject_runs.iter().any(|r| runs.contains(r)))
            .collect();
        if !covering.is_empty()
            && covering.iter().all(|e| {
                !e.readers.is_empty() && !e.readers.iter().any(|r| r == "public" || r == "*")
            })
        {
            restricted.insert(arm.arm_id.clone());
        }
    }
    let _ = summary; // summary is recomputed below — this helper counts arms only.
    restricted
}

/// `configuration_id → selection{analysis_record_ref, n_candidates,
/// rank?}` — the L9 analysis-record selection marks read off
/// `measurement.analysis.recorded` payloads (`selection{…}` naming the
/// chosen configuration).
fn selection_marks(store: &Store, experiment_run_id: &str) -> BTreeMap<String, Json> {
    let mut out = BTreeMap::new();
    let Ok(events) = store.envelopes(experiment_run_id) else {
        return out;
    };
    for e in events {
        if e.class != "measurement.analysis.recorded" {
            continue;
        }
        let Some(sel) = e
            .payload
            .get("selection")
            .or_else(|| e.payload.get("ranking").and_then(|r| r.get("selection")))
        else {
            continue;
        };
        let chosen = sel
            .get("chosen")
            .or_else(|| sel.get("choice"))
            .or_else(|| sel.get("configuration_id"))
            .and_then(Json::as_str);
        let Some(chosen) = chosen else { continue };
        let mut mark = BTreeMap::new();
        mark.insert("analysis_record_ref".into(), Json::str(&e.event_id));
        if let Some(n) = sel
            .get("n_candidates")
            .or_else(|| sel.get("n_candidates_registered"))
            .and_then(Json::as_int)
        {
            mark.insert("n_candidates".into(), Json::Int(n));
        }
        if let Some(r) = sel.get("rank").and_then(Json::as_int) {
            mark.insert("rank".into(), Json::Int(r));
        }
        out.insert(chosen.to_string(), Json::Obj(mark));
    }
    out
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

// ── C1 ops — definitions, snapshots, diff, retract, publish ─────────────
//
// The C1 surface (§6.5 §4; R-2.10.5¹): `define_leaderboard` persists the
// canonical definition + a name-history entry; `snapshot_leaderboard`
// retains the `view_hash`-bound snapshot; `diff_snapshots` reports
// additions/rank moves/flag changes (never a silent removal — a retracted
// or invalidated entry stays listed with new flags); `verify_snapshot`
// re-projects at the snapshot's watermark set and compares bytes;
// `retract_entry` appends `measurement.leaderboard.entry_retracted` and
// records the mark; `publish_leaderboard` lowers the snapshot to a
// `measurement.leaderboard.published` row whose `pinned_addresses[]`
// retention-pins every cited evidence address against `gc` (the ledger's
// pin set names any audit-field member carrying the address — CF-347).

fn lb_dir(results: &ResultsStore) -> std::path::PathBuf {
    results.root().join("leaderboards")
}

fn lb_write(path: &std::path::Path, j: &Json) -> Result<(), ResultsError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| ResultsError::Store {
            detail: format!("create dir: {e}"),
        })?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, j.to_canonical_string()).map_err(|e| ResultsError::Store {
        detail: format!("write {}: {e}", path.display()),
    })?;
    std::fs::rename(&tmp, path).map_err(|e| ResultsError::Store {
        detail: format!("rename {}: {e}", path.display()),
    })
}

fn lb_read(path: &std::path::Path) -> Result<Json, ResultsError> {
    let text = std::fs::read_to_string(path).map_err(|e| ResultsError::Store {
        detail: format!("read {}: {e}", path.display()),
    })?;
    hh_wire::json::parse(&text).map_err(|e| ResultsError::Store {
        detail: format!("parse {}: {e:?}", path.display()),
    })
}

fn safe_id(id: &str) -> String {
    id.replace([':', '/'], "_")
}

fn defs_dir(results: &ResultsStore) -> std::path::PathBuf {
    lb_dir(results).join("defs")
}

fn def_path(results: &ResultsStore, definition_ref: &str) -> std::path::PathBuf {
    defs_dir(results).join(format!("{}.json", safe_id(definition_ref)))
}

fn names_dir(results: &ResultsStore) -> std::path::PathBuf {
    lb_dir(results).join("names")
}

fn name_path(results: &ResultsStore, name: &str) -> std::path::PathBuf {
    names_dir(results).join(format!("{}.json", safe_id(name)))
}

fn snaps_dir(results: &ResultsStore) -> std::path::PathBuf {
    lb_dir(results).join("snapshots")
}

fn snap_path(results: &ResultsStore, snapshot_id: &str) -> std::path::PathBuf {
    snaps_dir(results).join(format!("{}.json", safe_id(snapshot_id)))
}

fn snap_index_path(results: &ResultsStore, definition_ref: &str) -> std::path::PathBuf {
    snaps_dir(results)
        .join("index")
        .join(format!("{}.json", safe_id(definition_ref)))
}

fn retractions_path(results: &ResultsStore, definition_ref: &str) -> std::path::PathBuf {
    lb_dir(results)
        .join("retractions")
        .join(format!("{}.json", safe_id(definition_ref)))
}

fn published_dir(results: &ResultsStore) -> std::path::PathBuf {
    lb_dir(results).join("published")
}

fn published_path(results: &ResultsStore, snapshot_id: &str) -> std::path::PathBuf {
    published_dir(results).join(format!("{}.json", safe_id(snapshot_id)))
}

/// `configuration_id|key → {reason_ref, authority, at}` — the persisted
/// retraction marks one definition carries.
fn load_retractions(results: &ResultsStore, definition_ref: &str) -> BTreeMap<String, Json> {
    let path = retractions_path(results, definition_ref);
    if !path.exists() {
        return BTreeMap::new();
    }
    let Ok(Json::Obj(m)) = lb_read(&path) else {
        return BTreeMap::new();
    };
    match m.get("entries") {
        Some(Json::Obj(e)) => e.clone(),
        _ => BTreeMap::new(),
    }
}

/// `diff_snapshots` verdict — `{entries_added[], entries_removed[],
/// rank_moves[], flag_changes[], watermark_delta}` (§6.5 §4: `removed`
/// carries only keys that genuinely vanished — retraction and invalidation
/// land in `flag_changes`, never here).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapshotDiff {
    /// Configuration ids admitted in `b` but absent from `a`.
    pub entries_added: Vec<String>,
    /// Configuration ids in `a` with no entry in `b` — under the
    /// annotate-never-hide rule this is always empty; a non-empty list
    /// is itself a finding (an entry vanished unannotated).
    pub entries_removed: Vec<String>,
    /// `(configuration_id, from_rank, to_rank)` for keys whose rank moved.
    pub rank_moves: Vec<(String, u64, u64)>,
    /// `(configuration_id, added_flags[], removed_flags[])`.
    pub flag_changes: Vec<(String, Vec<String>, Vec<String>)>,
    /// `(run_id, a_seq, b_seq)` watermark movement.
    pub watermark_delta: Vec<(String, u64, u64)>,
}

impl SnapshotDiff {
    /// Canonical JSON — the `publish`-time diff artifact.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "entries_added",
                Json::Arr(self.entries_added.iter().map(Json::str).collect()),
            ),
            (
                "entries_removed",
                Json::Arr(self.entries_removed.iter().map(Json::str).collect()),
            ),
            (
                "rank_moves",
                Json::Arr(
                    self.rank_moves
                        .iter()
                        .map(|(c, f, t)| {
                            Json::obj([
                                ("configuration_id", Json::str(c)),
                                ("from_rank", Json::Int(*f as i64)),
                                ("to_rank", Json::Int(*t as i64)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "flag_changes",
                Json::Arr(
                    self.flag_changes
                        .iter()
                        .map(|(c, add, rm)| {
                            Json::obj([
                                ("configuration_id", Json::str(c)),
                                (
                                    "added_flags",
                                    Json::Arr(add.iter().map(Json::str).collect()),
                                ),
                                (
                                    "removed_flags",
                                    Json::Arr(rm.iter().map(Json::str).collect()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "watermark_delta",
                Json::Arr(
                    self.watermark_delta
                        .iter()
                        .map(|(r, a, b)| {
                            Json::obj([
                                ("run_id", Json::str(r)),
                                ("from_seq", Json::Int(*a as i64)),
                                ("to_seq", Json::Int(*b as i64)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `publish_leaderboard`'s product.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedLeaderboard {
    /// The retained snapshot id.
    pub snapshot_id: String,
    /// `idp/1` over the published view bytes (`view_hash`).
    pub view_hash: String,
    /// The `measurement.leaderboard.published` event id.
    pub event_ref: String,
    /// The addresses the publication pins against `gc`.
    pub pinned_addresses: Vec<String>,
}

impl ResultsStore {
    /// `define_leaderboard(definition) → definition_id` — persist the
    /// canonical record at `leaderboards/defs/<definition_id>.json` and
    /// append the `name → definition_id` history entry (name history is
    /// append-only: a re-registered name lists every definition id in
    /// order).
    pub fn define_leaderboard(
        &self,
        definition: &LeaderboardDefinition,
    ) -> Result<String, ResultsError> {
        let id = definition.definition_id();
        lb_write(&def_path(self, &id), &definition.to_json())?;
        if let Some(name) = &definition.name {
            let path = name_path(self, name);
            let mut ids: Vec<String> = if path.exists() {
                match lb_read(&path) {
                    Ok(Json::Obj(m)) => match m.get("history") {
                        Some(Json::Arr(v)) => v
                            .iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect(),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            if !ids.contains(&id) {
                ids.push(id.clone());
            }
            lb_write(
                &path,
                &Json::obj([
                    ("name", Json::str(name)),
                    ("history", Json::Arr(ids.iter().map(Json::str).collect())),
                ]),
            )?;
        }
        Ok(id)
    }

    /// `leaderboard_definition(definition_ref)` — the persisted record.
    pub fn leaderboard_definition(
        &self,
        definition_ref: &str,
    ) -> Result<LeaderboardDefinition, ResultsError> {
        let path = def_path(self, definition_ref);
        if !path.exists() {
            return Err(ResultsError::Store {
                detail: format!("no leaderboard definition {definition_ref}"),
            });
        }
        LeaderboardDefinition::from_json(&lb_read(&path)?)
    }

    /// `leaderboard_definitions(name)` — the name-history list (every
    /// definition id the name has bound, oldest first).
    pub fn leaderboard_definitions(&self, name: &str) -> Vec<String> {
        let path = name_path(self, name);
        if !path.exists() {
            return Vec::new();
        }
        match lb_read(&path) {
            Ok(Json::Obj(m)) => match m.get("history") {
                Some(Json::Arr(v)) => v
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// `snapshot_leaderboard(definition, at?)` — compute the view, stamp
    /// `definition_ref`, persist the retained snapshot and append its id
    /// to the definition's snapshot index. Idempotent on `snapshot_id`.
    pub fn snapshot_leaderboard(
        &self,
        store: &Store,
        docs: &LabDocs,
        definition: &LeaderboardDefinition,
        at: Option<&WatermarkSet>,
    ) -> Result<LeaderboardSnapshot, ResultsError> {
        let snap = leaderboard(self, store, docs, definition, at)?;
        lb_write(&snap_path(self, &snap.snapshot_id), &snap.to_json())?;
        let idx_path = snap_index_path(self, &snap.snapshot_id);
        let _ = idx_path; // index is per-definition:
        let def_ref = definition.definition_id();
        let idx_path = snap_index_path(self, &def_ref);
        let mut ids: Vec<String> = if idx_path.exists() {
            match lb_read(&idx_path) {
                Ok(Json::Obj(m)) => match m.get("snapshot_ids") {
                    Some(Json::Arr(v)) => v
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if !ids.contains(&snap.snapshot_id) {
            ids.push(snap.snapshot_id.clone());
        }
        lb_write(
            &idx_path,
            &Json::obj([
                ("definition_ref", Json::str(&def_ref)),
                (
                    "snapshot_ids",
                    Json::Arr(ids.iter().map(Json::str).collect()),
                ),
            ]),
        )?;
        Ok(snap)
    }

    /// `leaderboard_snapshots(definition_ref)` — the retained snapshot
    /// ids, oldest first.
    pub fn leaderboard_snapshots(&self, definition_ref: &str) -> Vec<String> {
        let path = snap_index_path(self, definition_ref);
        if !path.exists() {
            return Vec::new();
        }
        match lb_read(&path) {
            Ok(Json::Obj(m)) => match m.get("snapshot_ids") {
                Some(Json::Arr(v)) => v
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// `load_snapshot(snapshot_id)` — a retained snapshot's bytes.
    pub fn load_snapshot(&self, snapshot_id: &str) -> Result<LeaderboardSnapshot, ResultsError> {
        let path = snap_path(self, snapshot_id);
        if !path.exists() {
            return Err(ResultsError::Store {
                detail: format!("no retained snapshot {snapshot_id}"),
            });
        }
        LeaderboardSnapshot::from_json(&lb_read(&path)?).ok_or_else(|| ResultsError::Store {
            detail: format!("snapshot {snapshot_id} malformed"),
        })
    }

    /// `diff_snapshots(a, b)` — the snapshot-diff view. Entries key by
    /// `configuration_id`; flag changes cover every overlay mark
    /// (retraction, member revocation, evidence state) — the diff never
    /// reports an annotation as a removal (AC-R-2.10.5-4).
    pub fn diff_snapshots(&self, a: &str, b: &str) -> Result<SnapshotDiff, ResultsError> {
        let sa = self.load_snapshot(a)?;
        let sb = self.load_snapshot(b)?;
        // Entries match on the row `key` — a leaderboard may list two
        // rows under one `configuration_id` (distinct attempts of one
        // configuration), and keying the diff on the configuration
        // would shadow one of them.
        let ea: BTreeMap<&str, &LeaderboardEntry> =
            sa.entries.iter().map(|e| (e.key.as_str(), e)).collect();
        let eb: BTreeMap<&str, &LeaderboardEntry> =
            sb.entries.iter().map(|e| (e.key.as_str(), e)).collect();
        let mut diff = SnapshotDiff::default();
        for (k, e) in &eb {
            match ea.get(k) {
                None => diff.entries_added.push(e.configuration_id.clone()),
                Some(old) => {
                    if old.rank != e.rank {
                        diff.rank_moves
                            .push((e.configuration_id.clone(), old.rank, e.rank));
                    }
                    let added: Vec<String> = e
                        .flags
                        .iter()
                        .filter(|f| !old.flags.contains(*f))
                        .cloned()
                        .collect();
                    let removed: Vec<String> = old
                        .flags
                        .iter()
                        .filter(|f| !e.flags.contains(*f))
                        .cloned()
                        .collect();
                    if e.retracted != old.retracted || !added.is_empty() || !removed.is_empty() {
                        let mut add = added;
                        if e.retracted && !old.retracted {
                            add.push("retracted".to_string());
                        }
                        let mut rm = removed;
                        if old.retracted && !e.retracted {
                            rm.push("retracted".to_string());
                        }
                        diff.flag_changes
                            .push((e.configuration_id.clone(), add, rm));
                    }
                }
            }
        }
        for (k, e) in &ea {
            if !eb.contains_key(k) {
                diff.entries_removed.push(e.configuration_id.clone());
            }
        }
        let wa = &sa.watermark_set.runs;
        let wb = &sb.watermark_set.runs;
        for (r, sb_seq) in wb {
            let a_seq = wa.get(r).copied().unwrap_or(0);
            if a_seq != *sb_seq {
                diff.watermark_delta.push((r.clone(), a_seq, *sb_seq));
            }
        }
        diff.entries_added.sort();
        diff.entries_removed.sort();
        diff.rank_moves.sort();
        diff.flag_changes.sort();
        diff.watermark_delta.sort();
        Ok(diff)
    }

    /// `verify_snapshot(snapshot_id)` — re-project the serving view at
    /// the snapshot's own watermark set and compare `snapshot_id`. The
    /// retained bytes are never trusted; recomputation is the check.
    pub fn verify_snapshot(
        &self,
        store: &Store,
        docs: &LabDocs,
        snapshot_id: &str,
    ) -> Result<crate::store::VerifyVerdict, ResultsError> {
        let snap = self.load_snapshot(snapshot_id)?;
        let def_ref = snap
            .definition_ref
            .clone()
            .ok_or_else(|| ResultsError::Store {
                detail: format!("snapshot {snapshot_id} carries no definition_ref"),
            })?;
        let def = self.leaderboard_definition(&def_ref)?;
        let recomputed = leaderboard(self, store, docs, &def, Some(&snap.watermark_set))?;
        if recomputed.to_json() == snap.to_json() {
            Ok(crate::store::VerifyVerdict::Ok)
        } else {
            Ok(crate::store::VerifyVerdict::Mismatch {
                recomputed: recomputed.snapshot_id,
            })
        }
    }

    /// `retract_entry(definition_ref, configuration_id, reason_ref,
    /// authority)` — append `measurement.leaderboard.entry_retracted` on
    /// the experiment run and record the mark. Never a deletion: the
    /// entry stays listed with the `retracted` flag.
    pub fn retract_entry(
        &self,
        store: &mut Store,
        definition_ref: &str,
        configuration_id: &str,
        reason_ref: &str,
        authority: &str,
    ) -> Result<Json, ResultsError> {
        let def = self.leaderboard_definition(definition_ref)?;
        let payload = Json::obj([
            ("definition_ref", Json::str(definition_ref)),
            ("configuration_id", Json::str(configuration_id)),
            ("reason_ref", Json::str(reason_ref)),
            ("authority", Json::str(authority)),
        ]);
        let env = store
            .commit_kernel_row_for(
                "hh-results/1",
                &def.experiment_run_id,
                "measurement.leaderboard.entry_retracted",
                payload.clone(),
                vec![],
                vec![],
            )
            .map_err(ResultsError::from)?;
        let path = retractions_path(self, definition_ref);
        let mut entries = load_retractions(self, definition_ref);
        entries.insert(
            configuration_id.to_string(),
            Json::obj([
                ("reason_ref", Json::str(reason_ref)),
                ("authority", Json::str(authority)),
                ("event_ref", Json::str(&env.event_id)),
            ]),
        );
        lb_write(
            &path,
            &Json::obj([
                ("definition_ref", Json::str(definition_ref)),
                ("entries", Json::Obj(entries)),
            ]),
        )?;
        Ok(Json::obj([
            ("event_ref", Json::str(&env.event_id)),
            ("configuration_id", Json::str(configuration_id)),
            ("retracted", Json::Bool(true)),
        ]))
    }

    /// `publish_leaderboard(definition, snapshot_id?, policy)` — the C1
    /// export-as-publication op (§6.5 §4; ADR-0141 D4): retain the
    /// snapshot, append `measurement.leaderboard.published{definition_ref,
    /// snapshot_id, view_hash, readers, pinned_addresses[]}` to the
    /// experiment run and `measurement.export.delivered` to every cited
    /// subject run. `pinned_addresses[]` names each admitted entry's
    /// `audit_ref.hash` + evidence `content_refs` + the loss report's own
    /// address — the ledger's pin set then refuses `gc` on them.
    pub fn publish_leaderboard(
        &self,
        store: &mut Store,
        docs: &LabDocs,
        definition: &LeaderboardDefinition,
        at: Option<&WatermarkSet>,
        policy: &Json,
    ) -> Result<PublishedLeaderboard, ResultsError> {
        let snap = self.snapshot_leaderboard(store, docs, definition, at)?;
        let def_ref = definition.definition_id();
        let view_hash = snap.to_json().to_canonical_string();
        let view_hash = idp_id("results_view", view_hash.as_bytes());
        // The pin set — every evidence address the publication cites.
        let mut pinned: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for e in &snap.entries {
            if hh_ledger::ids::is_pinned_id(&e.audit_ref.hash) {
                pinned.insert(e.audit_ref.hash.clone());
            }
            if hh_ledger::ids::is_pinned_id(&e.version_id) {
                pinned.insert(e.version_id.clone());
            }
            for r in &e.audit_ref.content_refs {
                if hh_ledger::ids::is_pinned_id(r) {
                    pinned.insert(r.clone());
                }
            }
            // Cells' evidence content refs ride the same pin.
            if let Ok((row, _)) = self.get_row(store, &e.key) {
                for c in &row.cells {
                    for a in &c.evidence {
                        for r in &a.content_refs {
                            if hh_ledger::ids::is_pinned_id(r) {
                                pinned.insert(r.clone());
                            }
                        }
                    }
                }
            }
        }
        let readers: Vec<String> = match policy.get("readers") {
            Some(Json::Arr(v)) => v
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect(),
            _ => definition.readers.clone(),
        };
        let payload = Json::obj([
            ("definition_ref", Json::str(&def_ref)),
            ("snapshot_id", Json::str(&snap.snapshot_id)),
            ("view_hash", Json::str(&view_hash)),
            (
                "readers",
                Json::Arr(readers.iter().map(Json::str).collect()),
            ),
            (
                "pinned_addresses",
                Json::Arr(pinned.iter().map(Json::str).collect()),
            ),
        ]);
        // The pins are real, not just recorded: the publication event's
        // `refs[]` name every pinned address so the ledger's `gc`
        // `pin_reason` scan refuses them (`referenced_by_run`, §5g.6).
        let pin_refs: Vec<hh_identity::idp::ContentAddress> = pinned
            .iter()
            .filter_map(|a| {
                hh_identity::idp::parse_id(a)
                    .ok()
                    .map(|p| hh_identity::idp::ContentAddress {
                        idp: "idp/1",
                        algorithm: "sha256",
                        digest: p.digest_hex,
                        media_type: String::new(),
                        size: 0,
                    })
            })
            .collect();
        let env = store
            .commit_kernel_row_for(
                "hh-results/1",
                &snap.experiment_run_id,
                "measurement.leaderboard.published",
                payload,
                pin_refs,
                vec![],
            )
            .map_err(ResultsError::from)?;
        // `measurement.export.delivered` per cited subject run (§5h.3).
        // The publication lowering's own loss report — members a foreign
        // leaderboard submission has no slot for (selection marks, flag
        // overlays) are listed under the same contract `export_rows`
        // produces.
        let mut loss_entries: Vec<crate::export::LossEntry> = Vec::new();
        for e in &snap.entries {
            if e.selection.is_some() {
                loss_entries.push(crate::export::LossEntry {
                    run_id: e.audit_ref.run_id.clone(),
                    field: "selection".to_string(),
                    reason: "no_slot".to_string(),
                    detail: "leaderboard submission carries no selection slot".to_string(),
                });
            }
            if !e.flags.is_empty() || e.retracted {
                loss_entries.push(crate::export::LossEntry {
                    run_id: e.audit_ref.run_id.clone(),
                    field: "flags".to_string(),
                    reason: "no_slot".to_string(),
                    detail: "leaderboard submission carries no flag slot".to_string(),
                });
            }
        }
        let loss = crate::export::LoweringLossReport {
            target: "leaderboard_submission".to_string(),
            lossless: loss_entries.is_empty(),
            entries: loss_entries,
        };
        let loss_ref = loss.content_ref();
        let mut delivered_runs: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for e in &snap.entries {
            delivered_runs.insert(e.audit_ref.run_id.clone());
        }
        for run_id in &delivered_runs {
            let dpayload = Json::obj([
                ("sink_id", Json::str(format!("leaderboard:{def_ref}"))),
                ("view_kind", Json::str("leaderboard_snapshot")),
                (
                    "seq_range",
                    Json::Arr(vec![
                        Json::Int(0),
                        Json::Int(snap.watermark_set.runs.get(run_id).copied().unwrap_or(0) as i64),
                    ]),
                ),
                (
                    "content_classes",
                    Json::Arr(vec![Json::str("leaderboard_entry")]),
                ),
                ("loss_report_ref", Json::str(loss_ref.id())),
            ]);
            store
                .commit_kernel_row_for(
                    "hh-results/1",
                    run_id,
                    "measurement.export.delivered",
                    dpayload,
                    vec![loss_ref.clone()],
                    vec![],
                )
                .map_err(ResultsError::from)?;
        }
        // Persist the publication record — `pin_set` scans it.
        lb_write(
            &published_path(self, &snap.snapshot_id),
            &Json::obj([
                ("snapshot_id", Json::str(&snap.snapshot_id)),
                ("definition_ref", Json::str(&def_ref)),
                ("view_hash", Json::str(&view_hash)),
                ("event_ref", Json::str(&env.event_id)),
                (
                    "readers",
                    Json::Arr(readers.iter().map(Json::str).collect()),
                ),
                (
                    "pinned_addresses",
                    Json::Arr(pinned.iter().map(Json::str).collect()),
                ),
                ("loss_report_ref", Json::str(loss_ref.id())),
            ]),
        )?;
        let snapshot_id = snap.snapshot_id;
        self.journal_emit(crate::journal::JournalEvent {
            kind: crate::journal::JournalKind::SnapshotPublished,
            subject: snapshot_id.clone(),
            detail: def_ref.clone(),
        });
        Ok(PublishedLeaderboard {
            snapshot_id,
            view_hash,
            event_ref: env.event_id,
            pinned_addresses: pinned.into_iter().collect(),
        })
    }

    /// `pin_set()` — every address a published snapshot pins: the set GC
    /// must refuse (CF-347/R-S-7). Derived from the persisted publication
    /// records — nothing is inferred (CC3).
    pub fn pin_set(&self) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        let dir = published_dir(self);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return out;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(Json::Obj(m)) = lb_read(&path) {
                if let Some(Json::Arr(v)) = m.get("pinned_addresses") {
                    for a in v.iter().filter_map(Json::as_str) {
                        out.insert(a.to_string());
                    }
                }
            }
        }
        out
    }
}

/// `snapshot_memberships(key_id)` — every retained snapshot id whose
/// `entries[]` admit the key (the annotation index's `leaders[]` source;
/// rebuildable derived state, never recorded on the row).
pub(crate) fn snapshot_memberships(results: &ResultsStore, key_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let dir = snaps_dir(results);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path.parent().map(|p| p.ends_with("index")).unwrap_or(false) {
            continue;
        }
        let Ok(j) = lb_read(&path) else {
            continue;
        };
        if let Some(snap) = LeaderboardSnapshot::from_json(&j) {
            if snap.entries.iter().any(|e| e.key == key_id) {
                out.push(snap.snapshot_id);
            }
        }
    }
    out.sort();
    out
}
