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
/// slots (§05h §4's `{uniform | fractional_design | concentrate | voi}` —
/// `concentrate`/`voi` land at S6.3a: `concentrate` is the explicit
/// concentrate-on-winners mode; `voi` is the value-of-information mode whose
/// only admissible input is the §05e scheduler's `slot_history` corpus
/// layer). Every mode carries per-slot `floors` (`map<slot, min_ppm>` —
/// class ids or hosted coordinate names) whose sum must stay ≤ the whole
/// and whose keys must name declared campaign slots; a uniform spread below
/// a declared floor is the `BudgetSplittingTrap` the open-time check reads.
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
    /// `concentrate{floors}` — the explicit concentration mode (6c):
    /// concentration above the per-slot floors is the declared intent —
    /// never a silent under-allocation.
    Concentrate {
        /// `slot → min_share_ppm` floor table (ppm; sum ≤ 1_000_000).
        floors: BTreeMap<String, u64>,
    },
    /// `voi{floors}` — value-of-information allocation over the
    /// `slot_history` corpus layer (R-2.9.5 6c; ADR-0190's scheduler
    /// seam). The bound experiment must run `voi_weighted` validation.
    Voi {
        /// `slot → min_share_ppm` floor table (ppm; sum ≤ 1_000_000).
        floors: BTreeMap<String, u64>,
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
            SlotAllocation::Concentrate { floors } => Json::obj([(
                "concentrate",
                Json::obj([(
                    "floors",
                    Json::Obj(
                        floors
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                            .collect(),
                    ),
                )]),
            )]),
            SlotAllocation::Voi { floors } => Json::obj([(
                "voi",
                Json::obj([(
                    "floors",
                    Json::Obj(
                        floors
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                            .collect(),
                    ),
                )]),
            )]),
        }
    }

    /// The mode spelling (`uniform|fractional_design|concentrate|voi`).
    pub fn mode(&self) -> &'static str {
        match self {
            SlotAllocation::Uniform { .. } => "uniform",
            SlotAllocation::FractionalDesign { .. } => "fractional_design",
            SlotAllocation::Concentrate { .. } => "concentrate",
            SlotAllocation::Voi { .. } => "voi",
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SlotAllocation, SchemaError> {
        let m = expect_obj(j, "slot_allocation")?;
        if m.len() != 1 {
            return Err(SchemaError::v(
                "slot_allocation",
                "must be a one-key {uniform|fractional_design|concentrate|voi} object",
            ));
        }
        let (k, v) = m.iter().next().expect("len checked");
        let vm = expect_obj(v, "slot_allocation")?;
        let floors_of =
            |vm: &BTreeMap<String, Json>| -> Result<BTreeMap<String, u64>, SchemaError> {
                reject_unknown(vm, &["floors"], "floored allocation")?;
                let mut floors = BTreeMap::new();
                match member_at(vm, "floors", "floored allocation")? {
                    Json::Obj(sm) => {
                        for (s, v) in sm {
                            floors.insert(
                                s.clone(),
                                v.as_int().ok_or_else(|| {
                                    SchemaError::v("floors", "share must be an integer (ppm)")
                                })? as u64,
                            );
                        }
                    }
                    _ => {
                        return Err(SchemaError::v(
                            "floors",
                            "must be a map<slot, min_share_ppm>",
                        ))
                    }
                }
                Ok(floors)
            };
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
            "concentrate" => Ok(SlotAllocation::Concentrate {
                floors: floors_of(vm)?,
            }),
            "voi" => Ok(SlotAllocation::Voi {
                floors: floors_of(vm)?,
            }),
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
    /// The declared corpus layers (§05h §2.2's `EvidenceCorpus.layers` —
    /// S6.2, R-2.4.4⁴): a closed subset of [`CORPUS_LAYERS`];
    /// `memory_lineage` admits `derived_from`/`supersedes` chains of
    /// `Memory` versions as evidence refs. `None` = the corpus's
    /// evidence domain stands as at 6a. A `held_out`/`private`/`secrets`
    /// layer name is a leak — `HeldOutLeak` at S0, never a schema slip.
    pub layers: Option<Vec<String>>,
}

/// The closed corpus-layer set (§05h §2.2; R-2.4.4⁴ adds
/// `memory_lineage` — evolution candidates read memory lineage as
/// evidence). `model_io` is admitted only under the campaign's `readers`
/// policy and consent (S6.2 gates it on `corpus.readers` non-empty).
pub const CORPUS_LAYERS: &[&str] = &[
    "outcomes",
    "ledger_views",
    "model_io",
    "artifacts",
    "prior_candidates",
    "slot_history",
    "memory_lineage",
    "reference_trajectories",
];

/// The layer names that name a held-out surface — refused `HeldOutLeak`
/// at S0 regardless of set membership.
pub const FORBIDDEN_LAYERS: &[&str] = &["held_out", "private", "validators", "secrets"];

/// The §5c evolvable target classes (R-2.4.1⁴/2.4.2⁴/2.4.4⁴, R-2.6.4⁴,
/// R-2.6.5⁴ — the one-class-per-campaign set an automated `target_class`
/// names). `evolution_proposer` itself is never a member (X6). S6.3a adds
/// `code_payload` — the `CompiledPayload` leaf bodies code-search families
/// rewrite (typed + out-of-process only; S1/S7 gate the interface).
pub const EVOLVABLE_TARGET_CLASSES: &[&str] = &[
    "guideline",
    "compaction_guideline",
    "memory_lineage",
    "scheduling_rule",
    "coordination_policy",
    "code_payload",
];

/// The automated `proposer_family` spellings the campaign admits
/// (§05h §2.4 — `human` is the 6a path, never an `evolution_proposer`
/// variant). `ahe` is the 6b instrument-grade family restricted to one
/// target class; `rho` (retrospective, needs `reference_trajectories`),
/// `gene_bank` (quality-diversity niche archive) and `code_search`
/// (`CompiledPayload` candidates) are the 6c research-grade families —
/// every research-family campaign carries `research-grade` in
/// `maturity_flags` and its results render `preview` (AC-R-2.9.5-5).
pub const PROPOSER_FAMILIES: &[&str] = &["ahe", "rho", "gene_bank", "code_search"];

/// The 6c research-grade families — the members of [`PROPOSER_FAMILIES`]
/// whose campaigns/results are `preview`-walled by construction.
pub const RESEARCH_FAMILIES: &[&str] = &["rho", "gene_bank", "code_search"];

/// Whether `family` is a 6c research-grade family spelling.
pub fn is_research_family(family: &str) -> bool {
    RESEARCH_FAMILIES.contains(&family)
}

impl CorpusSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
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
        ]);
        if let Some(l) = &self.layers {
            if let Json::Obj(m) = &mut j {
                m.insert(
                    "layers".into(),
                    Json::Arr(l.iter().map(Json::str).collect()),
                );
            }
        }
        j
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
                "layers",
            ],
            "corpus",
        )?;
        let layers = match m.get("layers") {
            Some(Json::Arr(a)) => Some(
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => None,
        };
        Ok(CorpusSpec {
            environments: enum_vec_at(m, "environments", "corpus", |j| {
                j.as_str().and_then(EnvironmentFamily::parse)
            })?,
            suite_refs: str_vec_at(m, "suite_refs", "corpus")?,
            metric_refs: str_vec_at(m, "metric_refs", "corpus")?,
            task_ids: str_vec_at(m, "task_ids", "corpus")?,
            evidence_refs: str_vec_at(m, "evidence_refs", "corpus")?,
            split_assignment_ref: str_at(m, "split_assignment_ref", "corpus")?.to_string(),
            layers,
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
    /// `protocol` — `human_proposed` (6a) or `automated` (6b: an automated
    /// proposer family drives the campaign).
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
    /// The registered `evolution_proposer` variant ref — mandatory when
    /// `proposer_family != "human"` (6b; `exactly-one per campaign` — a
    /// single ref, never a list).
    pub proposer_variant_ref: Option<String>,
    /// The one component class an automated-family campaign may target
    /// (§05h §2.5 `ProposalConstraints.target_classes[]`, length 1 at
    /// 6b) — mandatory with `proposer_variant_ref`; the admitted entity
    /// kinds ride `allowed_target_kinds`.
    pub target_class: Option<String>,
    /// The 6c multi-family surface — the full component-class set an
    /// automated campaign may search (`§05h §2.5`, R-2.9.5 6c). `ahe`
    /// campaigns keep the exactly-one rule (`target_class` carries it;
    /// `target_classes` must be empty or equal `{target_class}`); the
    /// research families (`rho`, `gene_bank`, `code_search`) may name
    /// several members of [`EVOLVABLE_TARGET_CLASSES`].
    pub target_classes: Vec<String>,
    /// The hosted coordinate names the campaign admits on
    /// `hh-hosting/1` participants (S6.3a; ADR-0196 D7 — the C4→C2
    /// edge). Non-empty requires `hosted_participants`; every name must
    /// resolve in a deposited [`HostedCoordinateDescriptor`] at open and
    /// carry `capability = "supported"`. A diff op on a `coordinates.`
    /// path outside this set is `HostedCoordinateUnsupported` at S1; a
    /// structural coordinate edit is refused outright (D7 — value
    /// edits only).
    pub hosted_coordinates: Vec<String>,
    /// The `hosted_coordinate` document refs/names the open gate reads
    /// (records-in — the descriptor's `capability_vector` +
    /// `budget_enforcement` is the admission surface the hosted
    /// participant's `hh-hosting/1` declaration mirrors).
    pub hosted_descriptor_refs: Vec<String>,
    /// `authority_cap` — the experiment-level grant ceiling a candidate
    /// proposal's `install` leg may name (R-2.8.5²). A requested grant
    /// outside this set widens authority → `AuthorityWidening` at S1;
    /// `None` = the campaign admits no model install at all.
    pub authority_cap: Option<Vec<String>>,
    /// The S9 rollout declaration — `split` canaries require it
    /// (`share_ppm`, `assignment_seed`, `advance_rule`); `shadow` remains
    /// the committable default.
    pub rollout_policy: Option<RolloutPolicy>,
    /// The G7 judge-selector table + audit budget (6b) — a `selected_
    /// best_of_n` baseline naming a judge selector must resolve against
    /// this table (calibrated, independent, honeypots in the
    /// counterexample set, `audited_share` reported).
    pub judge_policy: Option<JudgePolicy>,
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
        if !matches!(self.protocol.as_str(), "human_proposed" | "automated") {
            return Err(SchemaViolation {
                detail: format!(
                    "protocol `{}` — `human_proposed` (6a) or `automated` (6b) only",
                    self.protocol
                ),
            });
        }
        if self.proposer_family != "human"
            && !PROPOSER_FAMILIES.iter().any(|f| *f == self.proposer_family)
        {
            return Err(SchemaViolation {
                detail: format!(
                    "proposer_family `{}` — `human` (6a) or one of {PROPOSER_FAMILIES:?}",
                    self.proposer_family
                ),
            });
        }
        // The converse leg: a human family's protocol is `human_proposed`
        // — `automated` without a declared family is incoherent.
        if self.proposer_family == "human" && self.protocol != "human_proposed" {
            return Err(SchemaViolation {
                detail: format!(
                    "proposer_family `human` requires protocol = `human_proposed`, \
                     got `{}`",
                    self.protocol
                ),
            });
        }
        // The 6b automated family: `protocol = automated`, a registered
        // `evolution_proposer` variant, and exactly one target class
        // (§05h §2.5 — the AHE-shaped variant is restricted to one
        // component class per campaign).
        if self.proposer_family != "human" {
            if self.protocol != "automated" {
                return Err(SchemaViolation {
                    detail: format!(
                        "proposer_family `{}` requires protocol = `automated`",
                        self.proposer_family
                    ),
                });
            }
            if self.proposer_variant_ref.is_none() {
                return Err(SchemaViolation {
                    detail: "an automated family needs `proposer_variant_ref` — the \
                             registered `evolution_proposer` variant"
                        .into(),
                });
            }
            // 6c — the research families are `research-grade` by
            // construction: the flag is mandatory and the campaign's
            // results carry the `preview` wall (AC-R-2.9.5-5).
            if is_research_family(&self.proposer_family)
                && !self.maturity_flags.iter().any(|f| f == "research-grade")
            {
                return Err(SchemaViolation {
                    detail: format!(
                        "proposer_family `{}` is a 6c research family — \
                         maturity_flags must carry `research-grade`",
                        self.proposer_family
                    ),
                });
            }
            match &self.target_class {
                Some(c) if !c.is_empty() && self.allowed_target_kinds.is_some() => {
                    if !EVOLVABLE_TARGET_CLASSES.iter().any(|t| t == c) {
                        return Err(SchemaViolation {
                            detail: format!(
                                "target_class `{c}` is not in the evolvable set \
                                 {EVOLVABLE_TARGET_CLASSES:?} (§5c)"
                            ),
                        });
                    }
                }
                _ => {
                    return Err(SchemaViolation {
                        detail: "an automated family names its `target_class` and \
                                 its `allowed_target_kinds` (the class's op surface)"
                            .into(),
                    })
                }
            }
            if self.proposer_family == "ahe" {
                // The 6b instrument-grade rule stays: `ahe` is exactly
                // one class per campaign — `target_classes` must be
                // empty or name the same single class.
                let extra: Vec<&String> = self
                    .target_classes
                    .iter()
                    .filter(|c| Some(*c) != self.target_class.as_ref())
                    .collect();
                if !extra.is_empty() {
                    return Err(SchemaViolation {
                        detail: format!(
                            "proposer_family `ahe` is exactly-one-class — \
                             `target_classes {extra:?}` names more than `target_class`"
                        ),
                    });
                }
            } else {
                // 6c research families may be multi-class: every member
                // of `target_classes` must be in the evolvable set.
                for c in &self.target_classes {
                    if !EVOLVABLE_TARGET_CLASSES.iter().any(|t| t == c) {
                        return Err(SchemaViolation {
                            detail: format!(
                                "target_classes member `{c}` is not in the evolvable set \
                                 {EVOLVABLE_TARGET_CLASSES:?} (§5c)"
                            ),
                        });
                    }
                }
            }
        }
        // Human-family campaigns may still name the single class the
        // campaign addresses — membership stays closed either way.
        if let Some(c) = &self.target_class {
            if !EVOLVABLE_TARGET_CLASSES.iter().any(|t| t == c) {
                return Err(SchemaViolation {
                    detail: format!(
                        "target_class `{c}` is not in the evolvable set \
                         {EVOLVABLE_TARGET_CLASSES:?}"
                    ),
                });
            }
        }
        for c in &self.target_classes {
            if !EVOLVABLE_TARGET_CLASSES.iter().any(|t| t == c) {
                return Err(SchemaViolation {
                    detail: format!(
                        "target_classes member `{c}` is not in the evolvable set \
                         {EVOLVABLE_TARGET_CLASSES:?}"
                    ),
                });
            }
        }
        // `memory:`-prefixed evidence refs resolve through the lineage
        // layer (R-2.4.4⁴) — the layer must be declared; an undeclared
        // `memory:` ref is a schema violation, never silently admitted.
        let lineage_ok = self
            .corpus
            .layers
            .as_ref()
            .map(|l| l.iter().any(|x| x == "memory_lineage"))
            .unwrap_or(false);
        for r in &self.corpus.evidence_refs {
            if r.starts_with("memory:") && !lineage_ok {
                return Err(SchemaViolation {
                    detail: format!(
                        "evidence ref `{r}` is `memory:`-prefixed but the corpus \
                         declares no `memory_lineage` layer"
                    ),
                });
            }
        }
        // `split` canaries require a declared split rollout policy (the
        // recorded assignment seed + share are the picker's evidence).
        if let Some(rp) = &self.rollout_policy {
            if !matches!(rp.mode.as_str(), "shadow" | "split") {
                return Err(SchemaViolation {
                    detail: format!("rollout_policy.mode `{}` — `shadow`|`split`", rp.mode),
                });
            }
            if rp.mode == "split"
                && (rp.share_ppm == 0 || rp.share_ppm > 1_000_000 || rp.assignment_seed.is_empty())
            {
                return Err(SchemaViolation {
                    detail: "split rollout needs `share_ppm ∈ (0,1_000_000]` and a recorded \
                             `assignment_seed`"
                        .into(),
                });
            }
        }
        // G7 — a declared judge policy needs its audit budget and at
        // least one calibrated selector.
        if let Some(jp) = &self.judge_policy {
            if jp.audit_budget_ref.is_empty() {
                return Err(SchemaViolation {
                    detail: "judge_policy.audit_budget_ref absent — the audit budget is \
                             charged_to = instrument"
                        .into(),
                });
            }
            for s in &jp.selectors {
                if s.kind != "judge" {
                    return Err(SchemaViolation {
                        detail: format!(
                            "selector `{}` kind `{}` — `judge` only",
                            s.selector_ref, s.kind
                        ),
                    });
                }
                if s.calibration_ref.is_empty() {
                    return Err(SchemaViolation {
                        detail: format!(
                            "selector `{}` without a CalibrationRecord — G7",
                            s.selector_ref
                        ),
                    });
                }
            }
        }
        // Corpus layers — a held-out surface name is a leak, never a
        // schema slip; `model_io` only under a declared readers policy
        // (the corpus's `model_io` flag + a non-empty evidence domain).
        if let Some(layers) = &self.corpus.layers {
            for l in layers {
                if FORBIDDEN_LAYERS.iter().any(|f| f == l) {
                    return Err(HeldOutLeak {
                        detail: format!("corpus layer `{l}` names a held-out surface"),
                    });
                }
                if !CORPUS_LAYERS.iter().any(|c| c == l) {
                    return Err(SchemaViolation {
                        detail: format!("corpus layer `{l}` — not in the closed layer set"),
                    });
                }
            }
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
            SlotAllocation::Concentrate { floors } | SlotAllocation::Voi { floors }
                if floors.values().sum::<u64>() > 1_000_000 =>
            {
                return Err(BudgetSplittingTrap {
                    detail: format!(
                        "{} floors sum to {} ppm — over the whole",
                        self.slot_allocation.mode(),
                        floors.values().sum::<u64>()
                    ),
                })
            }
            _ => {}
        }
        // `voi` — the only admissible input is the `slot_history` corpus
        // layer (ADR-0190's scheduler seam; §05h §4's budget.rule_set
        // member). A VOI campaign whose corpus does not declare
        // `slot_history` is malformed at open.
        if matches!(self.slot_allocation, SlotAllocation::Voi { .. }) {
            let has_history = self
                .corpus
                .layers
                .as_ref()
                .map(|l| l.iter().any(|x| x == "slot_history"))
                .unwrap_or(false);
            if !has_history {
                return Err(SchemaViolation {
                    detail: "slot_allocation.mode = `voi` requires corpus layer \
                             `slot_history` — the VOI input is the scheduler's recorded \
                             slot history only"
                        .into(),
                });
            }
        }
        // Floored allocations — a floor names a declared slot: a target
        // class or an admitted hosted coordinate (the concentration
        // surface is never an unlabeled leak).
        let floors: Vec<String> = match &self.slot_allocation {
            SlotAllocation::FractionalDesign { slots } => slots.keys().cloned().collect(),
            SlotAllocation::Concentrate { floors } | SlotAllocation::Voi { floors } => {
                floors.keys().cloned().collect()
            }
            _ => Vec::new(),
        };
        for f in &floors {
            let declared = self.target_classes.iter().any(|c| c == f)
                || self.target_class.as_ref().map(|c| c == f).unwrap_or(false)
                || self.hosted_coordinates.iter().any(|c| c == f)
                || f == "native";
            if !declared {
                return Err(SchemaViolation {
                    detail: format!(
                        "slot floor `{f}` names no declared slot — floors cover target \
                         classes, admitted hosted coordinates, or `native`"
                    ),
                });
            }
        }
        // Hosted coordinates (6c — ADR-0196 D7): declaring coordinate
        // admissions requires `hosted_participants` and the descriptor
        // docs the admission gate resolves at open.
        if !self.hosted_coordinates.is_empty() && !self.hosted_participants {
            return Err(SchemaViolation {
                detail: "hosted_coordinates is non-empty but hosted_participants = false \
                         — coordinate admission is a hosted campaign surface"
                    .into(),
            });
        }
        if !self.hosted_coordinates.is_empty() && self.hosted_descriptor_refs.is_empty() {
            return Err(UnresolvableEvidence {
                ref_: "hosted_descriptor_refs is empty — every admitted coordinate must \
                       resolve in a deposited `hosted_coordinate` descriptor at open"
                    .into(),
            });
        }
        // AC-15 (R-2.9.5 §2.5) — a hosted campaign claiming a
        // reported-only dimension cannot satisfy `matched_total`: the
        // campaign refuses at open rather than minting an
        // incommensurable comparison downstream.
        if self.hosted_participants && !self.reported_only_dimensions.is_empty() {
            return Err(IncommensurableMatch {
                detail: format!(
                    "hosted_participants + reported_only_dimensions {:?} — a \
                     matched_total claim is incommensurable (AC-15)",
                    self.reported_only_dimensions
                ),
            });
        }
        // `authority_cap` — grant spellings are `domain:scope` (empty or
        // malformed members are a schema slip, never an admission).
        if let Some(cap) = &self.authority_cap {
            for g in cap {
                if !g.contains(':') {
                    return Err(SchemaViolation {
                        detail: format!(
                            "authority_cap member `{g}` is not a `domain:scope` grant \
                             spelling"
                        ),
                    });
                }
            }
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
        if let Some(p) = &self.proposer_variant_ref {
            m.insert("proposer_variant_ref".into(), Json::str(p));
        }
        if let Some(c) = &self.target_class {
            m.insert("target_class".into(), Json::str(c));
        }
        if !self.target_classes.is_empty() {
            m.insert(
                "target_classes".into(),
                Json::Arr(self.target_classes.iter().map(Json::str).collect()),
            );
        }
        if !self.hosted_coordinates.is_empty() {
            m.insert(
                "hosted_coordinates".into(),
                Json::Arr(self.hosted_coordinates.iter().map(Json::str).collect()),
            );
        }
        if !self.hosted_descriptor_refs.is_empty() {
            m.insert(
                "hosted_descriptor_refs".into(),
                Json::Arr(self.hosted_descriptor_refs.iter().map(Json::str).collect()),
            );
        }
        if let Some(cap) = &self.authority_cap {
            m.insert(
                "authority_cap".into(),
                Json::Arr(cap.iter().map(Json::str).collect()),
            );
        }
        if let Some(rp) = &self.rollout_policy {
            m.insert("rollout_policy".into(), rp.to_json());
        }
        if let Some(jp) = &self.judge_policy {
            m.insert("judge_policy".into(), jp.to_json());
        }
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
                "proposer_variant_ref",
                "target_class",
                "target_classes",
                "hosted_coordinates",
                "hosted_descriptor_refs",
                "authority_cap",
                "rollout_policy",
                "judge_policy",
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
            proposer_variant_ref: opt_str_at(m, "proposer_variant_ref")?.map(str::to_string),
            target_class: opt_str_at(m, "target_class")?.map(str::to_string),
            target_classes: match m.get("target_classes") {
                None | Some(Json::Null) => Vec::new(),
                _ => str_vec_at(m, "target_classes", "EvolutionCampaignSpec")?,
            },
            hosted_coordinates: match m.get("hosted_coordinates") {
                None | Some(Json::Null) => Vec::new(),
                _ => str_vec_at(m, "hosted_coordinates", "EvolutionCampaignSpec")?,
            },
            hosted_descriptor_refs: match m.get("hosted_descriptor_refs") {
                None | Some(Json::Null) => Vec::new(),
                _ => str_vec_at(m, "hosted_descriptor_refs", "EvolutionCampaignSpec")?,
            },
            authority_cap: match m.get("authority_cap") {
                None | Some(Json::Null) => None,
                _ => Some(str_vec_at(m, "authority_cap", "EvolutionCampaignSpec")?),
            },
            rollout_policy: match m.get("rollout_policy") {
                Some(Json::Obj(_)) => Some(RolloutPolicy::from_json(&m["rollout_policy"])?),
                _ => None,
            },
            judge_policy: match m.get("judge_policy") {
                Some(Json::Obj(_)) => Some(JudgePolicy::from_json(&m["judge_policy"])?),
                _ => None,
            },
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
    /// `reference_trajectories` (S6.3a) — the corpus evidence refs the
    /// hypothesis anchors its prediction to (the RHO selector's
    /// trajectory set; §05h §2.2's `reference_trajectories` layer).
    /// Non-empty requires the corpus to declare the layer and every ref
    /// must resolve in `corpus.evidence_refs` (S2).
    pub reference_trajectories: Vec<String>,
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
    /// `install` (S6.3a; R-2.8.5²) — the model install the candidate
    /// requests under the campaign's `authority_cap`. `Some` requires the
    /// spec to declare `authority_cap`; requested grants outside the cap
    /// are `AuthorityWidening` at S1.
    pub install: Option<InstallRequest>,
    /// `coordinate_values` (S6.3a; ADR-0196 D7) — the hosted-coordinate
    /// value assignments the candidate proposes (`{coordinate: value}` —
    /// scalar values only; the C4→C2 edge admits *value* edits, never a
    /// structural coordinate change). Every key must be a member of the
    /// spec's `hosted_coordinates`, `supported` in the deposited
    /// descriptor, and `enforced` under `budget_enforcement` — anything
    /// else is `HostedCoordinateUnsupported`/`ReportedOnlyDimension`
    /// at S1.
    pub coordinate_values: BTreeMap<String, Json>,
}

/// `install` — the candidate's model-install request (S6.3a;
/// R-2.8.5²): the named extension/kind plus the grant spellings the
/// install plan must satisfy. The candidate's `requested_grants` must be
/// a subset of the experiment's `authority_cap` *and* of the proposer's
/// own grant set — both legs run (cap first, proposer widening second).
#[derive(Debug, Clone, PartialEq)]
pub struct InstallRequest {
    /// The extension/model name to install.
    pub name: String,
    /// The extension kind (`model` for a model fetch — §05c §3's
    /// install contract).
    pub kind: String,
    /// The grant spellings the install requests (`domain:scope`).
    pub requested_grants: Vec<String>,
}

impl InstallRequest {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("name", Json::str(&self.name)),
            ("kind", Json::str(&self.kind)),
            (
                "requested_grants",
                Json::Arr(self.requested_grants.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<InstallRequest, SchemaError> {
        let m = expect_obj(j, "InstallRequest")?;
        reject_unknown(m, &["name", "kind", "requested_grants"], "InstallRequest")?;
        Ok(InstallRequest {
            name: str_at(m, "name", "InstallRequest")?.to_string(),
            kind: str_at(m, "kind", "InstallRequest")?.to_string(),
            requested_grants: str_vec_at(m, "requested_grants", "InstallRequest")?,
        })
    }
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
            (
                "reference_trajectories",
                Json::Arr(self.reference_trajectories.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<FailureHypothesis, SchemaError> {
        let m = expect_obj(j, "FailureHypothesis")?;
        reject_unknown(
            m,
            &[
                "kind",
                "evidence_refs",
                "predicted",
                "semantic_op_targets",
                "reference_trajectories",
            ],
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
            reference_trajectories: match m.get("reference_trajectories") {
                None | Some(Json::Null) => Vec::new(),
                _ => str_vec_at(m, "reference_trajectories", "FailureHypothesis")?,
            },
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
            (
                "install",
                self.install
                    .as_ref()
                    .map(|i| i.to_json())
                    .unwrap_or(Json::Null),
            ),
            (
                "coordinate_values",
                Json::Obj(
                    self.coordinate_values
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CandidateProposal, SchemaError> {
        let m = expect_obj(j, "CandidateProposal")?;
        reject_unknown(
            m,
            &[
                "base_ref",
                "diff",
                "slot",
                "hypothesis",
                "install",
                "coordinate_values",
            ],
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
        let install = match m.get("install") {
            Some(Json::Null) | None => None,
            Some(i) => Some(InstallRequest::from_json(i)?),
        };
        let mut coordinate_values = BTreeMap::new();
        match m.get("coordinate_values") {
            Some(Json::Obj(cm)) => {
                for (k, v) in cm {
                    coordinate_values.insert(k.clone(), v.clone());
                }
            }
            Some(Json::Null) | None => {}
            _ => {
                return Err(SchemaError::v(
                    "coordinate_values",
                    "must be a map<coordinate, value>",
                ))
            }
        }
        Ok(CandidateProposal {
            base_ref: str_at(m, "base_ref", "CandidateProposal")?.to_string(),
            diff,
            slot: str_at(m, "slot", "CandidateProposal")?.to_string(),
            hypothesis,
            install,
            coordinate_values,
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
    /// The judge selector the screen's counterexample picks went through
    /// (G7, 6b) — a `judge`-kind `SelectorDeclaration` on the campaign's
    /// `judge_policy`, or absent (deterministic screen).
    pub selector_ref: Option<String>,
    /// The honeypot items the counterexample set carries (G7's trap
    /// surface — the campaign's `judge_policy.min_honeypots` floor
    /// applies when `selector_ref` names a judge).
    pub honeypots: u32,
}

impl ScreenReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "counterexample_set_ref".into(),
            Json::str(&self.counterexample_set_ref),
        );
        m.insert("observations".into(), Json::Int(self.observations as i64));
        m.insert(
            "flips_in_direction".into(),
            Json::Int(self.flips_in_direction as i64),
        );
        m.insert(
            "replicate_count".into(),
            Json::Int(self.replicate_count as i64),
        );
        m.insert(
            "split_labels_used".into(),
            Json::Arr(
                self.split_labels_used
                    .iter()
                    .map(|l| Json::str(l.name()))
                    .collect(),
            ),
        );
        m.insert("judge_only".into(), Json::Bool(self.judge_only));
        if let Some(s) = &self.selector_ref {
            m.insert("selector_ref".into(), Json::str(s));
        }
        if self.honeypots > 0 {
            m.insert("honeypots".into(), Json::Int(self.honeypots as i64));
        }
        Json::Obj(m)
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
                "selector_ref",
                "honeypots",
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
            selector_ref: opt_str_at(m, "selector_ref")?.map(str::to_string),
            honeypots: opt_int_at(m, "honeypots")?.unwrap_or(0) as u32,
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
    /// `label` — the maturity label the report renders under
    /// (S6.3a; AC-R-2.9.5-5): a research-grade campaign's report must
    /// carry `preview` (the `n/a{research-grade}` wall — such results
    /// never enter C0–C3 acceptance or leaderboards without independent
    /// reproduction). `None` = no maturity label claimed.
    pub label: Option<String>,
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
            (
                "label",
                self.label.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
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
                "label",
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
            label: opt_str_at(m, "label")?.map(str::to_string),
        })
    }
}

// ── S6.2 — the automated proposer family (R-2.9.5 6b; ADR-0196) ─────────────

/// The reflexive tri-state (T-LCD-07) — `declare()` fields answer
/// `yes|no|unknown`; `unknown` is never coerced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    /// `yes`.
    Yes,
    /// `no`.
    No,
    /// `unknown`.
    Unknown,
}

impl Tri {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Tri::Yes => "yes",
            Tri::No => "no",
            Tri::Unknown => "unknown",
        }
    }
    /// Strict parse.
    pub fn parse(s: &str) -> Option<Tri> {
        match s {
            "yes" | "true" => Some(Tri::Yes),
            "no" | "false" => Some(Tri::No),
            "unknown" => Some(Tri::Unknown),
            _ => None,
        }
    }
}

/// `ParentSelectionPolicy` — `select_parent`'s closed policy sum (§05h
/// §2.4; ADR-0196 D3). Policies are campaign data; the op is pure.
#[derive(Debug, Clone, PartialEq)]
pub enum ParentSelectionPolicy {
    /// `best` — the highest-scoring lineage member.
    Best,
    /// `score_proportional{alpha, uniform_mix}` — softmax-ish pick with a
    /// uniform exploration floor (ppm).
    ScoreProportional {
        /// The temperature (ppm — integer arithmetic only).
        alpha_ppm: u64,
        /// The uniform exploration mix (ppm).
        uniform_mix_ppm: u64,
    },
    /// `score_child_proportional{alpha, children_penalty}` — score per
    /// child penalised by realised descendant count (ppm).
    ScoreChildProportional {
        /// The temperature (ppm).
        alpha_ppm: u64,
        /// The per-child penalty (ppm).
        children_penalty_ppm: u64,
    },
    /// `pareto_per_task` — a task-bucketed Pareto pick.
    ParetoPerTask,
    /// `gene_bank{niche}` (S6.3a) — the elite/gene-bank arm: pick the
    /// highest-scoring lineage member whose `niche` matches (the
    /// quality-diversity archive's niche-qualified elite; a niche with
    /// no member falls back to the lineage-wide elite).
    GeneBank {
        /// The niche spelling to select within.
        niche: String,
    },
    /// `rho{trajectory_ref}` (S6.3a) — the retrospective trajectory-
    /// anchored arm: pick the highest-scoring lineage member whose
    /// `trajectory_refs` cite the named reference trajectory.
    Rho {
        /// The reference-trajectory evidence ref the pick anchors to.
        trajectory_ref: String,
    },
}

impl ParentSelectionPolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            ParentSelectionPolicy::Best => Json::str("best"),
            ParentSelectionPolicy::ScoreProportional {
                alpha_ppm,
                uniform_mix_ppm,
            } => Json::obj([(
                "score_proportional",
                Json::obj([
                    ("alpha", Json::Int(*alpha_ppm as i64)),
                    ("uniform_mix", Json::Int(*uniform_mix_ppm as i64)),
                ]),
            )]),
            ParentSelectionPolicy::ScoreChildProportional {
                alpha_ppm,
                children_penalty_ppm,
            } => Json::obj([(
                "score_child_proportional",
                Json::obj([
                    ("alpha", Json::Int(*alpha_ppm as i64)),
                    ("children_penalty", Json::Int(*children_penalty_ppm as i64)),
                ]),
            )]),
            ParentSelectionPolicy::ParetoPerTask => Json::str("pareto_per_task"),
            ParentSelectionPolicy::GeneBank { niche } => {
                Json::obj([("gene_bank", Json::obj([("niche", Json::str(niche))]))])
            }
            ParentSelectionPolicy::Rho { trajectory_ref } => Json::obj([(
                "rho",
                Json::obj([("trajectory_ref", Json::str(trajectory_ref))]),
            )]),
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ParentSelectionPolicy, SchemaError> {
        match j {
            Json::Str(s) if s == "best" => Ok(ParentSelectionPolicy::Best),
            Json::Str(s) if s == "pareto_per_task" => Ok(ParentSelectionPolicy::ParetoPerTask),
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next().expect("len checked");
                let vm = expect_obj(v, "ParentSelectionPolicy")?;
                match k.as_str() {
                    "score_proportional" => {
                        reject_unknown(vm, &["alpha", "uniform_mix"], "score_proportional")?;
                        Ok(ParentSelectionPolicy::ScoreProportional {
                            alpha_ppm: int_at(vm, "alpha", "score_proportional")? as u64,
                            uniform_mix_ppm: int_at(vm, "uniform_mix", "score_proportional")?
                                as u64,
                        })
                    }
                    "score_child_proportional" => {
                        reject_unknown(
                            vm,
                            &["alpha", "children_penalty"],
                            "score_child_proportional",
                        )?;
                        Ok(ParentSelectionPolicy::ScoreChildProportional {
                            alpha_ppm: int_at(vm, "alpha", "score_child_proportional")? as u64,
                            children_penalty_ppm: int_at(
                                vm,
                                "children_penalty",
                                "score_child_proportional",
                            )? as u64,
                        })
                    }
                    "gene_bank" => {
                        reject_unknown(vm, &["niche"], "gene_bank")?;
                        Ok(ParentSelectionPolicy::GeneBank {
                            niche: str_at(vm, "niche", "gene_bank")?.to_string(),
                        })
                    }
                    "rho" => {
                        reject_unknown(vm, &["trajectory_ref"], "rho")?;
                        Ok(ParentSelectionPolicy::Rho {
                            trajectory_ref: str_at(vm, "trajectory_ref", "rho")?.to_string(),
                        })
                    }
                    _ => Err(SchemaError::v(
                        "ParentSelectionPolicy",
                        format!("unknown policy `{k}`"),
                    )),
                }
            }
            _ => Err(SchemaError::v(
                "ParentSelectionPolicy",
                "must be `best`|`pareto_per_task` or a one-key policy object",
            )),
        }
    }
}

/// One conditioned proposer rule and its `AssumptionDebtRecord` (the
/// static kind's completeness surface — a conditioned rule without a
/// record is `IncompleteDeclaration`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionedRuleDecl {
    /// The rule ref.
    pub rule_ref: String,
    /// The conditioned surface (`conditioned_on`).
    pub conditioned_on: String,
    /// The rule's `AssumptionDebtRecord` (canonical JSON) — `None` is the
    /// conformance failure.
    pub debt_record: Option<Json>,
}

impl ConditionedRuleDecl {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("rule_ref", Json::str(&self.rule_ref)),
            ("conditioned_on", Json::str(&self.conditioned_on)),
            (
                "debt_record",
                self.debt_record.clone().unwrap_or(Json::Null),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ConditionedRuleDecl, SchemaError> {
        let m = expect_obj(j, "ConditionedRuleDecl")?;
        reject_unknown(
            m,
            &["rule_ref", "conditioned_on", "debt_record"],
            "ConditionedRuleDecl",
        )?;
        let debt_record = match m.get("debt_record") {
            Some(Json::Null) | None => None,
            Some(d) => Some(d.clone()),
        };
        Ok(ConditionedRuleDecl {
            rule_ref: str_at(m, "rule_ref", "ConditionedRuleDecl")?.to_string(),
            conditioned_on: str_at(m, "conditioned_on", "ConditionedRuleDecl")?.to_string(),
            debt_record,
        })
    }
}

/// `ProposerDeclaration` — `declare()`'s output (§05h §2.4): `{family,
/// op_classes_admissible, needs_reference_trajectories, uses_judge,
/// judge_ref?, maturity}` + the conditioned-rule table the static kind
/// sweeps.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposerDeclaration {
    /// The proposer family (`ahe` at 6b).
    pub family: String,
    /// The component classes the proposer may target (`len == 1` at 6b).
    pub op_classes_admissible: Vec<String>,
    /// Whether the proposer needs reference trajectories (tri-state).
    pub needs_reference_trajectories: Tri,
    /// Whether the proposer calls a judge (tri-state; `no` + an observed
    /// judge call ⇒ DRIFT).
    pub uses_judge: Tri,
    /// The judge selector ref when `uses_judge = yes`.
    pub judge_ref: Option<String>,
    /// `instrument-grade` | `research-grade`.
    pub maturity: String,
    /// The proposer's model-conditioned rules (each needs a debt record).
    pub conditioned_rules: Vec<ConditionedRuleDecl>,
}

impl ProposerDeclaration {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("family".into(), Json::str(&self.family));
        m.insert(
            "op_classes_admissible".into(),
            Json::Arr(self.op_classes_admissible.iter().map(Json::str).collect()),
        );
        m.insert(
            "needs_reference_trajectories".into(),
            Json::str(self.needs_reference_trajectories.as_str()),
        );
        m.insert("uses_judge".into(), Json::str(self.uses_judge.as_str()));
        if let Some(j) = &self.judge_ref {
            m.insert("judge_ref".into(), Json::str(j));
        }
        m.insert("maturity".into(), Json::str(&self.maturity));
        m.insert(
            "conditioned_rules".into(),
            Json::Arr(self.conditioned_rules.iter().map(|r| r.to_json()).collect()),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ProposerDeclaration, SchemaError> {
        let m = expect_obj(j, "ProposerDeclaration")?;
        reject_unknown(
            m,
            &[
                "family",
                "op_classes_admissible",
                "needs_reference_trajectories",
                "uses_judge",
                "judge_ref",
                "maturity",
                "conditioned_rules",
            ],
            "ProposerDeclaration",
        )?;
        let tri = |k: &str, default: Tri| -> Result<Tri, SchemaError> {
            match m.get(k) {
                None => Ok(default),
                Some(Json::Str(s)) => {
                    Tri::parse(s).ok_or_else(|| SchemaError::v(k, "must be yes|no|unknown"))
                }
                Some(Json::Bool(b)) => Ok(if *b { Tri::Yes } else { Tri::No }),
                _ => Err(SchemaError::v(k, "must be yes|no|unknown")),
            }
        };
        Ok(ProposerDeclaration {
            family: str_at(m, "family", "ProposerDeclaration")?.to_string(),
            op_classes_admissible: str_vec_at(m, "op_classes_admissible", "ProposerDeclaration")?,
            needs_reference_trajectories: tri("needs_reference_trajectories", Tri::Unknown)?,
            uses_judge: tri("uses_judge", Tri::Unknown)?,
            judge_ref: opt_str_at(m, "judge_ref")?.map(str::to_string),
            maturity: str_at(m, "maturity", "ProposerDeclaration")?.to_string(),
            conditioned_rules: match m.get("conditioned_rules") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(ConditionedRuleDecl::from_json)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => Vec::new(),
            },
        })
    }
}

/// `ProposerOutcome` — `propose`'s typed output sum (§05h §2.4):
/// `[CandidateProposal ∪ FailureHypothesis] | NoAddressableFailure`. The
/// third arm is a typed outcome, never an empty list.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposerOutcome {
    /// A candidate proposal (boxed — the diff body is the large arm).
    Candidate(Box<CandidateProposal>),
    /// A bare hypothesis (no diff yet — the campaign binds it at S2).
    Hypothesis(FailureHypothesis),
    /// The corpus shows no addressable failure — a typed outcome with a
    /// reason (never an empty list).
    NoAddressableFailure {
        /// The reason (a closed vocabulary member or free detail).
        reason: String,
    },
}

/// `ProposerFailure` — `propose`'s typed failure sum (§05h §2.4):
/// `CorpusUnreadable | ConstraintUnsatisfiable | BudgetExhausted`.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposerFailure {
    /// The corpus ref did not resolve / a member failed the S0 surface
    /// checks.
    CorpusUnreadable {
        /// Detail.
        detail: String,
    },
    /// The proposal constraints admit no legal diff (empty target class,
    /// `max_semantic_ops = 0`, exclusion covers the whole target set).
    ConstraintUnsatisfiable {
        /// Detail.
        detail: String,
    },
    /// The proposer's budget slice is exhausted (`charged_to =
    /// instrument` — never a silent stop).
    BudgetExhausted {
        /// Detail.
        detail: String,
    },
}

impl ProposerFailure {
    /// The canonical code.
    pub fn code(&self) -> &'static str {
        match self {
            ProposerFailure::CorpusUnreadable { .. } => "corpus_unreadable",
            ProposerFailure::ConstraintUnsatisfiable { .. } => "constraint_unsatisfiable",
            ProposerFailure::BudgetExhausted { .. } => "budget_exhausted",
        }
    }
}

/// `RolloutPolicy` — the S9 rollout declaration (§05h §2.8; ADR-0195
/// D4): `shadow` branches hold irreversible effects back forever;
/// `split` serves `share_ppm` of live goals under the principal's
/// authority, picked by the recorded `assignment_seed` — split rows are
/// `exploratory` (`comparable = false`, CF-421); only `shadow` canaries
/// produce comparable evidence. `abort_on` names the veto metrics;
/// `advance_rule` the advance predicate (`min_runs` met ⇒ advance).
#[derive(Debug, Clone, PartialEq)]
pub struct RolloutPolicy {
    /// `shadow` | `split`.
    pub mode: String,
    /// `split`: the live-goal share (ppm).
    pub share_ppm: u64,
    /// The minimum settled runs before `advance_rule` may fire.
    pub min_runs: u64,
    /// The canary's maximum duration (ms).
    pub max_duration_ms: u64,
    /// The veto metrics an abort watches.
    pub abort_on: Vec<String>,
    /// The advance predicate (`min_runs_met` | `manual`).
    pub advance_rule: String,
    /// `split`: the recorded assignment seed (the deterministic picker).
    pub assignment_seed: String,
}

impl RolloutPolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("mode", Json::str(&self.mode)),
            ("share_ppm", Json::Int(self.share_ppm as i64)),
            ("min_runs", Json::Int(self.min_runs as i64)),
            ("max_duration_ms", Json::Int(self.max_duration_ms as i64)),
            (
                "abort_on",
                Json::Arr(self.abort_on.iter().map(Json::str).collect()),
            ),
            ("advance_rule", Json::str(&self.advance_rule)),
            ("assignment_seed", Json::str(&self.assignment_seed)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<RolloutPolicy, SchemaError> {
        let m = expect_obj(j, "RolloutPolicy")?;
        reject_unknown(
            m,
            &[
                "mode",
                "share_ppm",
                "min_runs",
                "max_duration_ms",
                "abort_on",
                "advance_rule",
                "assignment_seed",
            ],
            "RolloutPolicy",
        )?;
        Ok(RolloutPolicy {
            mode: str_at(m, "mode", "RolloutPolicy")?.to_string(),
            share_ppm: opt_int_at(m, "share_ppm")?.unwrap_or(0) as u64,
            min_runs: opt_int_at(m, "min_runs")?.unwrap_or(0) as u64,
            max_duration_ms: opt_int_at(m, "max_duration_ms")?.unwrap_or(0) as u64,
            abort_on: match m.get("abort_on") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            },
            advance_rule: opt_str_at(m, "advance_rule")?
                .unwrap_or("manual")
                .to_string(),
            assignment_seed: opt_str_at(m, "assignment_seed")?.unwrap_or("").to_string(),
        })
    }
}

/// `SelectorDeclaration` — one judge selector the campaign admits (G7;
/// §05h §4 S3–S4): judges are *selectors only* inside S3–S4 —
/// calibrated, independent of the beneficiary snapshot, and their
/// counterexample sets carry honeypots.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorDeclaration {
    /// The selector's registry ref.
    pub selector_ref: String,
    /// `judge` only at 6b (the closed kind).
    pub kind: String,
    /// The selector's `CalibrationRecord` ref — required, never `None`.
    pub calibration_ref: String,
    /// The beneficiary snapshots the selector is declared independent of.
    pub independent_of: Vec<String>,
}

impl SelectorDeclaration {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("selector_ref", Json::str(&self.selector_ref)),
            ("kind", Json::str(&self.kind)),
            ("calibration_ref", Json::str(&self.calibration_ref)),
            (
                "independent_of",
                Json::Arr(self.independent_of.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SelectorDeclaration, SchemaError> {
        let m = expect_obj(j, "SelectorDeclaration")?;
        reject_unknown(
            m,
            &["selector_ref", "kind", "calibration_ref", "independent_of"],
            "SelectorDeclaration",
        )?;
        Ok(SelectorDeclaration {
            selector_ref: str_at(m, "selector_ref", "SelectorDeclaration")?.to_string(),
            kind: str_at(m, "kind", "SelectorDeclaration")?.to_string(),
            calibration_ref: str_at(m, "calibration_ref", "SelectorDeclaration")?.to_string(),
            independent_of: str_vec_at(m, "independent_of", "SelectorDeclaration")?,
        })
    }
}

/// `JudgePolicy` — the campaign's G7 table: the admitted judge selectors
/// plus the fixed audit budget (`charged_to = instrument`; the audited
/// share is reported) and the honeypot floor every judge-selector
/// counterexample set must meet.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgePolicy {
    /// The admitted judge selectors.
    pub selectors: Vec<SelectorDeclaration>,
    /// The audit budget's ref (a `ResourceAccount`/`SearchBudgetRecord`
    /// slice the audit draws on — `charged_to = instrument`).
    pub audit_budget_ref: String,
    /// The audited share of judge-selected rows (ppm).
    pub audited_share_ppm: u64,
    /// The minimum honeypot count a judge-selector counterexample set
    /// must carry.
    pub min_honeypots: u32,
}

impl JudgePolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "selectors",
                Json::Arr(self.selectors.iter().map(|s| s.to_json()).collect()),
            ),
            ("audit_budget_ref", Json::str(&self.audit_budget_ref)),
            (
                "audited_share_ppm",
                Json::Int(self.audited_share_ppm as i64),
            ),
            ("min_honeypots", Json::Int(self.min_honeypots as i64)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<JudgePolicy, SchemaError> {
        let m = expect_obj(j, "JudgePolicy")?;
        reject_unknown(
            m,
            &[
                "selectors",
                "audit_budget_ref",
                "audited_share_ppm",
                "min_honeypots",
            ],
            "JudgePolicy",
        )?;
        Ok(JudgePolicy {
            selectors: match m.get("selectors") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(SelectorDeclaration::from_json)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => Vec::new(),
            },
            audit_budget_ref: str_at(m, "audit_budget_ref", "JudgePolicy")?.to_string(),
            audited_share_ppm: opt_int_at(m, "audited_share_ppm")?.unwrap_or(0) as u64,
            min_honeypots: opt_int_at(m, "min_honeypots")?.unwrap_or(0) as u32,
        })
    }
}

// ── S6.3a — hosted coordinate admission (ADR-0196 D7; hh-hosting/1) ─────────

/// `HostedCoordinateDescriptor` — the records-in admission surface for a
/// hosted participant's `SUPPORTED` coordinate (the C4→C2 edge,
/// ADR-0196 D7). The campaign deposits one descriptor per hosted
/// participant under `doc_kind::HOSTED_DESCRIPTOR` before open; the open
/// gate resolves `hosted_descriptor_refs`, checks `participant_class =
/// hosted` + `abi = hh-hosting/1`, and admits exactly the coordinates the
/// descriptor's `capability_vector` marks `supported` — anything else is
/// `HostedCoordinateUnsupported` (unknown coordinate), and a coordinate
/// in `budget_enforcement.reported_only` is refused admission for
/// matched-total claims (`IncommensurableMatch`, AC-15).
#[derive(Debug, Clone, PartialEq)]
pub struct HostedCoordinateDescriptor {
    /// The participant ref the coordinates bind to (`participant:<id>`).
    pub participant_ref: String,
    /// `hosted` only — the participant class the `hh-hosting/1` ABI
    /// serves (ADR-0196 D7).
    pub participant_class: String,
    /// The hosted participant ABI spelling (`hh-hosting/1`).
    pub abi: String,
    /// `coordinate_* → capability` — only `supported` admits.
    pub capability_vector: BTreeMap<String, String>,
    /// The budget-enforcement declaration — `{enforced: [...],
    /// reported_only: [...]}` (a `reported_only` member refuses
    /// `matched_total` admission, AC-15).
    pub budget_enforcement: BTreeMap<String, Vec<String>>,
}

/// The hosted-participant ABI the coordinate descriptors declare
/// (ADR-0196 D7).
pub const HOSTING_ABI: &str = "hh-hosting/1";

/// The `capability_vector` value that admits a coordinate.
pub const CAPABILITY_SUPPORTED: &str = "supported";

impl HostedCoordinateDescriptor {
    /// The capability key a named coordinate resolves under
    /// (`model` → `coordinate_model`; `foo` → `coordinate_foo`) — the
    /// `hh-hosting/1` `set_coordinate` spelling.
    pub fn capability_key(coordinate: &str) -> String {
        format!("coordinate_{coordinate}")
    }

    /// `coordinate ∈ capability_vector` with `supported` capability.
    pub fn admits(&self, coordinate: &str) -> bool {
        self.capability_vector
            .get(&Self::capability_key(coordinate))
            .map(|c| c == CAPABILITY_SUPPORTED)
            .unwrap_or(false)
    }

    /// The coordinate's enforcement tier — `enforced`, `reported_only`,
    /// or absent (undecidable — refuse matched-total claims on it).
    pub fn enforcement(&self, coordinate: &str) -> &'static str {
        if self
            .budget_enforcement
            .get("enforced")
            .map(|e| e.iter().any(|c| c == coordinate))
            .unwrap_or(false)
        {
            "enforced"
        } else if self
            .budget_enforcement
            .get("reported_only")
            .map(|e| e.iter().any(|c| c == coordinate))
            .unwrap_or(false)
        {
            "reported_only"
        } else {
            "undeclared"
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("participant_ref", Json::str(&self.participant_ref)),
            ("participant_class", Json::str(&self.participant_class)),
            ("abi", Json::str(&self.abi)),
            (
                "capability_vector",
                Json::Obj(
                    self.capability_vector
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            ),
            (
                "budget_enforcement",
                Json::Obj(
                    self.budget_enforcement
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Arr(v.iter().map(Json::str).collect())))
                        .collect(),
                ),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<HostedCoordinateDescriptor, SchemaError> {
        let m = expect_obj(j, "HostedCoordinateDescriptor")?;
        reject_unknown(
            m,
            &[
                "participant_ref",
                "participant_class",
                "abi",
                "capability_vector",
                "budget_enforcement",
            ],
            "HostedCoordinateDescriptor",
        )?;
        let mut capability_vector = BTreeMap::new();
        match member_at(m, "capability_vector", "HostedCoordinateDescriptor")? {
            Json::Obj(cm) => {
                for (k, v) in cm {
                    capability_vector.insert(
                        k.clone(),
                        v.as_str()
                            .ok_or_else(|| {
                                SchemaError::v("capability_vector", "member must be a string")
                            })?
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err(SchemaError::v(
                    "capability_vector",
                    "must be a map<coordinate_*, capability>",
                ))
            }
        }
        let mut budget_enforcement = BTreeMap::new();
        match m.get("budget_enforcement") {
            Some(Json::Obj(bm)) => {
                for (k, v) in bm {
                    let mut dims = Vec::new();
                    match v {
                        Json::Arr(a) => {
                            for d in a {
                                dims.push(
                                    d.as_str()
                                        .ok_or_else(|| {
                                            SchemaError::v(
                                                "budget_enforcement",
                                                "member must be a string",
                                            )
                                        })?
                                        .to_string(),
                                );
                            }
                        }
                        _ => {
                            return Err(SchemaError::v(
                                "budget_enforcement",
                                "member must be an array of dimension names",
                            ))
                        }
                    }
                    budget_enforcement.insert(k.clone(), dims);
                }
            }
            Some(Json::Null) | None => {}
            _ => {
                return Err(SchemaError::v(
                    "budget_enforcement",
                    "must be a map<list, dimensions>",
                ))
            }
        }
        Ok(HostedCoordinateDescriptor {
            participant_ref: str_at(m, "participant_ref", "HostedCoordinateDescriptor")?
                .to_string(),
            participant_class: str_at(m, "participant_class", "HostedCoordinateDescriptor")?
                .to_string(),
            abi: str_at(m, "abi", "HostedCoordinateDescriptor")?.to_string(),
            capability_vector,
            budget_enforcement,
        })
    }
}
