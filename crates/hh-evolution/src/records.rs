//! The evolution pipeline's wire records (§05h §4/§5; S6.1a) — strict
//! canonical-JSON codecs (`Json` + `hh_lab::json_util`), unknown members
//! refused, one spelling per fact (CC7).
//!
//! Records-in / records-out: every stage gate consumes the *record* the
//! caller deposits (proposal, hypothesis, screen report, comparison ref,
//! …); the pipeline never fabricates evidence — a missing or malformed
//! record is a typed refusal, never a default (CC3/CC9).

use hh_hir::diff::HirDiff;
use hh_identity::idp::idp_id;
use hh_lab::json_util::{
    bool_at, enum_vec_at, expect_obj, int_at, member_at, opt_bool_at, opt_int_at, opt_str_at,
    reject_unknown, str_at, str_vec_at, SchemaError,
};
use hh_ontology::lab::{EnvironmentFamily, SplitLabel};
use hh_wire::json::Json;
use std::collections::BTreeMap;

/// The campaign spec's dialect spelling.
pub const CAMPAIGN_SPEC_SCHEMA: &str = "hh.evolution.campaign_spec/1";

// ── CampaignSpec ────────────────────────────────────────────────────────────

/// `slot_allocation` — how the campaign's search budget distributes across
/// slots (§05h §4's `uniform | fractional_design`; `uniform` carries a
/// per-slot floor the open-time `BudgetSplittingTrap` check reads).
#[derive(Debug, Clone, PartialEq)]
pub enum SlotAllocation {
    /// `uniform` — equal shares; `min_share_ppm` is the per-slot floor.
    Uniform {
        /// The per-slot minimum share (ppm of the campaign budget).
        min_share_ppm: u64,
    },
    /// `fractional_design{slots[{slot, min_share_ppm}]}` — an explicit
    /// per-slot floor table.
    FractionalDesign {
        /// `slot → min_share_ppm` (ppm; must sum to ≤ 1_000_000).
        slots: BTreeMap<String, u64>,
    },
}

impl SlotAllocation {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            SlotAllocation::Uniform { min_share_ppm } => Json::obj([(
                "uniform",
                Json::obj([("min_share_ppm", Json::Int(*min_share_ppm as i64))]),
            )]),
            SlotAllocation::FractionalDesign { slots } => Json::obj([(
                "fractional_design",
                Json::obj([(
                    "slots",
                    Json::Obj(
                        slots
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                            .collect(),
                    ),
                )]),
            )]),
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SlotAllocation, SchemaError> {
        let m = expect_obj(j, "slot_allocation")?;
        if m.len() != 1 {
            return Err(SchemaError::v(
                "slot_allocation",
                "must be a one-key {uniform|fractional_design} object",
            ));
        }
        let (k, v) = m.iter().next().expect("len checked");
        let vm = expect_obj(v, "slot_allocation")?;
        match k.as_str() {
            "uniform" => {
                reject_unknown(vm, &["min_share_ppm"], "uniform")?;
                Ok(SlotAllocation::Uniform {
                    min_share_ppm: int_at(vm, "min_share_ppm", "uniform")? as u64,
                })
            }
            "fractional_design" => {
                reject_unknown(vm, &["slots"], "fractional_design")?;
                let mut slots = BTreeMap::new();
                match member_at(vm, "slots", "fractional_design")? {
                    Json::Obj(sm) => {
                        for (s, v) in sm {
                            slots.insert(
                                s.clone(),
                                v.as_int().ok_or_else(|| {
                                    SchemaError::v("slots", "share must be an integer (ppm)")
                                })? as u64,
                            );
                        }
                    }
                    _ => {
                        return Err(SchemaError::v(
                            "slots",
                            "must be a map<slot, min_share_ppm>",
                        ))
                    }
                }
                Ok(SlotAllocation::FractionalDesign { slots })
            }
            _ => Err(SchemaError::v(
                "slot_allocation",
                format!("unknown slot_allocation `{k}`"),
            )),
        }
    }
}

/// `stop_rule` — the mandatory campaign stop declaration (§05h §4; AC-13):
/// `budget_cap_ref` is a budget the campaign's spend may never exceed;
/// `max_candidates`/`stagnation_window` bound the loop;
/// `no_addressable_failure` admits the empty-frontier stop.
#[derive(Debug, Clone, PartialEq)]
pub struct StopRule {
    /// The campaign budget cap ref (None = uncapped is refused — the
    /// caller declares one; the check is `is_some` at validate).
    pub budget_cap_ref: Option<String>,
    /// Maximum candidates the campaign registers.
    pub max_candidates: Option<u64>,
    /// Proposals registered without a `validated` transition before the
    /// campaign stops `stagnation`.
    pub stagnation_window: Option<u64>,
    /// Admit `no_addressable_failure` as a stop reason.
    pub no_addressable_failure: bool,
}

impl StopRule {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(b) = &self.budget_cap_ref {
            m.insert("budget_cap_ref".into(), Json::str(b));
        }
        if let Some(n) = self.max_candidates {
            m.insert("max_candidates".into(), Json::Int(n as i64));
        }
        if let Some(w) = self.stagnation_window {
            m.insert("stagnation_window".into(), Json::Int(w as i64));
        }
        m.insert(
            "no_addressable_failure".into(),
            Json::Bool(self.no_addressable_failure),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<StopRule, SchemaError> {
        let m = expect_obj(j, "stop_rule")?;
        reject_unknown(
            m,
            &[
                "budget_cap_ref",
                "max_candidates",
                "stagnation_window",
                "no_addressable_failure",
            ],
            "stop_rule",
        )?;
        Ok(StopRule {
            budget_cap_ref: opt_str_at(m, "budget_cap_ref")?.map(str::to_string),
            max_candidates: opt_int_at(m, "max_candidates")?.map(|i| i as u64),
            stagnation_window: opt_int_at(m, "stagnation_window")?.map(|i| i as u64),
            no_addressable_failure: opt_bool_at(m, "no_addressable_failure")?.unwrap_or(false),
        })
    }
}

/// `corpus` — the S0 evidence corpus declaration (§05h §4 S0): the
/// environments, suites, metric vocabulary and task universe the
/// campaign's hypotheses cite, plus the L3 `split_assignment_ref` — a
/// `SplitAssignmentRecord` pinned *before* `opened`.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusSpec {
    /// The environment families the campaign's evidence spans.
    pub environments: Vec<EnvironmentFamily>,
    /// The suite refs the corpus admits.
    pub suite_refs: Vec<String>,
    /// The registered metric vocabulary (S2's `UnknownMetric` domain).
    pub metric_refs: Vec<String>,
    /// The task universe (`affected_task_ids` resolves against it).
    pub task_ids: Vec<String>,
    /// The evidence refs the corpus admits (S2's `evidence_refs` domain).
    pub evidence_refs: Vec<String>,
    /// The L3 split pin — a `SplitAssignmentRecord` ref registered before
    /// the campaign opened (R-2.9.4⁴).
    pub split_assignment_ref: String,
}

impl CorpusSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "environments",
                Json::Arr(
                    self.environments
                        .iter()
                        .map(|e| Json::str(e.name()))
                        .collect(),
                ),
            ),
            (
                "suite_refs",
                Json::Arr(self.suite_refs.iter().map(Json::str).collect()),
            ),
            (
                "metric_refs",
                Json::Arr(self.metric_refs.iter().map(Json::str).collect()),
            ),
            (
                "task_ids",
                Json::Arr(self.task_ids.iter().map(Json::str).collect()),
            ),
            (
                "evidence_refs",
                Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
            ),
            (
                "split_assignment_ref",
                Json::str(&self.split_assignment_ref),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CorpusSpec, SchemaError> {
        let m = expect_obj(j, "corpus")?;
        reject_unknown(
            m,
            &[
                "environments",
                "suite_refs",
                "metric_refs",
                "task_ids",
                "evidence_refs",
                "split_assignment_ref",
            ],
            "corpus",
        )?;
        Ok(CorpusSpec {
            environments: enum_vec_at(m, "environments", "corpus", |j| {
                j.as_str().and_then(EnvironmentFamily::parse)
            })?,
            suite_refs: str_vec_at(m, "suite_refs", "corpus")?,
            metric_refs: str_vec_at(m, "metric_refs", "corpus")?,
            task_ids: str_vec_at(m, "task_ids", "corpus")?,
            evidence_refs: str_vec_at(m, "evidence_refs", "corpus")?,
            split_assignment_ref: str_at(m, "split_assignment_ref", "corpus")?.to_string(),
        })
    }
}

/// `EvolutionCampaignSpec` (dialect `hh.evolution.campaign_spec/1`) —
/// the campaign's immutable declaration (§05h §4). `campaign_id =
/// H(canonical(spec minus campaign_id))` under `evolution_campaign`.
#[derive(Debug, Clone, PartialEq)]
pub struct EvolutionCampaignSpec {
    /// `campaign_id = H(canonical(spec))` — computed, never authored.
    pub campaign_id: String,
    /// `protocol` — `human_proposed` only at 6a (proposer plugins are 6b+).
    pub protocol: String,
    /// The definition ref the campaign evolves (the lineage's root).
    pub base_definition_ref: String,
    /// The evolution service's own definition ref — the self-modification
    /// exclusion's anchor (§05h §4 S1; G3-1/AC-R-2.12.2-14).
    pub service_definition_ref: String,
    /// The S0 corpus declaration.
    pub corpus: CorpusSpec,
    /// Semantic ids a candidate may never touch.
    pub exclusion_targets: Vec<String>,
    /// Semantic ids whose code leaves a candidate may never rewrite
    /// (the MUST-code exclusion).
    pub must_code_targets: Vec<String>,
    /// The admitted op target entity kinds (`None` = all kinds).
    pub allowed_target_kinds: Option<Vec<String>>,
    /// The `semantic_ops` bound (S1 `TooManyOps`).
    pub semantic_ops_bound: u64,
    /// The S3 observed-flip floor (ppm).
    pub min_flip_share_ppm: u64,
    /// The S3 replicate floor.
    pub min_replicates: u32,
    /// The S5 retention margin (ppm of baseline — a Δ below
    /// `-retention_margin_ppm` regresses).
    pub retention_margin_ppm: i64,
    /// The veto metric set (S5 hard gates).
    pub veto_metrics: Vec<String>,
    /// The slot allocation.
    pub slot_allocation: SlotAllocation,
    /// The mandatory stop rule.
    pub stop_rule: StopRule,
    /// The maturity flags the campaign claims (`evaluation_maturity`,
    /// `committable_state`, …) — AC-5 reads them on the acceptance report.
    pub maturity_flags: Vec<String>,
    /// The proposer family the campaign admits (`human` at 6a).
    pub proposer_family: String,
    /// Whether hosted participants join the campaign's arms (AC-15's
    /// `IncommensurableMatch` trigger when a reported-only dimension is
    /// declared on the spec's `reported_only_dimensions`).
    pub hosted_participants: bool,
    /// Dimensions a hosted campaign reports but cannot enforce — the
    /// AC-15 trap (non-empty + `hosted_participants` ⇒ every
    /// `matched_total` claim on this campaign is `IncommensurableMatch`).
    pub reported_only_dimensions: Vec<String>,
}

impl EvolutionCampaignSpec {
    /// `campaign_id` — the content address of the spec minus the id
    /// member (same scheme as `ExperimentSpec::experiment_id`, CC1).
    pub fn derive_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(m) = &mut j {
            m.remove("campaign_id");
        }
        idp_id("evolution_campaign", j.to_canonical_string().as_bytes())
    }

    /// The schema checks (`open` runs them before any row mints).
    pub fn validate(&self) -> Result<(), crate::errors::Refusal> {
        use crate::errors::Refusal::*;
        if self.protocol != "human_proposed" {
            return Err(SchemaViolation {
                detail: format!(
                    "protocol `{}` — only `human_proposed` is admitted at Stage 6a",
                    self.protocol
                ),
            });
        }
        if self.proposer_family != "human" {
            return Err(SchemaViolation {
                detail: format!(
                    "proposer_family `{}` — only `human` is admitted at Stage 6a",
                    self.proposer_family
                ),
            });
        }
        if self.base_definition_ref.is_empty() || self.service_definition_ref.is_empty() {
            return Err(SchemaViolation {
                detail: "base_definition_ref and service_definition_ref are mandatory".into(),
            });
        }
        if self.corpus.split_assignment_ref.is_empty() {
            return Err(EvidenceStale {
                detail: "corpus.split_assignment_ref absent — the L3 pin is mandatory".into(),
            });
        }
        // The stop rule is mandatory and must admit a real stop.
        if self.stop_rule.max_candidates.is_none()
            && self.stop_rule.stagnation_window.is_none()
            && self.stop_rule.budget_cap_ref.is_none()
            && !self.stop_rule.no_addressable_failure
        {
            return Err(BudgetSplittingTrap {
                detail: "stop_rule declares no bound — the campaign must be stoppable".into(),
            });
        }
        // BudgetSplittingTrap — `uniform` needs a slot count it can cover:
        // the floor times the slot count must not exceed the whole.
        match &self.slot_allocation {
            SlotAllocation::Uniform { min_share_ppm } if *min_share_ppm > 1_000_000 => {
                return Err(BudgetSplittingTrap {
                    detail: format!(
                        "uniform min_share_ppm {min_share_ppm} exceeds the whole (1_000_000)"
                    ),
                })
            }
            SlotAllocation::FractionalDesign { slots }
                if slots.values().sum::<u64>() > 1_000_000 =>
            {
                return Err(BudgetSplittingTrap {
                    detail: format!(
                        "fractional_design floors sum to {} ppm — over the whole",
                        slots.values().sum::<u64>()
                    ),
                })
            }
            _ => {}
        }
        Ok(())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("campaign_id".into(), Json::str(&self.campaign_id));
        m.insert("schema".into(), Json::str(CAMPAIGN_SPEC_SCHEMA));
        m.insert("protocol".into(), Json::str(&self.protocol));
        m.insert(
            "base_definition_ref".into(),
            Json::str(&self.base_definition_ref),
        );
        m.insert(
            "service_definition_ref".into(),
            Json::str(&self.service_definition_ref),
        );
        m.insert("corpus".into(), self.corpus.to_json());
        m.insert(
            "exclusion_targets".into(),
            Json::Arr(self.exclusion_targets.iter().map(Json::str).collect()),
        );
        m.insert(
            "must_code_targets".into(),
            Json::Arr(self.must_code_targets.iter().map(Json::str).collect()),
        );
        if let Some(k) = &self.allowed_target_kinds {
            m.insert(
                "allowed_target_kinds".into(),
                Json::Arr(k.iter().map(Json::str).collect()),
            );
        }
        m.insert(
            "semantic_ops_bound".into(),
            Json::Int(self.semantic_ops_bound as i64),
        );
        m.insert(
            "min_flip_share_ppm".into(),
            Json::Int(self.min_flip_share_ppm as i64),
        );
        m.insert(
            "min_replicates".into(),
            Json::Int(self.min_replicates as i64),
        );
        m.insert(
            "retention_margin_ppm".into(),
            Json::Int(self.retention_margin_ppm),
        );
        m.insert(
            "veto_metrics".into(),
            Json::Arr(self.veto_metrics.iter().map(Json::str).collect()),
        );
        m.insert("slot_allocation".into(), self.slot_allocation.to_json());
        m.insert("stop_rule".into(), self.stop_rule.to_json());
        m.insert(
            "maturity_flags".into(),
            Json::Arr(self.maturity_flags.iter().map(Json::str).collect()),
        );
        m.insert("proposer_family".into(), Json::str(&self.proposer_family));
        m.insert(
            "hosted_participants".into(),
            Json::Bool(self.hosted_participants),
        );
        m.insert(
            "reported_only_dimensions".into(),
            Json::Arr(
                self.reported_only_dimensions
                    .iter()
                    .map(Json::str)
                    .collect(),
            ),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<EvolutionCampaignSpec, SchemaError> {
        let m = expect_obj(j, "EvolutionCampaignSpec")?;
        reject_unknown(
            m,
            &[
                "campaign_id",
                "schema",
                "protocol",
                "base_definition_ref",
                "service_definition_ref",
                "corpus",
                "exclusion_targets",
                "must_code_targets",
                "allowed_target_kinds",
                "semantic_ops_bound",
                "min_flip_share_ppm",
                "min_replicates",
                "retention_margin_ppm",
                "veto_metrics",
                "slot_allocation",
                "stop_rule",
                "maturity_flags",
                "proposer_family",
                "hosted_participants",
                "reported_only_dimensions",
            ],
            "EvolutionCampaignSpec",
        )?;
        Ok(EvolutionCampaignSpec {
            campaign_id: str_at(m, "campaign_id", "EvolutionCampaignSpec")?.to_string(),
            protocol: str_at(m, "protocol", "EvolutionCampaignSpec")?.to_string(),
            base_definition_ref: str_at(m, "base_definition_ref", "EvolutionCampaignSpec")?
                .to_string(),
            service_definition_ref: str_at(m, "service_definition_ref", "EvolutionCampaignSpec")?
                .to_string(),
            corpus: CorpusSpec::from_json(member_at(m, "corpus", "EvolutionCampaignSpec")?)?,
            exclusion_targets: str_vec_at(m, "exclusion_targets", "EvolutionCampaignSpec")?,
            must_code_targets: str_vec_at(m, "must_code_targets", "EvolutionCampaignSpec")?,
            allowed_target_kinds: match m.get("allowed_target_kinds") {
                None | Some(Json::Null) => None,
                _ => Some(str_vec_at(
                    m,
                    "allowed_target_kinds",
                    "EvolutionCampaignSpec",
                )?),
            },
            semantic_ops_bound: int_at(m, "semantic_ops_bound", "EvolutionCampaignSpec")? as u64,
            min_flip_share_ppm: int_at(m, "min_flip_share_ppm", "EvolutionCampaignSpec")? as u64,
            min_replicates: int_at(m, "min_replicates", "EvolutionCampaignSpec")? as u32,
            retention_margin_ppm: int_at(m, "retention_margin_ppm", "EvolutionCampaignSpec")?,
            veto_metrics: str_vec_at(m, "veto_metrics", "EvolutionCampaignSpec")?,
            slot_allocation: SlotAllocation::from_json(member_at(
                m,
                "slot_allocation",
                "EvolutionCampaignSpec",
            )?)?,
            stop_rule: StopRule::from_json(member_at(m, "stop_rule", "EvolutionCampaignSpec")?)?,
            maturity_flags: str_vec_at(m, "maturity_flags", "EvolutionCampaignSpec")?,
            proposer_family: str_at(m, "proposer_family", "EvolutionCampaignSpec")?.to_string(),
            hosted_participants: bool_at(m, "hosted_participants", "EvolutionCampaignSpec")?,
            reported_only_dimensions: str_vec_at(
                m,
                "reported_only_dimensions",
                "EvolutionCampaignSpec",
            )?,
        })
    }
}

// ── Proposal + hypothesis ───────────────────────────────────────────────────

/// `predicted.deltas[]` — one directional prediction (§05h §4 S2's
/// falsifiability contract).
#[derive(Debug, Clone, PartialEq)]
pub struct PredictedDelta {
    /// The metric (must resolve in `corpus.metric_refs`).
    pub metric: String,
    /// `increase` | `decrease` | `preserves`.
    pub direction: String,
}

/// `hypothesis.predicted` — the falsifiable effect statement.
#[derive(Debug, Clone, PartialEq)]
pub struct PredictedEffect {
    /// The directional deltas.
    pub deltas: Vec<PredictedDelta>,
    /// The tasks the candidate predicts it affects (must be `search`/`dev`
    /// labelled — `LeakedSplit` otherwise).
    pub affected_task_ids: Vec<String>,
    /// `same_snapshot` | `cross_model`.
    pub model_scope: String,
    /// The eval horizon the prediction binds (optional).
    pub horizon: Option<String>,
}

/// `FailureHypothesis` (§05h §4 S2) — `{kind, evidence_refs[], predicted,
/// semantic_op_targets[]}`. `kind ∈ {failure, insufficiency, observational,
/// conditioned}`; `observational` candidates pass S3/S4 only.
#[derive(Debug, Clone, PartialEq)]
pub struct FailureHypothesis {
    /// The hypothesis kind.
    pub kind: String,
    /// The corpus evidence the hypothesis cites (non-empty).
    pub evidence_refs: Vec<String>,
    /// The predicted effect.
    pub predicted: PredictedEffect,
    /// The attribution set — every semantic diff op's target must be a
    /// member (`TargetMismatch`).
    pub semantic_op_targets: Vec<String>,
}

/// The admitted `hypothesis.kind` spellings.
pub const HYPOTHESIS_KINDS: &[&str] = &["failure", "insufficiency", "observational", "conditioned"];

/// `CandidateProposal` — the S1 intake record: `{base_ref, diff, slot,
/// hypothesis?}`. `diff` is the canonical `HirDiff` JSON (the caller's
/// `diff(base, target)` output — `hh-hir`'s own codec, CC7).
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateProposal {
    /// The sealed base definition ref the diff applies over.
    pub base_ref: String,
    /// The human-proposed diff.
    pub diff: HirDiff,
    /// The slot the proposal claims (the campaign's slot lock).
    pub slot: String,
    /// The hypothesis when the proposal carries one (else `hypothesize`).
    pub hypothesis: Option<FailureHypothesis>,
}

impl PredictedDelta {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("metric", Json::str(&self.metric)),
            ("direction", Json::str(&self.direction)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<PredictedDelta, SchemaError> {
        let m = expect_obj(j, "PredictedDelta")?;
        reject_unknown(m, &["metric", "direction"], "PredictedDelta")?;
        Ok(PredictedDelta {
            metric: str_at(m, "metric", "PredictedDelta")?.to_string(),
            direction: str_at(m, "direction", "PredictedDelta")?.to_string(),
        })
    }
}

impl PredictedEffect {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "deltas",
                Json::Arr(self.deltas.iter().map(|d| d.to_json()).collect()),
            ),
            (
                "affected_task_ids",
                Json::Arr(self.affected_task_ids.iter().map(Json::str).collect()),
            ),
            ("model_scope", Json::str(&self.model_scope)),
            (
                "horizon",
                self.horizon.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<PredictedEffect, SchemaError> {
        let m = expect_obj(j, "PredictedEffect")?;
        reject_unknown(
            m,
            &["deltas", "affected_task_ids", "model_scope", "horizon"],
            "PredictedEffect",
        )?;
        let deltas = match m.get("deltas") {
            Some(Json::Arr(a)) => a
                .iter()
                .map(PredictedDelta::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(SchemaError::v("PredictedEffect.deltas", "missing")),
        };
        Ok(PredictedEffect {
            deltas,
            affected_task_ids: str_vec_at(m, "affected_task_ids", "PredictedEffect")?,
            model_scope: str_at(m, "model_scope", "PredictedEffect")?.to_string(),
            horizon: opt_str_at(m, "horizon")?.map(str::to_string),
        })
    }
}

impl FailureHypothesis {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(&self.kind)),
            (
                "evidence_refs",
                Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
            ),
            ("predicted", self.predicted.to_json()),
            (
                "semantic_op_targets",
                Json::Arr(self.semantic_op_targets.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<FailureHypothesis, SchemaError> {
        let m = expect_obj(j, "FailureHypothesis")?;
        reject_unknown(
            m,
            &["kind", "evidence_refs", "predicted", "semantic_op_targets"],
            "FailureHypothesis",
        )?;
        let predicted = match m.get("predicted") {
            Some(p) => PredictedEffect::from_json(p)?,
            None => return Err(SchemaError::v("FailureHypothesis.predicted", "missing")),
        };
        Ok(FailureHypothesis {
            kind: str_at(m, "kind", "FailureHypothesis")?.to_string(),
            evidence_refs: str_vec_at(m, "evidence_refs", "FailureHypothesis")?,
            predicted,
            semantic_op_targets: str_vec_at(m, "semantic_op_targets", "FailureHypothesis")?,
        })
    }
}

impl CandidateProposal {
    /// The canonical JSON — `diff` rides `hh-hir`'s own codec (CC7).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("base_ref", Json::str(&self.base_ref)),
            ("diff", hh_hir::wire::diff_to_json(&self.diff)),
            ("slot", Json::str(&self.slot)),
            (
                "hypothesis",
                self.hypothesis
                    .as_ref()
                    .map(|h| h.to_json())
                    .unwrap_or(Json::Null),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CandidateProposal, SchemaError> {
        let m = expect_obj(j, "CandidateProposal")?;
        reject_unknown(
            m,
            &["base_ref", "diff", "slot", "hypothesis"],
            "CandidateProposal",
        )?;
        let diff = hh_hir::wire::diff_from_json(
            m.get("diff")
                .ok_or_else(|| SchemaError::v("CandidateProposal.diff", "missing"))?,
        )
        .map_err(|e| SchemaError::v("CandidateProposal.diff", format!("{e:?}")))?;
        let hypothesis = match m.get("hypothesis") {
            Some(Json::Null) | None => None,
            Some(h) => Some(FailureHypothesis::from_json(h)?),
        };
        Ok(CandidateProposal {
            base_ref: str_at(m, "base_ref", "CandidateProposal")?.to_string(),
            diff,
            slot: str_at(m, "slot", "CandidateProposal")?.to_string(),
            hypothesis,
        })
    }
}

// ── Stage evidence records ──────────────────────────────────────────────────

/// `ScreenReport` — the S3 counterexample screen's evidence record
/// (`{counterexample_set_ref, observations, flips_in_direction,
/// replicate_count, split_labels_used[], judge_only}`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenReport {
    /// The counterexample set's content ref.
    pub counterexample_set_ref: String,
    /// Observed cases.
    pub observations: u64,
    /// Of the observations, how many flipped in the predicted direction.
    pub flips_in_direction: u64,
    /// Replicates actually run.
    pub replicate_count: u32,
    /// The split labels the screen consulted (`held_out` → LeakedSplit).
    pub split_labels_used: Vec<SplitLabel>,
    /// Whether the screen is judge-only (refused — S3 needs targeted
    /// counterexamples).
    pub judge_only: bool,
}

impl ScreenReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "counterexample_set_ref",
                Json::str(&self.counterexample_set_ref),
            ),
            ("observations", Json::Int(self.observations as i64)),
            (
                "flips_in_direction",
                Json::Int(self.flips_in_direction as i64),
            ),
            ("replicate_count", Json::Int(self.replicate_count as i64)),
            (
                "split_labels_used",
                Json::Arr(
                    self.split_labels_used
                        .iter()
                        .map(|l| Json::str(l.name()))
                        .collect(),
                ),
            ),
            ("judge_only", Json::Bool(self.judge_only)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ScreenReport, SchemaError> {
        let m = expect_obj(j, "ScreenReport")?;
        reject_unknown(
            m,
            &[
                "counterexample_set_ref",
                "observations",
                "flips_in_direction",
                "replicate_count",
                "split_labels_used",
                "judge_only",
            ],
            "ScreenReport",
        )?;
        Ok(ScreenReport {
            counterexample_set_ref: str_at(m, "counterexample_set_ref", "ScreenReport")?
                .to_string(),
            observations: int_at(m, "observations", "ScreenReport")? as u64,
            flips_in_direction: int_at(m, "flips_in_direction", "ScreenReport")? as u64,
            replicate_count: int_at(m, "replicate_count", "ScreenReport")? as u32,
            split_labels_used: enum_vec_at(m, "split_labels_used", "ScreenReport", |j| {
                j.as_str().and_then(SplitLabel::parse)
            })?,
            judge_only: opt_bool_at(m, "judge_only")?.unwrap_or(false),
        })
    }
}

/// `TransferRow` — one transfer result over an environment family (§05h
/// §4 S6): `{environment_family, point?, interval?, method, status}`.
/// `status = n/a{observability}` is the compliance cell — reported, never
/// hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferRow {
    /// The environment family.
    pub environment_family: EnvironmentFamily,
    /// `measured` | `n/a{observability}` — the compliance marker.
    pub status: String,
    /// The point estimate (Json — the report's own value shape).
    pub point: Option<Json>,
    /// The interval estimate (`{lo, hi}` or the estimator's record).
    pub interval: Option<Json>,
    /// The interval method.
    pub method: String,
    /// Whether the row ran over the held-out split (the S6 gate requires
    /// ≥ 1 held-out family row).
    pub held_out: bool,
}

impl TransferRow {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "environment_family".into(),
            Json::str(self.environment_family.name()),
        );
        m.insert("status".into(), Json::str(&self.status));
        if let Some(p) = &self.point {
            m.insert("point".into(), p.clone());
        }
        if let Some(i) = &self.interval {
            m.insert("interval".into(), i.clone());
        }
        m.insert("method".into(), Json::str(&self.method));
        m.insert("held_out".into(), Json::Bool(self.held_out));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<TransferRow, SchemaError> {
        let m = expect_obj(j, "TransferRow")?;
        reject_unknown(
            m,
            &[
                "environment_family",
                "status",
                "point",
                "interval",
                "method",
                "held_out",
            ],
            "TransferRow",
        )?;
        let family = EnvironmentFamily::parse(str_at(m, "environment_family", "TransferRow")?)
            .ok_or_else(|| SchemaError::v("environment_family", "unknown environment family"))?;
        Ok(TransferRow {
            environment_family: family,
            status: str_at(m, "status", "TransferRow")?.to_string(),
            point: m.get("point").cloned(),
            interval: m.get("interval").cloned(),
            method: str_at(m, "method", "TransferRow")?.to_string(),
            held_out: opt_bool_at(m, "held_out")?.unwrap_or(false),
        })
    }
}

/// `SecurityInvarianceReport` — the S7 record (§05h §4 S7):
/// `{policy_leaves[{leaf, delta}], placement, has_interface,
/// dynamic_veto_table[{metric, regressed}], rollback_ref?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityInvarianceReport {
    /// The policy leaves the diff touches, each with its post-eval
    /// `classify_policy_edit` verdict (`narrowing` | other).
    pub policy_leaves: BTreeMap<String, String>,
    /// The candidate executable's placement — `subprocess_confined` (the
    /// only admissible placement) | `in_process` (refused).
    pub placement: String,
    /// Whether the candidate's `CompiledPayload` variants carry a declared
    /// interface (§05h §4 S7's `OpaqueWithoutInterface` gate).
    pub has_interface: bool,
    /// `metric → regressed` — the dynamic veto table (each member must be
    /// byte-equal-or-better vs the baseline; a `regressed` entry trips).
    pub dynamic_veto_table: BTreeMap<String, bool>,
}

impl SecurityInvarianceReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "policy_leaves",
                Json::Obj(
                    self.policy_leaves
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            ),
            ("placement", Json::str(&self.placement)),
            ("has_interface", Json::Bool(self.has_interface)),
            (
                "dynamic_veto_table",
                Json::Obj(
                    self.dynamic_veto_table
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Bool(*v)))
                        .collect(),
                ),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SecurityInvarianceReport, SchemaError> {
        let m = expect_obj(j, "SecurityInvarianceReport")?;
        reject_unknown(
            m,
            &[
                "policy_leaves",
                "placement",
                "has_interface",
                "dynamic_veto_table",
            ],
            "SecurityInvarianceReport",
        )?;
        let mut policy_leaves = BTreeMap::new();
        match member_at(m, "policy_leaves", "SecurityInvarianceReport")? {
            Json::Obj(pm) => {
                for (k, v) in pm {
                    policy_leaves.insert(
                        k.clone(),
                        v.as_str()
                            .ok_or_else(|| {
                                SchemaError::v("policy_leaves", "delta must be a string")
                            })?
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err(SchemaError::v(
                    "policy_leaves",
                    "must be a map<leaf, delta>",
                ))
            }
        }
        let mut dynamic_veto_table = BTreeMap::new();
        match member_at(m, "dynamic_veto_table", "SecurityInvarianceReport")? {
            Json::Obj(vm) => {
                for (k, v) in vm {
                    dynamic_veto_table.insert(
                        k.clone(),
                        match v {
                            Json::Bool(b) => *b,
                            _ => {
                                return Err(SchemaError::v(
                                    "dynamic_veto_table",
                                    "member must be a bool",
                                ))
                            }
                        },
                    );
                }
            }
            _ => {
                return Err(SchemaError::v(
                    "dynamic_veto_table",
                    "must be a map<metric, regressed>",
                ))
            }
        }
        Ok(SecurityInvarianceReport {
            policy_leaves,
            placement: str_at(m, "placement", "SecurityInvarianceReport")?.to_string(),
            has_interface: bool_at(m, "has_interface", "SecurityInvarianceReport")?,
            dynamic_veto_table,
        })
    }
}

// ── Acceptance report + seal ────────────────────────────────────────────────

/// `acceptance_item.status ∈ {pass, fail, n/a{reason}}`.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemStatus {
    /// The item's gate passed.
    Pass,
    /// The item's gate failed.
    Fail,
    /// The item legitimately does not apply — the reason is mandatory.
    NotApplicable {
        /// Why the item does not apply (`observability` | `no_counterfactual` |
        /// `no_boundary` | …).
        reason: String,
    },
}

/// One `EvolutionAcceptanceReport` line item (§05h §5 — items 1–7).
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceItem {
    /// The item number (1–7).
    pub item: u8,
    /// The verdict.
    pub status: ItemStatus,
    /// The evidence the verdict cites.
    pub evidence_refs: Vec<String>,
}

/// `portability_label ∈ {portable, conditioned_on{family}, reported}`.
#[derive(Debug, Clone, PartialEq)]
pub enum PortabilityLabel {
    /// ≥ 2 held-out families at held-out level — portable.
    Portable,
    /// Model/family-conditioned — `conditioned_on{family}`.
    ConditionedOn {
        /// The conditioning family.
        family: String,
    },
    /// One family only — reported, never claimed portable.
    Reported,
}

/// `EvolutionAcceptanceReport` (§05h §5; AC-R-2.9.5-1) — the seven-item
/// report the seal gate reads:
///
/// 1. static classification clean (S1 re-check at S8);
/// 2. counterfactual screen passed (S3);
/// 3. matched-budget evaluation passed (S4, `matched_total`);
/// 4. attribution settled at the declared granularity;
/// 5. security invariance passed (S7);
/// 6. transfer evidence recorded (S6);
/// 7. human review — always required.
///
/// Items 1, 2, 3, 5, 6 must be `pass`; 4 may be `n/a{not_settled}` only
/// when `attribution_granularity` is declared; 7 may be `pass` (evidence
/// attached) — never absent.
#[derive(Debug, Clone, PartialEq)]
pub struct EvolutionAcceptanceReport {
    /// The seven items (index 1..=7, complete).
    pub items: Vec<AcceptanceItem>,
    /// The declared attribution granularity
    /// (`designed_ablation | observation_substitution | parent_lineage |
    /// not_settled`).
    pub attribution_granularity: String,
    /// The portability label the report claims.
    pub portability_label: PortabilityLabel,
}

/// The accepted `attribution_granularity` spellings.
pub const GRANULARITIES: &[&str] = &[
    "designed_ablation",
    "observation_substitution",
    "parent_lineage",
    "not_settled",
];

impl EvolutionAcceptanceReport {
    /// The completeness check the seal gate runs (§05h §5 verbatim):
    /// items 1,2,3,5,6 pass; 4 pass-or-`n/a` with a declared granularity;
    /// 7 pass. Returns the failing item ids for `AcceptanceIncomplete`.
    pub fn check(&self) -> Vec<u8> {
        let mut failing = Vec::new();
        let get = |n: u8| self.items.iter().find(|i| i.item == n);
        for n in [1u8, 2, 3, 5, 6] {
            match get(n) {
                Some(AcceptanceItem {
                    status: ItemStatus::Pass,
                    ..
                }) => {}
                _ => failing.push(n),
            }
        }
        match get(4) {
            Some(AcceptanceItem {
                status: ItemStatus::Pass,
                ..
            }) => {}
            Some(AcceptanceItem {
                status: ItemStatus::NotApplicable { .. },
                ..
            }) if self.attribution_granularity == "not_settled"
                || GRANULARITIES.contains(&self.attribution_granularity.as_str()) => {}
            _ => failing.push(4),
        }
        match get(7) {
            Some(AcceptanceItem {
                status: ItemStatus::Pass,
                ..
            }) => {}
            _ => failing.push(7),
        }
        if self.items.len() != 7 {
            // An absent item is a fail by omission — report every index
            // not present.
            for n in 1u8..=7 {
                if get(n).is_none() && !failing.contains(&n) {
                    failing.push(n);
                }
            }
        }
        failing
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let status = |s: &ItemStatus| -> Json {
            match s {
                ItemStatus::Pass => Json::str("pass"),
                ItemStatus::Fail => Json::str("fail"),
                ItemStatus::NotApplicable { reason } => {
                    Json::obj([("n/a", Json::obj([("reason", Json::str(reason))]))])
                }
            }
        };
        let pl = match &self.portability_label {
            PortabilityLabel::Portable => Json::str("portable"),
            PortabilityLabel::ConditionedOn { family } => {
                Json::obj([("conditioned_on", Json::obj([("family", Json::str(family))]))])
            }
            PortabilityLabel::Reported => Json::str("reported"),
        };
        Json::obj([
            (
                "items",
                Json::Arr(
                    self.items
                        .iter()
                        .map(|i| {
                            Json::obj([
                                ("item", Json::Int(i.item as i64)),
                                ("status", status(&i.status)),
                                (
                                    "evidence_refs",
                                    Json::Arr(i.evidence_refs.iter().map(Json::str).collect()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "attribution_granularity",
                Json::str(&self.attribution_granularity),
            ),
            ("portability_label", pl),
            ("human_review", Json::str("required")),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<EvolutionAcceptanceReport, SchemaError> {
        let m = expect_obj(j, "EvolutionAcceptanceReport")?;
        reject_unknown(
            m,
            &[
                "items",
                "attribution_granularity",
                "portability_label",
                "human_review",
            ],
            "EvolutionAcceptanceReport",
        )?;
        let mut items = Vec::new();
        match member_at(m, "items", "EvolutionAcceptanceReport")? {
            Json::Arr(arr) => {
                for iv in arr {
                    let im = expect_obj(iv, "item")?;
                    reject_unknown(im, &["item", "status", "evidence_refs"], "item")?;
                    let status = match im.get("status") {
                        Some(Json::Str(s)) if s == "pass" => ItemStatus::Pass,
                        Some(Json::Str(s)) if s == "fail" => ItemStatus::Fail,
                        Some(Json::Obj(nm)) => {
                            let na = nm.get("n/a").ok_or_else(|| {
                                SchemaError::v("status", "status must be pass|fail|n/a{reason}")
                            })?;
                            ItemStatus::NotApplicable {
                                reason: str_at(expect_obj(na, "n/a")?, "reason", "n/a")?
                                    .to_string(),
                            }
                        }
                        _ => {
                            return Err(SchemaError::v(
                                "status",
                                "status must be pass|fail|n/a{reason}",
                            ))
                        }
                    };
                    items.push(AcceptanceItem {
                        item: int_at(im, "item", "item")? as u8,
                        status,
                        evidence_refs: str_vec_at(im, "evidence_refs", "item")?,
                    });
                }
            }
            _ => return Err(SchemaError::v("items", "must be an array")),
        }
        let portability_label = match m.get("portability_label") {
            Some(Json::Str(s)) if s == "portable" => PortabilityLabel::Portable,
            Some(Json::Str(s)) if s == "reported" => PortabilityLabel::Reported,
            Some(Json::Obj(pm)) => {
                let c = pm.get("conditioned_on").ok_or_else(|| {
                    SchemaError::v(
                        "portability_label",
                        "portable|reported|conditioned_on{family}",
                    )
                })?;
                PortabilityLabel::ConditionedOn {
                    family: str_at(expect_obj(c, "conditioned_on")?, "family", "conditioned_on")?
                        .to_string(),
                }
            }
            _ => {
                return Err(SchemaError::v(
                    "portability_label",
                    "portable|reported|conditioned_on{family}",
                ))
            }
        };
        Ok(EvolutionAcceptanceReport {
            items,
            attribution_granularity: str_at(
                m,
                "attribution_granularity",
                "EvolutionAcceptanceReport",
            )?
            .to_string(),
            portability_label,
        })
    }
}
