//! The eval-plane run record (spec §5h.2/§6.5 row keys; R-2.9.2; S3.3).
//!
//! `EvalRun` is the records-in projection `compare`/`render_scorecard` consume:
//! the manifest's row-key members (`task_ref`, `experiment{arm_id, cell_id,
//! replicate_index, comparable}`, `participant_class`,
//! `observability_level`, `environment_ref`, `seed`) plus the settled
//! outcome-side facts the ledger projects (`outcome_class`, consumed budgets,
//! emitted metric values, the veto/observability evidence — [`LedgerFacts`]).
//! The projection never manufactures values: a run that never emitted a metric
//! carries no `MetricValue` for it and the cell renders `n/a{not_run}`.
//!
//! Codec members are strict (unknown members refuse — CC3/CC8). The record is
//! language-neutral canonical JSON (CC4); `configuration_id` is computed by
//! `hh-identity`, never trusted from input.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ontology::compliance::NaReason;
use hh_ontology::eval::{MediationChannel, MetricValue, MetricValueKind};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{CapabilityVerdict, Observability, ParticipantClass};
use hh_ontology::DimensionId;
use hh_wire::Json;

use crate::facts::LedgerFacts;
use crate::json_util::*;

/// A run's cache state at start (the `MatchSpec.cache_policy` the run
/// realised — consumed at compare time, never inferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheState {
    /// `cold_start`.
    ColdStart,
    /// `natural`.
    Natural,
    /// `primed{prime_ref}` — the ref rides the JSON form.
    Primed,
}

impl CacheState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheState::ColdStart => "cold_start",
            CacheState::Natural => "natural",
            CacheState::Primed => "primed",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<CacheState> {
        match s {
            "cold_start" => Some(CacheState::ColdStart),
            "natural" => Some(CacheState::Natural),
            "primed" => Some(CacheState::Primed),
            _ => None,
        }
    }
}

/// `eval_run/1` — the eval-plane run row (the comparison/scorecard input).
#[derive(Debug, Clone, PartialEq)]
pub struct EvalRun {
    /// The run id (the ledger's `run_id`).
    pub run_id: String,
    /// The arm this run executes (`experiment.arm_id` — a bare `arm_id`
    /// member for the records-in path).
    pub arm_id: String,
    /// The cell id, when bound (`experiment.cell_id`).
    pub cell_id: Option<String>,
    /// The seedless aggregate coordinate (`configuration_id` — computed at
    /// ingest by `hh_identity::configuration_id`, carried here as the bound
    /// value).
    pub configuration_id: String,
    /// `native | hosted`.
    pub participant_class: ParticipantClass,
    /// The declared observability subset (`ledger ⇔ native`).
    pub observability_level: BTreeSet<Observability>,
    /// The mediation channels the participant exposes (ADR-0165 D6 —
    /// `requires_mediation` consults this).
    pub mediation: BTreeSet<MediationChannel>,
    /// The capability verdicts (`capability_vector` — `applicability`
    /// consumes `Supported` verdicts only; absent/`unknown` never coerces).
    pub capability_vector: BTreeMap<String, CapabilityVerdict>,
    /// `task_ref.task_id`.
    pub task_id: String,
    /// `task_ref.suite_id`.
    pub suite_id: String,
    /// `task_ref.split_label`.
    pub split_label: SplitLabel,
    /// The replicate within the cell.
    pub replicate_index: u64,
    /// The attempt ordinal within the replicate.
    pub attempt_no: u64,
    /// The run seed, when bound.
    pub seed: Option<u64>,
    /// Whether the run honoured its declared seed material (CF-095 — the
    /// `by_task_and_replicate` pairing gate).
    pub seed_honoured: bool,
    /// The realised cache state.
    pub cache_state: CacheState,
    /// `experiment.comparable` — `false` marks the run non-comparable
    /// (exploratory/diagnostic); `compare` refuses it.
    pub comparable: bool,
    /// The derived outcome class (`eval::derive_outcome_class` / the settled
    /// `StopReason` projection — never a metric value).
    pub outcome_class: hh_ontology::control::OutcomeClass,
    /// Per-dimension consumed quantities (`control.budget.consumed` settled).
    pub budget_consumed: BTreeMap<DimensionId, i64>,
    /// The veto ids this run tripped (`vetoes::evaluate_vetoes` output —
    /// carried so the scorecard can count vetoed successes beside the
    /// headline).
    pub veto_tripped: Vec<String>,
    /// The emitted metric values (`measurement.metric.emitted` rows).
    pub values: Vec<MetricValue>,
    /// The pinned environment record version id (the image digest class —
    /// compare refuses an environment mismatch that is not the varied factor).
    pub environment_version_id: Option<String>,
    /// The environment family the run executed under.
    pub environment_family: EnvironmentFamily,
    /// The bound fault profile ref (F-class level; `None` = unfaulted).
    pub fault_profile: Option<String>,
    /// The bound perturbation profile ref (F-class level).
    pub perturbation_profile: Option<String>,
    /// `role → model snapshot ref` (the `same_snapshot` check's input).
    pub model_snapshots: BTreeMap<String, String>,
    /// The contamination stratum the run's task sits in.
    pub stratum: ContaminationStratum,
    /// The search spend charged to the eval phase (µ-units; the
    /// `artifact_benefit` zero-spend precondition reads this).
    pub eval_search_spend: u64,
    /// The task-split hash the run's task belongs to (the
    /// `SplitAssignmentRecord` hash — paired against the pre-registration).
    pub split_hash: Option<String>,
    /// `routing.deviation` — the run routed off its pre-registered plan (a
    /// fallback chain fired; ADR-0122 d.5; the report generator never pools
    /// deviated and clean rows without the design's declared policy —
    /// AC-R-2.3.2-8).
    pub routing_deviation: bool,
    /// `replayed_trajectory` — the run's rows were scored by replaying
    /// logged trajectories under a substituted model; `compare` refuses
    /// (`replayed_trajectory`; AC-R-2.3.2-13).
    pub replayed_trajectory: bool,
    /// `served_from_cache_count` — the calls a K4/K5 response cache served
    /// (`served_from_cache` terminals; AC-R-2.3.4-10/-11).
    pub served_from_cache_count: u64,
    /// `cache.prefix_hit_ratio` (ppm) — `None` renders `n/a` (unmeasured,
    /// e.g. `cache_state_visible = unsupported` or hosted usage roles).
    pub cache_prefix_hit_ratio: Option<i64>,
    /// The projected ledger facts the veto/compliance predicates read.
    pub facts: LedgerFacts,
}

impl EvalRun {
    /// The metric's value for this run — the typed `n/a{not_run}` when no
    /// `MetricValue` for `metric_ref` was emitted (never 0, never a proxy).
    pub fn value_for(&self, metric_ref: &str) -> MetricValueKind {
        self.values
            .iter()
            .find(|v| v.metric_ref == metric_ref)
            .map(|v| v.value.clone())
            .unwrap_or(MetricValueKind::Na(NaReason::NotRun))
    }

    /// The `MetricValue` row for a metric — detector/oracle provenance
    /// included (the cell render filters on `detector_classes_allowed`;
    /// AC-R-2.9.2-13).
    pub fn metric_value(&self, metric_ref: &str) -> Option<&MetricValue> {
        self.values.iter().find(|v| v.metric_ref == metric_ref)
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("eval_run/1"));
        m.insert("run_id".into(), Json::str(&self.run_id));
        m.insert("arm_id".into(), Json::str(&self.arm_id));
        if let Some(c) = &self.cell_id {
            m.insert("cell_id".into(), Json::str(c));
        }
        m.insert("configuration_id".into(), Json::str(&self.configuration_id));
        m.insert(
            "participant_class".into(),
            Json::str(self.participant_class.as_str()),
        );
        m.insert(
            "observability_level".into(),
            Json::Arr(
                self.observability_level
                    .iter()
                    .map(|o| Json::str(o.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "mediation".into(),
            Json::Arr(
                self.mediation
                    .iter()
                    .map(|c| Json::str(c.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "capability_vector".into(),
            Json::Obj(
                self.capability_vector
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(capability_verdict_str(*v))))
                    .collect(),
            ),
        );
        m.insert("task_id".into(), Json::str(&self.task_id));
        m.insert("suite_id".into(), Json::str(&self.suite_id));
        m.insert("split_label".into(), Json::str(self.split_label.name()));
        m.insert(
            "replicate_index".into(),
            Json::Int(self.replicate_index as i64),
        );
        m.insert("attempt_no".into(), Json::Int(self.attempt_no as i64));
        if let Some(s) = self.seed {
            m.insert("seed".into(), Json::Int(s as i64));
        }
        m.insert("seed_honoured".into(), Json::Bool(self.seed_honoured));
        m.insert("cache_state".into(), Json::str(self.cache_state.as_str()));
        m.insert("comparable".into(), Json::Bool(self.comparable));
        m.insert(
            "outcome_class".into(),
            Json::str(self.outcome_class.as_str()),
        );
        m.insert(
            "budget_consumed".into(),
            Json::Obj(
                self.budget_consumed
                    .iter()
                    .map(|(d, q)| (d.as_str().to_string(), Json::Int(*q)))
                    .collect(),
            ),
        );
        m.insert(
            "veto_tripped".into(),
            Json::Arr(self.veto_tripped.iter().map(Json::str).collect()),
        );
        m.insert(
            "values".into(),
            Json::Arr(self.values.iter().map(|v| v.to_json()).collect()),
        );
        if let Some(e) = &self.environment_version_id {
            m.insert("environment_version_id".into(), Json::str(e));
        }
        m.insert(
            "environment_family".into(),
            Json::str(self.environment_family.name()),
        );
        if let Some(f) = &self.fault_profile {
            m.insert("fault_profile".into(), Json::str(f));
        }
        if let Some(p) = &self.perturbation_profile {
            m.insert("perturbation_profile".into(), Json::str(p));
        }
        m.insert(
            "model_snapshots".into(),
            Json::Obj(
                self.model_snapshots
                    .iter()
                    .map(|(r, s)| (r.clone(), Json::str(s)))
                    .collect(),
            ),
        );
        m.insert("stratum".into(), Json::str(self.stratum.name()));
        m.insert(
            "eval_search_spend".into(),
            Json::Int(self.eval_search_spend as i64),
        );
        if let Some(h) = &self.split_hash {
            m.insert("split_hash".into(), Json::str(h));
        }
        if self.routing_deviation {
            m.insert("routing_deviation".into(), Json::Bool(true));
        }
        if self.replayed_trajectory {
            m.insert("replayed_trajectory".into(), Json::Bool(true));
        }
        m.insert(
            "served_from_cache_count".into(),
            Json::Int(self.served_from_cache_count as i64),
        );
        if let Some(r) = self.cache_prefix_hit_ratio {
            m.insert("cache_prefix_hit_ratio".into(), Json::Int(r));
        }
        m.insert("facts".into(), self.facts.to_json());
        Json::Obj(m)
    }

    /// Strict decode — `SchemaError` on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<EvalRun, SchemaError> {
        const REC: &str = "eval_run/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "schema",
                "run_id",
                "arm_id",
                "cell_id",
                "configuration_id",
                "participant_class",
                "observability_level",
                "mediation",
                "capability_vector",
                "task_id",
                "suite_id",
                "split_label",
                "replicate_index",
                "attempt_no",
                "seed",
                "seed_honoured",
                "cache_state",
                "comparable",
                "outcome_class",
                "budget_consumed",
                "veto_tripped",
                "values",
                "environment_version_id",
                "environment_family",
                "fault_profile",
                "perturbation_profile",
                "model_snapshots",
                "stratum",
                "eval_search_spend",
                "split_hash",
                "routing_deviation",
                "replayed_trajectory",
                "served_from_cache_count",
                "cache_prefix_hit_ratio",
                "facts",
            ],
            REC,
        )?;
        let observability_level = enum_vec_at(m, "observability_level", REC, |j| {
            j.as_str().and_then(Observability::parse)
        })?;
        let mediation = enum_vec_at(m, "mediation", REC, |j| {
            j.as_str().and_then(MediationChannel::parse)
        })?;
        let capability_vector = match member_at(m, "capability_vector", REC)? {
            Json::Obj(cv) => {
                let mut out = BTreeMap::new();
                for (k, v) in cv {
                    out.insert(
                        k.clone(),
                        parse_capability_verdict(v.as_str().unwrap_or("")).ok_or_else(|| {
                            SchemaError::v("capability_vector", "unknown verdict spelling")
                        })?,
                    );
                }
                out
            }
            _ => return Err(SchemaError::v("capability_vector", "must be an object")),
        };
        let budget_consumed = match member_at(m, "budget_consumed", REC)? {
            Json::Obj(bc) => {
                let mut out = BTreeMap::new();
                for (k, v) in bc {
                    let d = DimensionId::parse(k)
                        .ok_or_else(|| SchemaError::v("budget_consumed", "unknown dimension"))?;
                    let q = v
                        .as_int()
                        .ok_or_else(|| SchemaError::v("budget_consumed", "quantity must be int"))?;
                    out.insert(d, q);
                }
                out
            }
            _ => return Err(SchemaError::v("budget_consumed", "must be an object")),
        };
        let values = arr_at(m, "values", REC)?
            .iter()
            .map(|v| {
                MetricValue::from_json(v).map_err(|e| SchemaError::v("values", format!("{e:?}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let model_snapshots = match member_at(m, "model_snapshots", REC)? {
            Json::Obj(ms) => ms
                .iter()
                .map(|(r, s)| {
                    s.as_str()
                        .map(|s| (r.clone(), s.to_string()))
                        .ok_or_else(|| SchemaError::v("model_snapshots", "value must be string"))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            _ => return Err(SchemaError::v("model_snapshots", "must be an object")),
        };
        Ok(EvalRun {
            run_id: str_at(m, "run_id", REC)?.to_string(),
            arm_id: str_at(m, "arm_id", REC)?.to_string(),
            cell_id: opt_str_at(m, "cell_id")?.map(str::to_string),
            configuration_id: str_at(m, "configuration_id", REC)?.to_string(),
            participant_class: ParticipantClass::parse(str_at(m, "participant_class", REC)?)
                .ok_or_else(|| SchemaError::v("participant_class", "unknown class"))?,
            observability_level: observability_level.into_iter().collect(),
            mediation: mediation.into_iter().collect(),
            capability_vector,
            task_id: str_at(m, "task_id", REC)?.to_string(),
            suite_id: str_at(m, "suite_id", REC)?.to_string(),
            split_label: SplitLabel::parse(str_at(m, "split_label", REC)?)
                .ok_or_else(|| SchemaError::v("split_label", "unknown label"))?,
            replicate_index: int_at(m, "replicate_index", REC)? as u64,
            attempt_no: int_at(m, "attempt_no", REC)? as u64,
            seed: opt_int_at(m, "seed")?.map(|s| s as u64),
            seed_honoured: bool_at(m, "seed_honoured", REC)?,
            cache_state: CacheState::parse(str_at(m, "cache_state", REC)?)
                .ok_or_else(|| SchemaError::v("cache_state", "unknown state"))?,
            comparable: bool_at(m, "comparable", REC)?,
            outcome_class: hh_ontology::control::OutcomeClass::parse(str_at(
                m,
                "outcome_class",
                REC,
            )?)
            .ok_or_else(|| SchemaError::v("outcome_class", "unknown class"))?,
            budget_consumed,
            veto_tripped: str_vec_at(m, "veto_tripped", REC)?,
            values,
            environment_version_id: opt_str_at(m, "environment_version_id")?.map(str::to_string),
            environment_family: EnvironmentFamily::parse(str_at(m, "environment_family", REC)?)
                .ok_or_else(|| SchemaError::v("environment_family", "unknown family"))?,
            fault_profile: opt_str_at(m, "fault_profile")?.map(str::to_string),
            perturbation_profile: opt_str_at(m, "perturbation_profile")?.map(str::to_string),
            model_snapshots,
            stratum: ContaminationStratum::parse(str_at(m, "stratum", REC)?)
                .ok_or_else(|| SchemaError::v("stratum", "unknown stratum"))?,
            eval_search_spend: int_at(m, "eval_search_spend", REC)? as u64,
            split_hash: opt_str_at(m, "split_hash")?.map(str::to_string),
            routing_deviation: match m.get("routing_deviation") {
                Some(Json::Bool(v)) => *v,
                _ => false,
            },
            replayed_trajectory: match m.get("replayed_trajectory") {
                Some(Json::Bool(v)) => *v,
                _ => false,
            },
            served_from_cache_count: opt_int_at(m, "served_from_cache_count")?.unwrap_or(0) as u64,
            cache_prefix_hit_ratio: opt_int_at(m, "cache_prefix_hit_ratio")?,
            facts: LedgerFacts::from_json(member_at(m, "facts", REC)?)?,
        })
    }
}

/// `task_context/1` — the per-task context the scorecard/compare needs from
/// the suite plane (a projection of `hh_lab::bench::TaskRecord` +
/// `SplitAssignmentRecord` — records-in, never re-derived here).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskContext {
    /// The task id.
    pub task_id: String,
    /// The suite id.
    pub suite_id: String,
    /// The split label.
    pub split_label: SplitLabel,
    /// The split-hash the assignment record pins.
    pub split_hash: String,
    /// The contamination stratum.
    pub stratum: ContaminationStratum,
}

/// `suite_context/1` — the per-suite flags the scorecard honours.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteContext {
    /// The suite id.
    pub suite_id: String,
    /// The `suite.retirement` flag — a retired suite never feeds the headline
    /// (its rows render labelled, beside).
    pub retired_for_headline: bool,
    /// The environment family.
    pub family: EnvironmentFamily,
}

/// The `CapabilityVerdict` spelling (the ontology type has no `as_str` —
/// mirror the §2.7.3 canonical spellings here, one place).
pub fn capability_verdict_str(v: CapabilityVerdict) -> &'static str {
    match v {
        CapabilityVerdict::Supported => "supported",
        CapabilityVerdict::Unsupported => "unsupported",
        CapabilityVerdict::Partial => "partial",
        CapabilityVerdict::NotApplicable => "not_applicable",
        CapabilityVerdict::Unknown => "unknown",
        CapabilityVerdict::Skipped => "skipped",
        CapabilityVerdict::Drift => "drift",
    }
}

/// Parse a `CapabilityVerdict` spelling.
pub fn parse_capability_verdict(s: &str) -> Option<CapabilityVerdict> {
    Some(match s {
        "supported" => CapabilityVerdict::Supported,
        "unsupported" => CapabilityVerdict::Unsupported,
        "partial" => CapabilityVerdict::Partial,
        "not_applicable" => CapabilityVerdict::NotApplicable,
        "unknown" => CapabilityVerdict::Unknown,
        "skipped" => CapabilityVerdict::Skipped,
        "drift" => CapabilityVerdict::Drift,
        _ => return None,
    })
}
