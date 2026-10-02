//! `EvolutionProposer` — the §05h §2.4 proposer contract (R-2.9.5 6b;
//! ADR-0196) — `propose` / `select_parent` / `declare` — plus
//! `AheProposer`, the AHE-shaped (observability-driven) first-party
//! variant restricted to one target class per campaign.
//!
//! Records-in/records-out: the campaign assembles the [`EvidenceCorpus`]
//! view and [`ProposalConstraints`]; the proposer reads *only* those
//! (no ledger, no fs, no network — the first-party variant is CPU-only
//! and holds no handles), emits the typed [`ProposerOutcome`] sum, and
//! every emitted `CandidateProposal` still passes through the campaign's
//! S1 gates (`propose`) unchanged — the proposer is a *source* of
//! candidates, never a gate bypass (G1/G2/X6).

use std::collections::BTreeMap;

use hh_hir::diff::{self, DiffDerivation};
use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
use hh_hir::leaves::Text;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::records::{
    CandidateProposal, ConditionedRuleDecl, FailureHypothesis, ParentSelectionPolicy,
    PredictedDelta, PredictedEffect, ProposerDeclaration, ProposerFailure, ProposerOutcome, Tri,
};

/// One evidence row the corpus exposes — a per-task/per-arm outcome
/// observation on a `search`/`dev` split (held-out rows never enter a
/// corpus; S0 refused them).
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusRow {
    /// The task.
    pub task_id: String,
    /// The arm/level the row measures (`"base"` = the frozen head).
    pub arm: String,
    /// The metric (`corpus.metric_refs` member).
    pub metric: String,
    /// The observed value (ppm where rates; raw integers elsewhere).
    pub value_ppm: u64,
    /// Replicates observed.
    pub replicates: u32,
}

/// One memory-lineage entry the corpus exposes (R-2.4.4⁴ — evolution
/// candidates read memory lineage: the `derived_from`/`supersedes`
/// chains of `Memory` versions).
#[derive(Debug, Clone, PartialEq)]
pub struct LineageLink {
    /// The memory version ref.
    pub version_ref: String,
    /// The version it derives from (`None` = root).
    pub derived_from: Option<String>,
    /// The version it supersedes, when declared.
    pub supersedes: Option<String>,
}

/// `EvidenceCorpus` — the propose input's realised view (§05h §2.2;
/// `layers` subset already S0-checked by the campaign).
#[derive(Debug, Clone, Default)]
pub struct EvidenceCorpus {
    /// The declared layers (`corpus.layers` — the closed set).
    pub layers: Vec<String>,
    /// The outcome rows (search/dev only).
    pub outcome_rows: Vec<CorpusRow>,
    /// `slot_history` — per-slot `search_time_benefit` refs (ADR-0190's
    /// scheduler seam; the corpus's own layer).
    pub slot_history: BTreeMap<String, String>,
    /// `memory_lineage` — memory version lineage links.
    pub memory_lineage: Vec<LineageLink>,
    /// `reference_trajectories` — the trajectory refs the corpus anchors
    /// (S6.3a — the RHO family's required input; the
    /// `reference_trajectories` corpus layer admits them).
    pub reference_trajectories: Vec<String>,
    /// `prior_candidates` — the lineage's earlier candidate refs.
    pub prior_candidates: Vec<String>,
    /// The exclusion set (fixed — G5/G6).
    pub exclusions: Vec<String>,
    /// The task universe.
    pub task_ids: Vec<String>,
    /// The metric vocabulary.
    pub metric_refs: Vec<String>,
}

/// `ProposalConstraints` — the campaign's proposal envelope (§05h §4 S1
/// input; §2.5 slot allocation): the one target class, the fixed
/// exclusion set, the semantic-op bound, the admitted kinds, the
/// proposer's budget slice.
#[derive(Debug, Clone)]
pub struct ProposalConstraints {
    /// The campaign's base definition ref spelling — the emitted
    /// `CandidateProposal.base_ref` (the campaign compares against its
    /// head; the proposer never invents the ref).
    pub base_ref: String,
    /// `target_classes[]` — exactly one at 6b.
    pub target_classes: Vec<String>,
    /// Fixed target exclusions (G5).
    pub exclusions: Vec<String>,
    /// MUST-code targets (leaf rewrites refused).
    pub must_code: Vec<String>,
    /// `semantic_ops` bound.
    pub max_semantic_ops: u64,
    /// The admitted entity kinds (`allowed_target_kinds`).
    pub allowed_kinds: Vec<String>,
    /// The proposer's budget slice (`budget: slice` — ppm of the
    /// campaign's search budget).
    pub budget_slice_ppm: u64,
}

/// A lineage entry `select_parent` scores — folded campaign data, never
/// a live read.
#[derive(Debug, Clone)]
pub struct LineageEntry {
    /// The lineage member's candidate id.
    pub candidate_id: String,
    /// Its applied target (`DefinitionVersionRef`).
    pub target_ref: DefinitionVersionRef,
    /// The folded score (ppm — a `search_time_benefit`/acceptance score).
    pub score_ppm: u64,
    /// Realised descendants (children count).
    pub children: u32,
    /// Per-task scores for `pareto_per_task` (`task_id → ppm`).
    pub per_task: BTreeMap<String, u64>,
    /// The niche the member occupies in the gene-bank archive (S6.3a —
    /// `gene_bank{niche}` selects within it; `None` = un-niched).
    pub niche: Option<String>,
    /// The reference-trajectory evidence refs the member anchors to
    /// (S6.3a — `rho{trajectory_ref}` selects among members citing it).
    pub trajectory_refs: Vec<String>,
}

/// The shared deterministic argmax behind `select_parent` — the policy
/// weights are the campaign's; every first-party variant resolves
/// through this one function so policy semantics never fork (CC1).
/// `gene_bank{niche}` picks the niche's elite (falling back to the
/// lineage-wide elite when the niche has no member); `rho{trajectory_ref}`
/// picks the highest-scoring member anchored to the named reference
/// trajectory (`None` when no member cites it — an anchorless pick is
/// never invented).
pub fn select_parent_apply(
    lineage: &[LineageEntry],
    policy: &ParentSelectionPolicy,
) -> Option<DefinitionVersionRef> {
    if lineage.is_empty() {
        return None;
    }
    // The candidate pool the arm selects over.
    let pool: Vec<&LineageEntry> = match policy {
        ParentSelectionPolicy::GeneBank { niche } => {
            let niched: Vec<&LineageEntry> = lineage
                .iter()
                .filter(|e| e.niche.as_deref() == Some(niche.as_str()))
                .collect();
            if niched.is_empty() {
                lineage.iter().collect()
            } else {
                niched
            }
        }
        ParentSelectionPolicy::Rho { trajectory_ref } => lineage
            .iter()
            .filter(|e| e.trajectory_refs.iter().any(|t| t == trajectory_ref))
            .collect(),
        _ => lineage.iter().collect(),
    };
    if pool.is_empty() {
        return None;
    }
    let key = |e: &LineageEntry| -> u64 {
        match policy {
            ParentSelectionPolicy::Best => e.score_ppm,
            ParentSelectionPolicy::ScoreProportional {
                alpha_ppm,
                uniform_mix_ppm,
            } => e
                .score_ppm
                .saturating_mul(*alpha_ppm)
                .saturating_add(*uniform_mix_ppm),
            ParentSelectionPolicy::ScoreChildProportional {
                alpha_ppm,
                children_penalty_ppm,
            } => e
                .score_ppm
                .saturating_mul(*alpha_ppm)
                .saturating_sub(children_penalty_ppm.saturating_mul(e.children as u64)),
            ParentSelectionPolicy::ParetoPerTask => {
                e.per_task.values().min().copied().unwrap_or(e.score_ppm)
            }
            // The niche/trajectory filter already restricted the pool —
            // the elite pick inside it is score-argmax.
            ParentSelectionPolicy::GeneBank { .. } | ParentSelectionPolicy::Rho { .. } => {
                e.score_ppm
            }
        }
    };
    pool.iter()
        .max_by(|a, b| {
            key(a)
                .cmp(&key(b))
                .then(b.candidate_id.cmp(&a.candidate_id))
        })
        .map(|e| e.target_ref.clone())
}

/// The `evolution_proposer` contract (§05h §2.4).
pub trait EvolutionProposer {
    /// `declare()` — the tri-state declaration the static kind checks.
    fn declare(&self) -> ProposerDeclaration;
    /// `propose(corpus, base, constraints)` — reads only corpus + base;
    /// emits the typed outcome sum (never an empty list —
    /// `NoAddressableFailure` is the typed arm).
    fn propose(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure>;
    /// `select_parent(lineage, policy)` — pure; returns the chosen
    /// parent's `DefinitionVersionRef` (`None` = empty lineage).
    fn select_parent(
        &self,
        lineage: &[LineageEntry],
        policy: &ParentSelectionPolicy,
    ) -> Option<DefinitionVersionRef>;
}

/// Whether a node sits inside the admitted surface: the node kind is
/// admitted (`allowed_kinds`) and the semantic id is not excluded /
/// MUST-code. Shared by every leaf-picking variant (AHE, GeneBank,
/// RHO, code_search).
fn admissible_node(n: &Node, constraints: &ProposalConstraints) -> bool {
    let id = n.semantic_id();
    if constraints.exclusions.iter().any(|e| e == &id)
        || constraints.must_code.iter().any(|e| e == &id)
    {
        return false;
    }
    constraints
        .allowed_kinds
        .iter()
        .any(|k| k.as_str() == n.kind.name())
}

/// The corpus's addressable-failure table — `task → (metric, base_min,
/// sibling_max)` over `outcome_rows` (`replicates = 0` rows carry no
/// signal). Shared by every observability-shaped first-party variant.
fn addressable(corpus: &EvidenceCorpus) -> BTreeMap<String, (String, u64, u64)> {
    let mut failing: BTreeMap<String, (String, u64, u64)> = BTreeMap::new();
    for row in &corpus.outcome_rows {
        if row.replicates == 0 {
            continue;
        }
        if row.arm == "base" {
            let e = failing
                .entry(row.task_id.clone())
                .or_insert((row.metric.clone(), u64::MAX, 0));
            e.1 = e.1.min(row.value_ppm);
        } else {
            let e = failing
                .entry(row.task_id.clone())
                .or_insert((row.metric.clone(), u64::MAX, 0));
            e.2 = e.2.max(row.value_ppm);
        }
    }
    failing
        .into_iter()
        .filter(|(_, (_, base_v, best_v))| *best_v > *base_v)
        .collect()
}

// ── AheProposer — the AHE-shaped first-party variant ─────────────────────────

/// `AheProposer` — the observability-driven (AHE-shaped) first-party
/// `evolution_proposer` variant: `family = ahe`, `uses_judge = no`,
/// deterministic, CPU-only, and restricted to **one** target class per
/// campaign (6b).
///
/// The heuristic: read the corpus's `outcome_rows` for tasks where the
/// base arm underperforms a sibling arm (the observed failure), bind a
/// falsifiable `FailureHypothesis` to the metric the rows name, and emit
/// a `CandidateProposal` whose diff tightens the *weakest* admitted leaf
/// on the target-class node — a `ReplaceLeaf` over a numeric bound
/// (tighten by 10%) or a guideline `Text` rewrite naming the observed
/// failure class. Tightening only: the emitted diff's classification
/// never widens authority, loosens budget, or loosens validity — and the
/// campaign's S1 gates re-check every emitted candidate anyway (the
/// proposer is a source, never a bypass).
///
/// When the corpus shows no addressable failure (every base row at
/// ceiling, or no rows at all), `propose` returns the typed
/// `NoAddressableFailure{reason}` outcome — never an empty list.
#[derive(Debug)]
pub struct AheProposer {
    /// The one component class this variant instance targets.
    target_class: String,
    /// The variant's conditioned proposer rules (debt-managed — the
    /// static conformance kind sweeps them).
    conditioned_rules: Vec<ConditionedRuleDecl>,
    /// The proposer's observed judge-call count (a `uses_judge = no`
    /// variant is always zero — the port exposes it for the DRIFT
    /// property).
    judge_calls: u64,
    /// Spend drawn against the budget slice this campaign (model calls —
    /// zero for the deterministic first-party variant; the member is the
    /// accounting surface the port reads).
    model_calls_spent: u64,
    /// `budget_slice_ppm` exhausted → `BudgetExhausted` (never silent).
    spent_ppm: u64,
}

impl AheProposer {
    /// The AHE-shaped variant instance for `target_class` — the one
    /// component class restriction is construction-time, so a variant
    /// *cannot* emit a candidate outside it.
    pub fn new(target_class: impl Into<String>) -> AheProposer {
        let target_class = target_class.into();
        AheProposer {
            conditioned_rules: vec![ConditionedRuleDecl {
                rule_ref: format!("hh/evolution_proposer/ahe:{target_class}"),
                conditioned_on: "model_profile".to_string(),
                // The conditioned selection heuristic carries its
                // AssumptionDebtRecord — the static kind's completeness
                // surface (a `None` here is `IncompleteDeclaration`).
                debt_record: Some(Json::obj([
                    ("kind", Json::str("assumption_debt")),
                    (
                        "rule_ref",
                        Json::str(format!("hh/evolution_proposer/ahe:{target_class}")),
                    ),
                    (
                        "deficiency",
                        Json::str("weakest-cell tightening may not transfer"),
                    ),
                    (
                        "expiry_condition",
                        Json::Arr(vec![Json::str("model_version_change")]),
                    ),
                    ("owner", Json::str("hh/evolution")),
                ])),
            }],
            target_class,
            judge_calls: 0,
            model_calls_spent: 0,
            spent_ppm: 0,
        }
    }

    /// The judge-call count the conformance port reads (DRIFT probe —
    /// `uses_judge = no` ⇒ always zero on this variant).
    pub fn judge_calls(&self) -> u64 {
        self.judge_calls
    }

    /// Model calls spent on the proposer's slice (G8's accounting
    /// surface — zero for the deterministic variant).
    pub fn model_calls_spent(&self) -> u64 {
        self.model_calls_spent
    }

    /// Whether a node sits inside the target-class surface: the node
    /// kind is admitted (`allowed_kinds`) and the semantic id is not
    /// excluded / MUST-code.
    fn admissible_node(&self, n: &Node, constraints: &ProposalConstraints) -> bool {
        admissible_node(n, constraints)
    }

    /// The tighten-able member surface: numeric bounds (`*_ppm`,
    /// `*_cap`, `*_max`, `threshold`, `target_fraction`, `window`) and
    /// guideline `Text` members (`guideline`, `prompt`, `*_template`) —
    /// found by a deterministic depth-first walk over the semantic
    /// record's canonical JSON (object keys are already sorted).
    fn find_member(j: &Json, path: &str, depth: u8) -> Option<String> {
        const NUMERIC_SUFFIXES: &[&str] = &["_ppm", "_cap", "_max"];
        const NUMERIC_NAMES: &[&str] = &[
            "threshold",
            "target_fraction",
            "agree_window",
            "k_max",
            "budget",
        ];
        const TEXT_NAMES: &[&str] = &["guideline", "prompt", "rationale_template"];
        if depth > 4 {
            return None;
        }
        let Json::Obj(m) = j else { return None };
        // Numeric members first — a tightened bound is the AHE heuristic's
        // primary move; text members second (guideline optimisation).
        for (k, v) in m {
            if let Json::Int(_) = v {
                let leaf = k.rsplit('.').next().unwrap_or(k);
                if NUMERIC_SUFFIXES.iter().any(|s| k.ends_with(s)) || NUMERIC_NAMES.contains(&leaf)
                {
                    return Some(if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    });
                }
            }
        }
        for (k, v) in m {
            if let Json::Str(_) = v {
                let leaf = k.rsplit('.').next().unwrap_or(k);
                if TEXT_NAMES.contains(&leaf) {
                    return Some(if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    });
                }
            }
        }
        for (k, v) in m {
            if let Json::Obj(_) = v {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if let Some(p) = Self::find_member(v, &sub, depth + 1) {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Pick the diff's target leaf: the first admissible node in
    /// canonical order carrying a tighten-able member — returns the node
    /// semantic id + the member's dotted path inside `semantic`.
    fn pick_leaf(
        &self,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Option<(String, String)> {
        for n in &base.nodes {
            if !self.admissible_node(n, constraints) {
                continue;
            }
            let rec = hh_hir::wire::semantic_record_json(&n.semantic, true);
            if let Some(path) = Self::find_member(&rec, "", 0) {
                return Some((n.semantic_id(), path));
            }
        }
        None
    }

    /// Set the member at `path` inside a record JSON (dotted path into
    /// `semantic`) — numeric members tighten (×0.9, floor 1), text
    /// members take the deterministic rewrite.
    fn set_member(j: &mut Json, path: &[&str], task: &str) {
        let (head, tail) = path.split_first().expect("non-empty path");
        let Json::Obj(m) = j else { return };
        let Some(v) = m.get_mut(*head) else { return };
        if tail.is_empty() {
            match v {
                Json::Int(i) => {
                    *i = (*i as u64).saturating_sub((*i as u64) / 10).max(1) as i64;
                }
                Json::Str(s) => {
                    *s = format!("{s}\n# evolved: address observed failure on {task}");
                }
                _ => {}
            }
            return;
        }
        Self::set_member(v, tail, task);
    }

    /// The tightening emit shared by every observability-shaped
    /// first-party variant (S6.3a — GeneBank picks a niche slot; RHO
    /// anchors the hypothesis to a `reference_trajectories` ref).
    /// `propose` still owns the family constraint checks; this body is
    /// the corpus-read + leaf-tighten + diff-emit machinery.
    fn emit(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
        slot: &str,
        trajectory_ref: Option<&str>,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure> {
        // The observability signal: a task where the base arm trails a
        // sibling arm on a corpus metric — the addressable failure.
        let mut failing: BTreeMap<String, (String, u64, u64)> = BTreeMap::new();
        for row in &corpus.outcome_rows {
            if row.replicates == 0 {
                continue;
            }
            if row.arm == "base" {
                let e =
                    failing
                        .entry(row.task_id.clone())
                        .or_insert((row.metric.clone(), u64::MAX, 0));
                e.1 = e.1.min(row.value_ppm);
            } else {
                let e =
                    failing
                        .entry(row.task_id.clone())
                        .or_insert((row.metric.clone(), u64::MAX, 0));
                e.2 = e.2.max(row.value_ppm);
            }
        }
        let addressable: Vec<String> = failing
            .iter()
            .filter(|(_, (_, base_v, best_v))| *best_v > *base_v)
            .map(|(t, _)| t.clone())
            .collect();
        if addressable.is_empty() {
            return Ok(vec![ProposerOutcome::NoAddressableFailure {
                reason: "no corpus row shows a sibling arm beating the base".to_string(),
            }]);
        }
        let Some((node_id, member)) = self.pick_leaf(base, constraints) else {
            return Err(ProposerFailure::ConstraintUnsatisfiable {
                detail: format!(
                    "no admissible leaf on target class `{}` under the exclusion set",
                    self.target_class
                ),
            });
        };
        let task = &addressable[0];
        let metric = failing[task].0.clone();
        let direction = if metric.contains("latency")
            || metric.contains("cost")
            || metric.contains("tokens")
            || metric.contains("overhead")
        {
            "decrease"
        } else {
            "increase"
        };

        // The hypothesis — falsifiable, corpus-cited, bounded to the
        // observed task (search/dev labels only; S2 re-checks).
        let hyp = FailureHypothesis {
            kind: "observational".to_string(),
            evidence_refs: match trajectory_ref {
                Some(t) => vec![t.to_string()],
                None => vec![format!("corpus:{node_id}")],
            },
            predicted: PredictedEffect {
                deltas: vec![PredictedDelta {
                    metric: metric.clone(),
                    direction: direction.to_string(),
                }],
                affected_task_ids: vec![task.clone()],
                model_scope: "same_snapshot".to_string(),
                horizon: None,
            },
            semantic_op_targets: vec![node_id.clone()],
            reference_trajectories: trajectory_ref
                .map(|t| vec![t.to_string()])
                .unwrap_or_default(),
        };

        // The diff — a `ReplaceLeaf` tightening the picked member
        // (numeric: −10% with a floor of 1; text: a deterministic
        // guideline rewrite naming the observed failure). Built by
        // editing a clone of `base` then `diff::diff(base, target)` so
        // the classification and invert contract come from the one
        // place (CC1/CC7).
        let mut target = base.clone();
        let tnode = target
            .nodes
            .iter_mut()
            .find(|n| n.semantic_id() == node_id)
            .expect("node came from base");
        let mut rec = hh_hir::wire::semantic_record_json(&tnode.semantic, true);
        Self::set_member(&mut rec, &member.split('.').collect::<Vec<_>>(), task);
        // Rewrite the node's record through the kinds' own codec — a
        // member that doesn't decode is a proposer defect surfaced as a
        // typed failure, never a silent drop.
        tnode.semantic = hh_hir::wire::semantic_record_from_json(tnode.kind, &rec, "ahe-proposer")
            .map_err(|e| ProposerFailure::ConstraintUnsatisfiable {
                detail: format!("edited record fails its own codec: {e:?}"),
            })?;
        let provenance = ProvenanceRecord::minted(
            hh_provenance::Origin::evolution(&node_id, &node_id),
            hh_provenance::PersistenceScope::Run,
            0,
        );
        let d = diff::diff(
            base,
            &target,
            provenance.clone(),
            DiffDerivation {
                hypothesis: Some(Text::new(
                    hyp.to_json().to_canonical_string(),
                    "hh/evolution",
                    provenance,
                )),
                trajectories: vec![],
                candidate_id: None,
            },
        )
        .map_err(|errs| ProposerFailure::ConstraintUnsatisfiable {
            detail: format!("emitted diff fails its own gates: {errs:?}"),
        })?;
        let proposal = CandidateProposal {
            base_ref: constraints.base_ref.clone(),
            diff: d,
            slot: slot.to_string(),
            hypothesis: Some(hyp.clone()),
            install: None,
            coordinate_values: BTreeMap::new(),
        };
        Ok(vec![
            ProposerOutcome::Candidate(Box::new(proposal)),
            ProposerOutcome::Hypothesis(hyp),
        ])
    }
}

impl EvolutionProposer for AheProposer {
    fn declare(&self) -> ProposerDeclaration {
        ProposerDeclaration {
            family: "ahe".to_string(),
            op_classes_admissible: vec![self.target_class.clone()],
            needs_reference_trajectories: Tri::No,
            uses_judge: Tri::No,
            judge_ref: None,
            maturity: "instrument-grade".to_string(),
            conditioned_rules: self.conditioned_rules.clone(),
        }
    }

    fn propose(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure> {
        // Constraint checks first — the typed failures of §2.4.
        if corpus
            .layers
            .iter()
            .any(|l| crate::records::FORBIDDEN_LAYERS.iter().any(|f| f == l))
        {
            return Err(ProposerFailure::CorpusUnreadable {
                detail: "corpus declares a held-out surface layer".to_string(),
            });
        }
        if constraints.target_classes != vec![self.target_class.clone()] {
            return Err(ProposerFailure::ConstraintUnsatisfiable {
                detail: format!(
                    "AHE-shaped variant targets exactly `{}` — the one-class rule",
                    self.target_class
                ),
            });
        }
        if constraints.max_semantic_ops == 0 || constraints.allowed_kinds.is_empty() {
            return Err(ProposerFailure::ConstraintUnsatisfiable {
                detail: "no admitted semantic op on the target class".to_string(),
            });
        }
        if self.spent_ppm >= constraints.budget_slice_ppm && constraints.budget_slice_ppm > 0 {
            return Err(ProposerFailure::BudgetExhausted {
                detail: "the proposer's budget slice is exhausted".to_string(),
            });
        }
        self.spent_ppm = self.spent_ppm.saturating_add(1);

        self.emit(corpus, base, constraints, &self.target_class.clone(), None)
    }

    fn select_parent(
        &self,
        lineage: &[LineageEntry],
        policy: &ParentSelectionPolicy,
    ) -> Option<DefinitionVersionRef> {
        select_parent_apply(lineage, policy)
    }
}

// ── S6.3a research-grade families (R-2.9.5 6c) ───────────────────────────────

/// `CodeSearchProposer` — the `code_search` family variant (6c): it
/// searches `CompiledPayload` leaves on admissible `Procedure` nodes —
/// `ProcedureStep::Opaque` bodies carrying a `declared_interface` — and
/// emits candidates whose diff replaces the payload's `bytes_hash`
/// while holding the interface fixed (typed + out-of-process only; the
/// campaign's S1/S7 gates re-check `OpaqueWithoutInterface`/placement).
/// `maturity = research-grade` — results render `preview`.
#[derive(Debug)]
pub struct CodeSearchProposer {
    /// The component classes this variant may search (`code_payload`
    /// mandatory — the payload-leaf surface).
    classes: Vec<String>,
    /// Emitted candidate count (drives the deterministic bytes_hash).
    emitted: u64,
    /// `budget_slice_ppm` accounting (deterministic — model spend is 0).
    spent_ppm: u64,
}

impl CodeSearchProposer {
    /// The variant instance for the admitted classes.
    pub fn new(classes: Vec<String>) -> CodeSearchProposer {
        CodeSearchProposer {
            classes,
            emitted: 0,
            spent_ppm: 0,
        }
    }

    /// The first admissible `Procedure` node's top-level `Opaque` step
    /// carrying a declared interface — `(node_id, step_index)`.
    fn pick_payload(
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Option<(String, usize)> {
        for n in &base.nodes {
            if !admissible_node(n, constraints) {
                continue;
            }
            if let hh_hir::records::KindRecord::Procedure(rec) = &n.semantic {
                for (i, step) in rec.steps.iter().enumerate() {
                    if let hh_hir::records::ProcedureStep::Opaque(p) = step {
                        if p.declared_interface.is_some() {
                            return Some((n.semantic_id(), i));
                        }
                    }
                }
            }
        }
        None
    }
}

impl EvolutionProposer for CodeSearchProposer {
    fn declare(&self) -> ProposerDeclaration {
        ProposerDeclaration {
            family: "code_search".to_string(),
            op_classes_admissible: self.classes.clone(),
            needs_reference_trajectories: Tri::No,
            uses_judge: Tri::No,
            judge_ref: None,
            maturity: "research-grade".to_string(),
            conditioned_rules: vec![ConditionedRuleDecl {
                rule_ref: "hh/evolution_proposer/code_search".to_string(),
                conditioned_on: "model_profile".to_string(),
                debt_record: Some(Json::obj([
                    ("kind", Json::str("assumption_debt")),
                    ("rule_ref", Json::str("hh/evolution_proposer/code_search")),
                    (
                        "deficiency",
                        Json::str("same-interface payload search may not generalise"),
                    ),
                    (
                        "expiry_condition",
                        Json::Arr(vec![Json::str("model_version_change")]),
                    ),
                    ("owner", Json::str("hh/evolution")),
                ])),
            }],
        }
    }

    fn propose(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure> {
        if corpus
            .layers
            .iter()
            .any(|l| crate::records::FORBIDDEN_LAYERS.iter().any(|f| f == l))
        {
            return Err(ProposerFailure::CorpusUnreadable {
                detail: "corpus declares a held-out surface layer".to_string(),
            });
        }
        for c in &constraints.target_classes {
            if !self.classes.iter().any(|k| k == c) {
                return Err(ProposerFailure::ConstraintUnsatisfiable {
                    detail: format!(
                        "code_search variant admits {classes:?} — `{c}` is outside",
                        classes = self.classes
                    ),
                });
            }
        }
        if !constraints
            .target_classes
            .iter()
            .any(|c| c == "code_payload")
        {
            return Err(ProposerFailure::ConstraintUnsatisfiable {
                detail: "code_search searches `CompiledPayload` leaves — \
                         `code_payload` must be an admitted target class"
                    .to_string(),
            });
        }
        if self.spent_ppm >= constraints.budget_slice_ppm && constraints.budget_slice_ppm > 0 {
            return Err(ProposerFailure::BudgetExhausted {
                detail: "the proposer's budget slice is exhausted".to_string(),
            });
        }
        self.spent_ppm = self.spent_ppm.saturating_add(1);
        let failing = addressable(corpus);
        if failing.is_empty() {
            return Ok(vec![ProposerOutcome::NoAddressableFailure {
                reason: "no corpus row shows a sibling arm beating the base".to_string(),
            }]);
        }
        let Some((node_id, step_idx)) = Self::pick_payload(base, constraints) else {
            return Ok(vec![ProposerOutcome::NoAddressableFailure {
                reason: "no admissible `CompiledPayload` leaf with a declared \
                         interface on a Procedure node"
                    .to_string(),
            }]);
        };
        let task = failing.keys().next().expect("non-empty").clone();
        let metric = failing[&task].0.clone();

        // The move: rewrite the opaque step's payload — a fresh
        // `bytes_hash` under the *same* declared interface (the
        // interface is the contract the leaf satisfies; the campaign's
        // S1 gate re-checks `OpaqueWithoutInterface` on the assembled
        // target).
        let mut target = base.clone();
        let tnode = target
            .nodes
            .iter_mut()
            .find(|n| n.semantic_id() == node_id)
            .expect("node came from base");
        let hh_hir::records::KindRecord::Procedure(rec) = &mut tnode.semantic else {
            unreachable!("pick_payload only returns Procedure nodes")
        };
        let hh_hir::records::ProcedureStep::Opaque(old) = &rec.steps[step_idx] else {
            unreachable!("pick_payload returned this step index")
        };
        let provenance = ProvenanceRecord::minted(
            hh_provenance::Origin::evolution(&node_id, &node_id),
            hh_provenance::PersistenceScope::Run,
            0,
        );
        rec.steps[step_idx] =
            hh_hir::records::ProcedureStep::Opaque(hh_hir::leaves::CompiledPayload {
                format_tag: old.format_tag.clone(),
                bytes_hash: format!("{}#cs{}", old.bytes_hash, self.emitted),
                declared_interface: old.declared_interface.clone(),
                owner: old.owner.clone(),
                provenance: provenance.clone(),
            });
        self.emitted += 1;

        let hyp = FailureHypothesis {
            kind: "observational".to_string(),
            evidence_refs: vec![format!("corpus:{node_id}")],
            predicted: PredictedEffect {
                deltas: vec![PredictedDelta {
                    metric,
                    direction: "increase".to_string(),
                }],
                affected_task_ids: vec![task],
                model_scope: "same_snapshot".to_string(),
                horizon: None,
            },
            semantic_op_targets: vec![node_id.clone()],
            reference_trajectories: Vec::new(),
        };
        let d = diff::diff(
            base,
            &target,
            provenance.clone(),
            DiffDerivation {
                hypothesis: Some(Text::new(
                    hyp.to_json().to_canonical_string(),
                    "hh/evolution",
                    provenance,
                )),
                trajectories: vec![],
                candidate_id: None,
            },
        )
        .map_err(|errs| ProposerFailure::ConstraintUnsatisfiable {
            detail: format!("emitted diff fails its own gates: {errs:?}"),
        })?;
        let proposal = CandidateProposal {
            base_ref: constraints.base_ref.clone(),
            diff: d,
            slot: "code_payload".to_string(),
            hypothesis: Some(hyp.clone()),
            install: None,
            coordinate_values: BTreeMap::new(),
        };
        Ok(vec![
            ProposerOutcome::Candidate(Box::new(proposal)),
            ProposerOutcome::Hypothesis(hyp),
        ])
    }

    fn select_parent(
        &self,
        lineage: &[LineageEntry],
        policy: &ParentSelectionPolicy,
    ) -> Option<DefinitionVersionRef> {
        select_parent_apply(lineage, policy)
    }
}

/// `GeneBankProposer` — the `gene_bank` family variant (6c): a
/// quality-diversity archive over the declared niches (target classes).
/// Each `propose` addresses the *least-covered* niche — the archive's
/// slot counter per class — and runs the shared tightening emit. The
/// `gene_bank{niche}` parent-selection arm reads
/// `LineageEntry.niche` to pick niche elites. `research-grade`.
#[derive(Debug)]
pub struct GeneBankProposer {
    /// The niches this variant covers (the campaign's `target_classes`).
    classes: Vec<String>,
    /// `niche → emitted candidate count` — the archive's coverage table.
    archive: BTreeMap<String, u64>,
    /// `budget_slice_ppm` accounting.
    spent_ppm: u64,
}

impl GeneBankProposer {
    /// The variant instance covering `classes`.
    pub fn new(classes: Vec<String>) -> GeneBankProposer {
        GeneBankProposer {
            classes,
            archive: BTreeMap::new(),
            spent_ppm: 0,
        }
    }
}

impl EvolutionProposer for GeneBankProposer {
    fn declare(&self) -> ProposerDeclaration {
        ProposerDeclaration {
            family: "gene_bank".to_string(),
            op_classes_admissible: self.classes.clone(),
            needs_reference_trajectories: Tri::No,
            uses_judge: Tri::No,
            judge_ref: None,
            maturity: "research-grade".to_string(),
            conditioned_rules: vec![],
        }
    }

    fn propose(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure> {
        if corpus
            .layers
            .iter()
            .any(|l| crate::records::FORBIDDEN_LAYERS.iter().any(|f| f == l))
        {
            return Err(ProposerFailure::CorpusUnreadable {
                detail: "corpus declares a held-out surface layer".to_string(),
            });
        }
        for c in &constraints.target_classes {
            if !self.classes.iter().any(|k| k == c) {
                return Err(ProposerFailure::ConstraintUnsatisfiable {
                    detail: format!(
                        "gene_bank variant covers {classes:?} — `{c}` is outside",
                        classes = self.classes
                    ),
                });
            }
        }
        if self.spent_ppm >= constraints.budget_slice_ppm && constraints.budget_slice_ppm > 0 {
            return Err(ProposerFailure::BudgetExhausted {
                detail: "the proposer's budget slice is exhausted".to_string(),
            });
        }
        self.spent_ppm = self.spent_ppm.saturating_add(1);
        // The least-covered niche wins — deterministic argmin over the
        // archive's emission counts (uncovered niches first, canonical
        // order on ties).
        let niche = constraints
            .target_classes
            .iter()
            .min_by_key(|c| self.archive.get(*c).copied().unwrap_or(0))
            .cloned()
            .ok_or_else(|| ProposerFailure::ConstraintUnsatisfiable {
                detail: "no target class to archive into".to_string(),
            })?;
        *self.archive.entry(niche.clone()).or_insert(0) += 1;
        AheProposer::new(niche.clone()).emit(corpus, base, constraints, &niche, None)
    }

    fn select_parent(
        &self,
        lineage: &[LineageEntry],
        policy: &ParentSelectionPolicy,
    ) -> Option<DefinitionVersionRef> {
        select_parent_apply(lineage, policy)
    }
}

/// `RhoProposer` — the `rho` family variant (6c): retrospective,
/// trajectory-anchored search. `declare().needs_reference_trajectories
/// = yes` — the corpus's `reference_trajectories` layer is mandatory
/// (`CorpusUnreadable` when undeclared); the emitted hypothesis carries
/// `reference_trajectories` citing the anchor, and the
/// `rho{trajectory_ref}` parent-selection arm reads
/// `LineageEntry.trajectory_refs`. `research-grade`.
#[derive(Debug)]
pub struct RhoProposer {
    /// The component classes this variant may target.
    classes: Vec<String>,
    /// `budget_slice_ppm` accounting.
    spent_ppm: u64,
}

impl RhoProposer {
    /// The variant instance covering `classes`.
    pub fn new(classes: Vec<String>) -> RhoProposer {
        RhoProposer {
            classes,
            spent_ppm: 0,
        }
    }
}

impl EvolutionProposer for RhoProposer {
    fn declare(&self) -> ProposerDeclaration {
        ProposerDeclaration {
            family: "rho".to_string(),
            op_classes_admissible: self.classes.clone(),
            needs_reference_trajectories: Tri::Yes,
            uses_judge: Tri::No,
            judge_ref: None,
            maturity: "research-grade".to_string(),
            conditioned_rules: vec![],
        }
    }

    fn propose(
        &mut self,
        corpus: &EvidenceCorpus,
        base: &HirDocument,
        constraints: &ProposalConstraints,
    ) -> Result<Vec<ProposerOutcome>, ProposerFailure> {
        if corpus
            .layers
            .iter()
            .any(|l| crate::records::FORBIDDEN_LAYERS.iter().any(|f| f == l))
        {
            return Err(ProposerFailure::CorpusUnreadable {
                detail: "corpus declares a held-out surface layer".to_string(),
            });
        }
        // The retrospective family's mandatory input — the
        // `reference_trajectories` corpus layer and its anchored refs.
        if !corpus.layers.iter().any(|l| l == "reference_trajectories") {
            return Err(ProposerFailure::CorpusUnreadable {
                detail: "rho requires the corpus's `reference_trajectories` layer".to_string(),
            });
        }
        let Some(trajectory) = corpus.reference_trajectories.first().cloned() else {
            return Ok(vec![ProposerOutcome::NoAddressableFailure {
                reason: "the reference_trajectories layer anchors no trajectory".to_string(),
            }]);
        };
        for c in &constraints.target_classes {
            if !self.classes.iter().any(|k| k == c) {
                return Err(ProposerFailure::ConstraintUnsatisfiable {
                    detail: format!(
                        "rho variant covers {classes:?} — `{c}` is outside",
                        classes = self.classes
                    ),
                });
            }
        }
        if self.spent_ppm >= constraints.budget_slice_ppm && constraints.budget_slice_ppm > 0 {
            return Err(ProposerFailure::BudgetExhausted {
                detail: "the proposer's budget slice is exhausted".to_string(),
            });
        }
        self.spent_ppm = self.spent_ppm.saturating_add(1);
        let slot = constraints
            .target_classes
            .first()
            .cloned()
            .unwrap_or_else(|| "guideline".to_string());
        AheProposer::new(slot.clone()).emit(corpus, base, constraints, &slot, Some(&trajectory))
    }

    fn select_parent(
        &self,
        lineage: &[LineageEntry],
        policy: &ParentSelectionPolicy,
    ) -> Option<DefinitionVersionRef> {
        select_parent_apply(lineage, policy)
    }
}

/// The typed sum's JSON — every output parses as a
/// `CandidateProposal`/`FailureHypothesis`/`NoAddressableFailure`
/// (the contract kind's parse check target).
pub fn outcome_to_json(o: &ProposerOutcome) -> Json {
    match o {
        ProposerOutcome::Candidate(c) => {
            Json::obj([("kind", Json::str("candidate")), ("proposal", c.to_json())])
        }
        ProposerOutcome::Hypothesis(h) => Json::obj([
            ("kind", Json::str("hypothesis")),
            ("hypothesis", h.to_json()),
        ]),
        ProposerOutcome::NoAddressableFailure { reason } => Json::obj([
            ("kind", Json::str("no_addressable_failure")),
            ("reason", Json::str(reason)),
        ]),
    }
}

/// Strict decode of `outcome_to_json`'s documents — the contract kind
/// parses every emitted output through this (a doc that fails to parse
/// is the conformance failure).
pub fn outcome_from_json(j: &Json) -> Result<ProposerOutcome, hh_lab::json_util::SchemaError> {
    use hh_lab::json_util::{expect_obj, str_at, SchemaError};
    let m = expect_obj(j, "ProposerOutcome")?;
    match m.get("kind").and_then(Json::as_str) {
        Some("candidate") => {
            let p = m
                .get("proposal")
                .ok_or_else(|| SchemaError::v("ProposerOutcome.proposal", "missing"))?;
            Ok(ProposerOutcome::Candidate(Box::new(
                CandidateProposal::from_json(p)?,
            )))
        }
        Some("hypothesis") => {
            let h = m
                .get("hypothesis")
                .ok_or_else(|| SchemaError::v("ProposerOutcome.hypothesis", "missing"))?;
            Ok(ProposerOutcome::Hypothesis(FailureHypothesis::from_json(
                h,
            )?))
        }
        Some("no_addressable_failure") => Ok(ProposerOutcome::NoAddressableFailure {
            reason: str_at(m, "reason", "NoAddressableFailure")?.to_string(),
        }),
        other => Err(SchemaError::v(
            "ProposerOutcome.kind",
            format!("{other:?} — not in the typed sum"),
        )),
    }
}
