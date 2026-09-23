//! `ResultsRow/1` (`hh-results-row/1`) — the immutable derived projection of
//! one subject run (§6.5 §3 row 1; ADR-0161 D1). The record is canonical
//! JSON end to end; `version_id = H(idp ∥ "results_row" ∥ canonical(row))`
//! and `view_hash` binds the watermark set, so equal inputs reproduce equal
//! bytes (R-ROW-1). **R-ROW-2**: no display names — every coordinate is an
//! id or a pinned ref. **R-ROW-3**: `cells[]` is the total metric list of
//! the pinned registry version — one cell per `MetricDeclaration`, `n/a`
//! with its typed reason where no value exists.

use std::collections::BTreeMap;

use hh_identity::idp::idp_id;
use hh_ontology::compliance::NaReason;
use hh_ontology::eval::MetricValueKind;
use hh_wire::json::Json;

use crate::audit::AuditRef;
use crate::error::ResultsError;
use crate::scoring::ScoringContext;
use crate::watermark::WatermarkSet;

/// The schema tag.
pub const ROW_SCHEMA: &str = "hh-results-row/1";
/// The identity profile.
pub const ROW_IDP: &str = "idp/1";

fn bad(member: &str, detail: &str) -> ResultsError {
    ResultsError::Schema {
        member: member.to_string(),
        detail: detail.to_string(),
    }
}

fn str_member(m: &BTreeMap<String, Json>, name: &str) -> Result<String, ResultsError> {
    m.get(name)
        .and_then(Json::as_str)
        .map(String::from)
        .ok_or_else(|| bad(name, "absent or ill-typed"))
}

fn opt_str(m: &BTreeMap<String, Json>, name: &str) -> Option<String> {
    m.get(name).and_then(Json::as_str).map(String::from)
}

fn obj_member<'a>(
    m: &'a BTreeMap<String, Json>,
    name: &str,
) -> Result<&'a BTreeMap<String, Json>, ResultsError> {
    match m.get(name) {
        Some(Json::Obj(o)) => Ok(o),
        _ => Err(bad(name, "absent or not an object")),
    }
}

/// `key{configuration_version_id, run_id}` — the fixed row key (ADR-0038's
/// seeded coordinate + the run; ADR-0161 D2: versions supersede under the
/// fixed key, the key never changes).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowKey {
    /// The seeded exact-bytes configuration coordinate.
    pub configuration_version_id: String,
    /// The subject run.
    pub run_id: String,
}

impl RowKey {
    /// The key's own pinned id — `idp/1` over the canonical key record. This
    /// is the store's directory coordinate (never the display tuple — R-ROW-2
    /// is about names; the pair stays on the row for reads).
    pub fn key_id(&self) -> String {
        idp_id(
            "results_row.key",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "configuration_version_id",
                Json::str(&self.configuration_version_id),
            ),
            ("run_id", Json::str(&self.run_id)),
        ])
    }

    /// Strict decode — accepts either a bare `{configuration_version_id,
    /// run_id}` object or a `{"key": {…}}` envelope.
    pub fn from_json(j: &Json) -> Result<RowKey, ResultsError> {
        let top = objmap(j)?;
        match obj_member(&top, "key") {
            Ok(m) => RowKey::from_obj(m),
            Err(_) => RowKey::from_obj(&top),
        }
    }

    /// Decode from a bare `{configuration_version_id, run_id}` object.
    pub fn from_obj(m: &BTreeMap<String, Json>) -> Result<RowKey, ResultsError> {
        Ok(RowKey {
            configuration_version_id: str_member(m, "configuration_version_id")?,
            run_id: str_member(m, "run_id")?,
        })
    }
}

fn objmap(j: &Json) -> Result<BTreeMap<String, Json>, ResultsError> {
    match j {
        Json::Obj(m) => Ok(m.clone()),
        _ => Err(bad("record", "not an object")),
    }
}

/// `coordinates` — the ADR-0045 factor tuple. Budget members are populated
/// from the bound experiment arm (via `LabDocs`) when resolvable; an unbound
/// run carries `budget_ref` alone (the member stays honest — no fabricated
/// refs).
#[derive(Debug, Clone, PartialEq)]
pub struct Coordinates {
    /// `configuration_id` — the seedless aggregate coordinate.
    pub configuration_id: Option<String>,
    /// `configuration_version_id` — the seeded exact-bytes coordinate.
    pub configuration_version_id: String,
    /// `model_snapshots{role → snapshot_id}` — per-role pinned snapshots.
    pub model_snapshots: BTreeMap<String, String>,
    /// The sealed harness definition ref (`semantic_id`/`version_id` pinned).
    pub harness_def_ref: Option<String>,
    /// The bound model profile.
    pub model_profile_ref: Option<String>,
    /// The environment record ref.
    pub environment_ref: Option<String>,
    /// The environment record's version id.
    pub environment_version_id: Option<String>,
    /// `task{task_id, suite_id, split_label}`.
    pub task: Option<Json>,
    /// `budget{budget_ref?, eval_budget_ref?, search_budget_ref?,
    /// match_spec_ref?, match_spec?, cache_policy?}` — refs plus the pinned
    /// `MatchSpec` record when resolvable (the leaderboard's L4 input).
    pub budget: Json,
    /// `replicate{seed?, seed_material?}`.
    pub replicate: Json,
    /// `native | hosted`.
    pub participant_class: String,
    /// The declared observability subset.
    pub observability_level: Vec<String>,
    /// The hosting mechanism (hosted rows).
    pub hosting_mechanism: Option<String>,
    /// The capability vector ref (hosted rows).
    pub capability_vector_ref: Option<String>,
    /// The mediation channels the run's surface covered (§6.6 — the
    /// `MediationChannel` spellings the arm's hosting declaration admits;
    /// absent/empty = none). Additive member — never authored for the hosted
    /// side beyond what the declaration claims (T-LCD-07).
    pub mediation: Vec<String>,
    /// The pinned registry snapshot (`§6.2`'s one-snapshot rule).
    pub registry_snapshot_id: Option<String>,
}

impl Coordinates {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(c) = &self.configuration_id {
            m.insert("configuration_id".into(), Json::str(c));
        }
        m.insert(
            "configuration_version_id".into(),
            Json::str(&self.configuration_version_id),
        );
        if !self.model_snapshots.is_empty() {
            m.insert(
                "model_snapshots".into(),
                Json::Obj(
                    self.model_snapshots
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            );
        }
        if let Some(h) = &self.harness_def_ref {
            m.insert("harness_def_ref".into(), Json::str(h));
        }
        if let Some(p) = &self.model_profile_ref {
            m.insert("model_profile_ref".into(), Json::str(p));
        }
        if let Some(e) = &self.environment_ref {
            m.insert("environment_ref".into(), Json::str(e));
        }
        if let Some(e) = &self.environment_version_id {
            m.insert("environment_version_id".into(), Json::str(e));
        }
        if let Some(t) = &self.task {
            m.insert("task".into(), t.clone());
        }
        m.insert("budget".into(), self.budget.clone());
        m.insert("replicate".into(), self.replicate.clone());
        m.insert(
            "participant_class".into(),
            Json::str(&self.participant_class),
        );
        m.insert(
            "observability_level".into(),
            Json::Arr(self.observability_level.iter().map(Json::str).collect()),
        );
        if let Some(h) = &self.hosting_mechanism {
            m.insert("hosting_mechanism".into(), Json::str(h));
        }
        if let Some(c) = &self.capability_vector_ref {
            m.insert("capability_vector_ref".into(), Json::str(c));
        }
        if !self.mediation.is_empty() {
            m.insert(
                "mediation".into(),
                Json::Arr(self.mediation.iter().map(Json::str).collect()),
            );
        }
        if let Some(r) = &self.registry_snapshot_id {
            m.insert("registry_snapshot_id".into(), Json::str(r));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<Coordinates, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(bad("coordinates", "not an object"));
        };
        let mut model_snapshots = BTreeMap::new();
        if let Some(Json::Obj(ms)) = m.get("model_snapshots") {
            for (k, v) in ms {
                model_snapshots.insert(
                    k.clone(),
                    v.as_str()
                        .ok_or_else(|| bad("model_snapshots", "ill-typed"))?
                        .to_string(),
                );
            }
        }
        Ok(Coordinates {
            configuration_id: opt_str(m, "configuration_id"),
            configuration_version_id: str_member(m, "configuration_version_id")?,
            model_snapshots,
            harness_def_ref: opt_str(m, "harness_def_ref"),
            model_profile_ref: opt_str(m, "model_profile_ref"),
            environment_ref: opt_str(m, "environment_ref"),
            environment_version_id: opt_str(m, "environment_version_id"),
            task: m.get("task").cloned(),
            budget: m.get("budget").cloned().unwrap_or(Json::obj([])),
            replicate: m.get("replicate").cloned().unwrap_or(Json::obj([])),
            participant_class: str_member(m, "participant_class")?,
            observability_level: match m.get("observability_level") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            hosting_mechanism: opt_str(m, "hosting_mechanism"),
            capability_vector_ref: opt_str(m, "capability_vector_ref"),
            mediation: match m.get("mediation") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            registry_snapshot_id: opt_str(m, "registry_snapshot_id"),
        })
    }
}

/// `outcome{outcome_class?, stop_reason?, status, veto_tripped[],
/// finished_at?, wall_ms?, activation_no}` — the run's terminal projection
/// (`status = open` while no `lifecycle.run.finished` is durable).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutcomeSection {
    /// `finished | open`.
    pub status: String,
    /// The derived outcome class (`None` while open).
    pub outcome_class: Option<String>,
    /// The recorded stop reason.
    pub stop_reason: Option<String>,
    /// Veto ids tripped (annotate-only — ADR-0047; rows store them at every
    /// tier per OQ-128).
    pub veto_tripped: Vec<String>,
    /// `lifecycle.run.finished`'s `ts`.
    pub finished_at: Option<String>,
    /// Wall time when the run records it.
    pub wall_ms: Option<i64>,
    /// The manifest's `activation_no`.
    pub activation_no: u64,
}

impl OutcomeSection {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("status".into(), Json::str(&self.status));
        if let Some(o) = &self.outcome_class {
            m.insert("outcome_class".into(), Json::str(o));
        }
        if let Some(s) = &self.stop_reason {
            m.insert("stop_reason".into(), Json::str(s));
        }
        m.insert(
            "veto_tripped".into(),
            Json::Arr(self.veto_tripped.iter().map(Json::str).collect()),
        );
        if let Some(f) = &self.finished_at {
            m.insert("finished_at".into(), Json::str(f));
        }
        if let Some(w) = self.wall_ms {
            m.insert("wall_ms".into(), Json::Int(w));
        }
        m.insert("activation_no".into(), Json::Int(self.activation_no as i64));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<OutcomeSection, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(bad("outcome", "not an object"));
        };
        Ok(OutcomeSection {
            status: str_member(m, "status")?,
            outcome_class: opt_str(m, "outcome_class"),
            stop_reason: opt_str(m, "stop_reason"),
            veto_tripped: match m.get("veto_tripped") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            finished_at: opt_str(m, "finished_at"),
            wall_ms: m.get("wall_ms").and_then(Json::as_int),
            activation_no: m.get("activation_no").and_then(Json::as_int).unwrap_or(1) as u64,
        })
    }
}

/// One `cells[]` member — `{metric_ref, value | n/a{reason}, detector?,
/// oracle_ref?, confidence?, evidence: audit_ref[], computed_from[]}`.
/// `value` is `MetricValueKind`'s canonical form (`n/a` rides inside it as
/// `MetricValueKind::Na` — one schema source, CC7).
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The metric's registry ref (declaration name at Stage 3).
    pub metric_ref: String,
    /// The typed value (`MetricValueKind::Na` for `n/a{reason}`).
    pub value: MetricValueKind,
    /// The detector class that produced the value.
    pub detector: Option<String>,
    /// The oracle the value cites.
    pub oracle_ref: Option<String>,
    /// Confidence ppm, when declared.
    pub confidence: Option<u64>,
    /// The citations — every cell carries ≥ 1 (the head citation at minimum,
    /// so the "not emitted" fact is itself auditable).
    pub evidence: Vec<AuditRef>,
    /// The emitting event seqs (subject run) + overlay verdict seqs
    /// (`overlay:<run>:<seq>` spellings keep the run coordinate visible).
    pub computed_from: Vec<String>,
}

impl Cell {
    /// The typed `n/a` reason when the value is `Na`.
    pub fn na_reason(&self) -> Option<NaReason> {
        match &self.value {
            MetricValueKind::Na(r) => Some(*r),
            _ => None,
        }
    }

    /// Whether the cell carries a concrete value.
    pub fn is_concrete(&self) -> bool {
        !matches!(self.value, MetricValueKind::Na(_))
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("metric_ref".into(), Json::str(&self.metric_ref));
        m.insert("value".into(), self.value.to_json());
        if let Some(d) = &self.detector {
            m.insert("detector".into(), Json::str(d));
        }
        if let Some(o) = &self.oracle_ref {
            m.insert("oracle_ref".into(), Json::str(o));
        }
        if let Some(c) = self.confidence {
            m.insert("confidence".into(), Json::Int(c as i64));
        }
        m.insert(
            "evidence".into(),
            Json::Arr(self.evidence.iter().map(|e| e.to_json()).collect()),
        );
        m.insert(
            "computed_from".into(),
            Json::Arr(self.computed_from.iter().map(Json::str).collect()),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<Cell, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(bad("cells", "not an object"));
        };
        let value = m
            .get("value")
            .and_then(MetricValueKind::from_json)
            .ok_or_else(|| bad("value", "not a MetricValueKind"))?;
        let mut evidence = Vec::new();
        if let Some(Json::Arr(items)) = m.get("evidence") {
            for i in items {
                evidence
                    .push(AuditRef::from_json(i).ok_or_else(|| bad("evidence", "bad audit_ref"))?);
            }
        }
        Ok(Cell {
            metric_ref: str_member(m, "metric_ref")?,
            value,
            detector: opt_str(m, "detector"),
            oracle_ref: opt_str(m, "oracle_ref"),
            confidence: m.get("confidence").and_then(Json::as_int).map(|c| c as u64),
            evidence,
            computed_from: match m.get("computed_from") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
        })
    }
}

/// `consumption{dimensions{dimension → amount}, utilization?, spend?,
/// instrument_spend?}` — the `control.budget.consumed` fold plus the
/// settle-time utilization record (§6.5 row 1; `ResultsRowFields`' owner is
/// `hh-budget` — the section carries its members verbatim).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConsumptionSection {
    /// Per-dimension consumed totals.
    pub dimensions: BTreeMap<String, i64>,
    /// The experiment settle record's `budget_utilization` (`{dim → ppm}`),
    /// when the binding experiment reported it.
    pub utilization: Option<Json>,
    /// The `ResultsSpend` record, when priced.
    pub spend: Option<Json>,
    /// The instrument-side spend, when reported.
    pub instrument_spend: Option<Json>,
}

impl ConsumptionSection {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "dimensions".into(),
            Json::Obj(
                self.dimensions
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v)))
                    .collect(),
            ),
        );
        if let Some(u) = &self.utilization {
            m.insert("utilization".into(), u.clone());
        }
        if let Some(s) = &self.spend {
            m.insert("spend".into(), s.clone());
        }
        if let Some(s) = &self.instrument_spend {
            m.insert("instrument_spend".into(), s.clone());
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ConsumptionSection, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(bad("consumption", "not an object"));
        };
        let mut dimensions = BTreeMap::new();
        if let Some(Json::Obj(d)) = m.get("dimensions") {
            for (k, v) in d {
                dimensions.insert(
                    k.clone(),
                    v.as_int().ok_or_else(|| bad("dimensions", "ill-typed"))?,
                );
            }
        }
        Ok(ConsumptionSection {
            dimensions,
            utilization: m.get("utilization").cloned(),
            spend: m.get("spend").cloned(),
            instrument_spend: m.get("instrument_spend").cloned(),
        })
    }
}

/// `audit{head: audit_ref, checkpoint_ref?}` — the run-head citation the row
/// was projected at (§6.5 §3; §6 "the auditor may store chain heads in the
/// results store").
#[derive(Debug, Clone, PartialEq)]
pub struct AuditSection {
    /// The head citation.
    pub head: AuditRef,
    /// The last `security.audit.checkpoint`'s idp, when durable.
    pub checkpoint_ref: Option<String>,
}

/// `derived_from{run_id, seq, overlay_watermarks{}}` — the exact inputs the
/// projection consumed (ADR-0161 D1/R-ROW-1).
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedFrom {
    /// The subject run.
    pub run_id: String,
    /// The subject prefix seq.
    pub seq: u64,
    /// `overlay_run → seq` the regrade folded.
    pub overlay_watermarks: BTreeMap<String, u64>,
}

/// `ResultsRow/1` — the immutable derived row (see module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct ResultsRow {
    /// The fixed key.
    pub key: RowKey,
    /// The factor-tuple coordinates.
    pub coordinates: Coordinates,
    /// `experiment{experiment_run_id, arm_id, cell_id, design_ref?,
    /// pre_registration_ref?, replicate_index, attempt_no, comparable}` —
    /// absent on unbound rows.
    pub experiment: Option<Json>,
    /// The terminal projection.
    pub outcome: OutcomeSection,
    /// The total metric cell list (R-ROW-3).
    pub cells: Vec<Cell>,
    /// The consumption fold.
    pub consumption: ConsumptionSection,
    /// `cache{policy?, …}` — the arm's `cache_policy` when bound.
    pub cache: Json,
    /// `lineage{parent_run_id?, forked_from?, continued_from?, …}` —
    /// ADR-0133's row fields (never key components, CF-345).
    pub lineage: Json,
    /// `annotations_from_ledger{discontinuities[], …}` — facts lifted from
    /// the ledger itself (the rebuildable annotation *index* is a separate
    /// record — CF-298 keeps it out of row bytes).
    pub annotations_from_ledger: Json,
    /// The head citation.
    pub audit: AuditSection,
    /// The pinned scoring context.
    pub scoring: ScoringContext,
    /// The exact source prefix.
    pub derived_from: DerivedFrom,
    /// The full watermark set (subject + overlays).
    pub watermark_set: WatermarkSet,
    /// `view_hash` — `idp/1` over `{row_preimage, watermark_set}`.
    pub view_hash: String,
    /// `version_id` — `H(idp ∥ "results_row" ∥ canonical(row))`.
    pub version_id: String,
}

impl ResultsRow {
    /// The canonical preimage — every member except `view_hash`/`version_id`.
    fn preimage(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(ROW_SCHEMA));
        m.insert("idp".into(), Json::str(ROW_IDP));
        m.insert("key".into(), self.key.to_json());
        m.insert("coordinates".into(), self.coordinates.to_json());
        if let Some(e) = &self.experiment {
            m.insert("experiment".into(), e.clone());
        }
        m.insert("outcome".into(), self.outcome.to_json());
        m.insert(
            "cells".into(),
            Json::Arr(self.cells.iter().map(|c| c.to_json()).collect()),
        );
        m.insert("consumption".into(), self.consumption.to_json());
        m.insert("cache".into(), self.cache.clone());
        m.insert("lineage".into(), self.lineage.clone());
        m.insert(
            "annotations_from_ledger".into(),
            self.annotations_from_ledger.clone(),
        );
        let mut audit = BTreeMap::new();
        audit.insert("head".into(), self.audit.head.to_json());
        if let Some(c) = &self.audit.checkpoint_ref {
            audit.insert("checkpoint_ref".into(), Json::str(c));
        }
        m.insert("audit".into(), Json::Obj(audit));
        m.insert("scoring".into(), self.scoring.to_json());
        let mut df = BTreeMap::new();
        df.insert("run_id".into(), Json::str(&self.derived_from.run_id));
        df.insert("seq".into(), Json::Int(self.derived_from.seq as i64));
        df.insert(
            "overlay_watermarks".into(),
            Json::Obj(
                self.derived_from
                    .overlay_watermarks
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        m.insert("derived_from".into(), Json::Obj(df));
        m.insert("watermark_set".into(), self.watermark_set.to_json());
        Json::Obj(m)
    }

    /// `version_id = H(idp ∥ "results_row" ∥ canonical(row minus
    /// view_hash/version_id))` — the recompute `verify_row` compares.
    pub fn compute_version_id(&self) -> String {
        idp_id(
            "results_row",
            self.preimage().to_canonical_string().as_bytes(),
        )
    }

    /// `view_hash` — the projection's binding to its watermark set.
    pub fn compute_view_hash(&self) -> String {
        let j = Json::obj([
            ("row", self.preimage()),
            ("watermark_set", self.watermark_set.to_json()),
        ]);
        idp_id("results_view", j.to_canonical_string().as_bytes())
    }

    /// Canonical JSON (identity members stamped).
    pub fn to_json(&self) -> Json {
        let mut m = match self.preimage() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("view_hash".into(), Json::str(&self.view_hash));
        m.insert("version_id".into(), Json::str(&self.version_id));
        Json::Obj(m)
    }

    /// Strict decode — unknown members refuse (closed record).
    pub fn from_json(j: &Json) -> Result<ResultsRow, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(bad("row", "not an object"));
        };
        if m.get("schema").and_then(Json::as_str) != Some(ROW_SCHEMA) {
            return Err(bad("schema", "not hh-results-row/1"));
        }
        let key = RowKey::from_obj(obj_member(m, "key")?)?;
        let coordinates = Coordinates::from_json(
            m.get("coordinates")
                .ok_or_else(|| bad("coordinates", "absent"))?,
        )?;
        let outcome =
            OutcomeSection::from_json(m.get("outcome").ok_or_else(|| bad("outcome", "absent"))?)?;
        let cells = match m.get("cells") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(Cell::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(bad("cells", "absent")),
        };
        let consumption = ConsumptionSection::from_json(
            m.get("consumption")
                .ok_or_else(|| bad("consumption", "absent"))?,
        )?;
        let audit_m = obj_member(m, "audit")?;
        let head = AuditRef::from_json(&Json::Obj(
            audit_m
                .get("head")
                .cloned()
                .and_then(|h| match h {
                    Json::Obj(o) => Some(o),
                    _ => None,
                })
                .ok_or_else(|| bad("audit.head", "absent"))?,
        ))
        .ok_or_else(|| bad("audit.head", "bad audit_ref"))?;
        let scoring =
            ScoringContext::from_json(m.get("scoring").ok_or_else(|| bad("scoring", "absent"))?)?;
        let df_m = obj_member(m, "derived_from")?;
        let mut overlay_watermarks = BTreeMap::new();
        if let Some(Json::Obj(ow)) = df_m.get("overlay_watermarks") {
            for (k, v) in ow {
                overlay_watermarks.insert(
                    k.clone(),
                    v.as_int()
                        .ok_or_else(|| bad("overlay_watermarks", "ill-typed"))?
                        as u64,
                );
            }
        }
        let derived_from = DerivedFrom {
            run_id: str_member(df_m, "run_id")?,
            seq: df_m
                .get("seq")
                .and_then(Json::as_int)
                .ok_or_else(|| bad("derived_from.seq", "absent"))? as u64,
            overlay_watermarks,
        };
        let watermark_set = WatermarkSet::from_json(
            m.get("watermark_set")
                .ok_or_else(|| bad("watermark_set", "absent"))?,
        )
        .ok_or_else(|| bad("watermark_set", "bad set"))?;
        Ok(ResultsRow {
            key,
            coordinates,
            experiment: m.get("experiment").cloned(),
            outcome,
            cells,
            consumption,
            cache: m.get("cache").cloned().unwrap_or(Json::obj([])),
            lineage: m.get("lineage").cloned().unwrap_or(Json::obj([])),
            annotations_from_ledger: m
                .get("annotations_from_ledger")
                .cloned()
                .unwrap_or(Json::obj([])),
            audit: AuditSection {
                head,
                checkpoint_ref: opt_str(audit_m, "checkpoint_ref"),
            },
            scoring,
            derived_from,
            watermark_set,
            view_hash: str_member(m, "view_hash")?,
            version_id: str_member(m, "version_id")?,
        })
    }

    /// The canonical bytes.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }
}
