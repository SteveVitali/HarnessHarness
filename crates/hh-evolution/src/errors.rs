//! `EvolutionError` / `Refusal` — the campaign pipeline's closed error sum
//! (§05h §4's refusal catalogue; S6.1a; T-LCD-14: refusal, never a
//! warning). Every gate failure is one of these members — spelled exactly
//! as the spec names it (`SealRefused{not_human}`, `SelfModificationRefused`,
//! `BudgetSplittingTrap`, …) — so a rejected candidate's durable
//! `transitioned{to: rejected{stage, code, report_ref}}` row carries a code
//! a reader can trace to the spec clause.

use hh_ledger::errors::LedgerError;
use hh_wire::json::Json;

/// The closed stage-gate refusal sum. `code()` is the canonical snake_case
/// spelling carried on the durable `rejected{stage, code}` transition.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    // ── S0 observe / corpus admission ───────────────────────────────────
    /// The corpus spec names a `held_out`/`private` surface — S0 never
    /// sees it (§05h §4 S0; AC-R-2.9.5-2a).
    HeldOutLeak {
        /// The offending member detail.
        detail: String,
    },
    /// The corpus spec declares readers the corpus record does not carry
    /// (L1 evidence restriction — §05h §4 S0).
    ReaderViolation {
        /// The offending member detail.
        detail: String,
    },
    /// A validator/grader executable byte stream entered the corpus
    /// (`import`ed definition records carrying executable members —
    /// §05h §4 S0).
    ExecutableBytesInCorpus {
        /// The offending member detail.
        detail: String,
    },
    /// The pinned `SplitAssignmentRecord`'s `registered_at` does not
    /// precede the campaign's `opened` watermark (L3 — the split must
    /// predate every observation; §05h §4 S0; R-2.9.4⁴).
    EvidenceStale {
        /// The offending member detail.
        detail: String,
    },
    /// An `evidence_ref`/`suite_ref`/`metric` the campaign spec or a
    /// hypothesis names is not resolvable in the admitted corpus.
    UnresolvableEvidence {
        /// The unresolved ref.
        ref_: String,
    },

    // ── S1 propose / classification ─────────────────────────────────────
    /// `authority_delta` is `widening` — a candidate may never widen
    /// authority (§05h §4 S1; CC2).
    AuthorityWidening {
        /// The failure detail.
        detail: String,
    },
    /// `budget_delta` is `loosening` — candidates may tighten, never
    /// loosen (§05h §4 S1).
    BudgetLoosening {
        /// The failure detail.
        detail: String,
    },
    /// `validity_delta` is `loosening` (§05h §4 S1).
    ValidityLoosening {
        /// The failure detail.
        detail: String,
    },
    /// A semantic op targets a MUST-code leaf (an opaque `Text`/
    /// `CompiledPayload` the diff rewrites as code) (§05h §4 S1).
    MustCodeTarget {
        /// The offending op/node.
        detail: String,
    },
    /// A semantic op's target node is in the campaign's exclusion set or
    /// outside the admitted target classes (§05h §4 S1).
    ExcludedTarget {
        /// The excluded node/semantic id.
        node_id: String,
    },
    /// The proposal targets the evolution service's own definition — the
    /// service may never modify itself (§05h §4 S1; G3-1; AC-R-2.12.2-14).
    SelfModificationRefused {
        /// The failure detail.
        detail: String,
    },
    /// `classification.semantic_ops` exceeds the campaign's declared
    /// `semantic_ops_bound` (§05h §4 S1).
    TooManyOps {
        /// The observed count.
        seen: u64,
        /// The campaign's bound.
        bound: u64,
    },
    /// The candidate is already registered in this campaign's lineage —
    /// `candidate_id = H(base ∥ diff)` dedups by content (§05h §4 S1).
    DuplicateCandidate {
        /// The already-registered candidate id.
        candidate_id: String,
    },
    /// `apply`/`invert` round-trip failed — the diff is not a legal
    /// candidate (§05h §4 S1; G3's apply/invert contract).
    DiffNotInvertible {
        /// The failure detail.
        detail: String,
    },
    /// `provenance.validate` or the diff's derived-from record failed —
    /// a candidate's provenance is never guessed (CC3).
    InvalidProvenance {
        /// The failure detail.
        detail: String,
    },
    /// A semantic op touches a `coordinates.*` hosted surface the
    /// campaign did not admit — either the coordinate is not in
    /// `hosted_coordinates`, its descriptor does not carry
    /// `capability = supported`, or the edit is structural (ADR-0196
    /// D7 — coordinate admission is value-scoped, never structural).
    HostedCoordinateUnsupported {
        /// The coordinate or op path refused.
        coordinate: String,
        /// The failure detail.
        detail: String,
    },

    // ── S2 hypothesize ──────────────────────────────────────────────────
    /// The hypothesis is not falsifiable: empty `evidence_refs`, no
    /// predicted delta, or a failure kind the campaign does not admit
    /// (§05h §4 S2; AC-R-2.9.5-6).
    HypothesisUnfalsifiable {
        /// The reason.
        reason: String,
    },
    /// A predicted delta names a metric not registered in the corpus's
    /// metric vocabulary (§05h §4 S2).
    UnknownMetric {
        /// The metric id.
        metric: String,
    },
    /// `affected_task_ids` or a stage's `split_labels_used` touches a task
    /// whose split label is not `search`/`dev` admissible for the stage —
    /// or a held-out label leaked into the search side (§05h §4 S2/S3).
    LeakedSplit {
        /// The failure detail.
        detail: String,
    },
    /// A diff semantic op's target is not in the hypothesis's
    /// `semantic_op_targets` attribution set (§05h §4 S2).
    TargetMismatch {
        /// The offending op target.
        op: String,
    },

    // ── S3 screen ───────────────────────────────────────────────────────
    /// The observed counterexample flips in the predicted direction fall
    /// below the declared `min_flip_share` (§05h §4 S3; AC-R-2.9.5-7).
    PredictionFalsified {
        /// The observed share (ppm).
        share_ppm: u64,
        /// The declared floor (ppm).
        floor_ppm: u64,
    },
    /// `replicate_count` below the campaign floor (§05h §4 S3).
    InsufficientReplicates {
        /// The observed count.
        seen: u32,
        /// The floor.
        required: u32,
    },
    /// Judge-only acceptance is insufficient — S3 needs targeted
    /// counterexamples, not a judge scorecard (§05h §4 S3).
    JudgeOnlyAcceptance {
        /// The failure detail.
        detail: String,
    },

    // ── S4 search/evaluation (M3 matched_total — R-2.1.6⁴) ──────────────
    /// The evaluation spec's arms are not all `MatchSpec{matched_total}`
    /// — every evolution claim is gated on M3 (§05h §4 S4; AC-R-2.1.6-12).
    UnmatchedSearchBudget {
        /// The failure detail.
        detail: String,
    },
    /// The `ComparisonReport`'s `budget_match.status` is not `matched`.
    UnmatchedBudget {
        /// The failure detail.
        detail: String,
    },
    /// An arm carries no `search_budget` ref (§05h §4 S4).
    UnbudgetedArm {
        /// The offending arm.
        arm: String,
    },
    /// An arm's `search_budget` resolves to an incomplete
    /// `SearchBudgetRecord` (§05h §4 S4 — complete or refuse).
    SearchBudgetIncomplete {
        /// The failure detail.
        detail: String,
    },
    /// An arm's matched dimension is not `enforced` on the arm's
    /// `budget_enforcement` view — an unenforceable matched dimension
    /// refuses, never matches (ADR-0041 as amended; §05h §4 S4).
    IncommensurableMatch {
        /// The failure detail.
        detail: String,
    },
    /// A `hosted`/`reported`-only dimension under a `matched_total`
    /// claim — the ADR-0041-amended refusal (§05h §4 S4; AC-R-2.9.5-15;
    /// ADR-0196 D7).
    ReportedOnlyDimension {
        /// The failure detail.
        detail: String,
    },
    /// The named experiment is not registered in LabDocs — evidence the
    /// campaign did not derive is not evidence.
    SpecNotRegistered {
        /// The experiment id.
        experiment_id: String,
    },
    /// The named experiment's spec does not bind `evolution_campaign` to
    /// this campaign — a foreign experiment is not this pipeline's
    /// evidence (§05h §4; CC10).
    ForeignExperiment {
        /// The experiment id.
        experiment_id: String,
    },
    /// The named experiment is registered and campaign-bound but its
    /// `kind` is not one the stage admits — the eval stages take a
    /// comparison-bearing kind (`comparative`/`equivalence`), the
    /// removal-test stage a `retirement`/`retirement_batch` kind (§05h
    /// §4's stage tables).
    StageKindMismatch {
        /// The experiment id.
        experiment_id: String,
        /// The spec's declared kind.
        kind: String,
        /// The stage label the refusal attaches to (`S4`/`S5`/`S10`).
        stage: &'static str,
    },

    // ── S5 held-out / artifact benefit ──────────────────────────────────
    /// The `artifact_benefit` comparison's interval includes 0 — a
    /// candidate needs positive held-out benefit (§05h §4 S5; AC-R-2.9.5-8).
    NoArtifactBenefit {
        /// The failure detail.
        detail: String,
    },
    /// The `artifact_benefit` comparison did not run over a held-out
    /// split, or the spec's `validation_strategy` is not `full_set`
    /// (§05h §4 S5).
    NotHeldOut {
        /// The failure detail.
        detail: String,
    },
    /// Retention regressed on tasks beyond the declared margin
    /// (§05h §4 S5).
    RetentionRegressed {
        /// The regressed tasks.
        tasks: Vec<String>,
    },
    /// A declared veto metric regressed (§05h §4 S5 — vetoes are hard
    /// gates).
    VetoRegressed {
        /// The veto metric.
        metric: String,
    },
    /// The hypothesis is `observational` — such candidates may advance
    /// through S3–S4 only, never to artifact benefit / seal (§05h §4 S2).
    ObservationalOnly {
        /// The failure detail.
        detail: String,
    },

    // ── S6 transfer ─────────────────────────────────────────────────────
    /// No transfer row over a held-out environment family, or a row
    /// missing sign/interval (§05h §4 S6; AC-R-2.9.5-9).
    TransferUnreported {
        /// The failure detail.
        detail: String,
    },
    /// A `verified`/`drifted`/`broken` `CompatibilityRecord` carries no
    /// `evidence_ref` (§05h §4 S6 — a verdict without evidence is not a
    /// verdict).
    CompatibilityUnproven {
        /// The failure detail.
        detail: String,
    },

    // ── S7 security invariance ──────────────────────────────────────────
    /// A diff touching a policy leaf did not classify `narrowing`
    /// post-eval (§05h §4 S7; AC-R-2.9.5-10).
    PolicyWidening {
        /// The offending leaf.
        leaf: String,
    },
    /// A `CompiledPayload` variant lacks a declared interface (§05h §4 S7).
    OpaqueWithoutInterface {
        /// The failure detail.
        detail: String,
    },
    /// A candidate executable declares `in_process` placement — plugin
    /// variants run `subprocess_confined`, never in-process (§05h §4 S7;
    /// D-5/plugin_abi/1).
    InProcessCandidate {
        /// The failure detail.
        detail: String,
    },
    /// A dynamic security veto's post-eval observation regressed below
    /// the candidate's recorded baseline (§05h §4 S7).
    SecurityVetoTripped {
        /// The veto metric.
        metric: String,
    },

    // ── S8 seal / publish ───────────────────────────────────────────────
    /// The `EvolutionAcceptanceReport` is incomplete or a mandatory item
    /// is not `pass`/`n/a`-declared (§05h §4 S8; AC-R-2.9.5-1).
    AcceptanceIncomplete {
        /// The failing item ids.
        items: Vec<u8>,
    },
    /// The seal endorsement's provenance is not `origin = human` with
    /// `authority = definition` — the evolution process never endorses
    /// itself (§05h §4 S8; G3-1's literal spelling).
    SealRefused {
        /// `not_human` | `not_definition` | detail.
        reason: String,
    },
    /// The publish target namespace is not `exp/<campaign_id>` — nothing
    /// a candidate produces is published outside `exp/` (§05h §4 S8;
    /// AC-R-2.12.2-14; G3-2).
    NamespaceForbidden {
        /// The refused namespace.
        namespace: String,
    },

    // ── S9 canary / rollout ─────────────────────────────────────────────
    /// A canary `veto` tripped or the run aborted — the candidate reverts
    /// to its last human-sealed ancestor (§05h §4 S9).
    CanaryAborted {
        /// The abort reason.
        reason: String,
    },
    /// The canary was declared `split` — a split rollout is exploratory,
    /// never an `active` transition (§05h §4 S9).
    SplitRolloutNotCommittable {
        /// The failure detail.
        detail: String,
    },
    /// A conditioned rule the candidate touches carries no complete
    /// `AssumptionDebtRecord` at activation (§05h §4 S9; R-2.9.6⁰ᵃ
    /// seams — DF-S5.4-1's debt rows the S6.1b manager consumes).
    ConditionedRuleIncomplete {
        /// The failure detail.
        detail: String,
    },
    /// `coordination_delta = loosening` on the candidate diff — K-4
    /// classified and refused in evolution contexts like
    /// `authority_delta = widening` (R-2.6.5⁴; ADR-0193 (e);
    /// ADR-0053 D-5's shape). Checked at S1 and again at S7.
    CoordinationLoosening {
        /// The failure detail.
        detail: String,
    },
    /// A `selected_best_of_n` baseline or screen named a judge selector
    /// the campaign's `judge_policy` does not declare — calibrated,
    /// independent, honeypot-bearing selectors only (G7; §05h §4 S3–S4).
    JudgeSelectorUndeclared {
        /// The undeclared selector ref.
        selector: String,
        /// The failure detail.
        detail: String,
    },
    /// A monitor selector's `adversarial` is missing, or is
    /// `untrusted_unmonitored` — §8.1's untrusted-monitor protocols
    /// never admit an unmonitored untrusted selector for gate use
    /// (S6.3b; §05h §4 G7's policy validation).
    MonitorSelectorInadmissible {
        /// The inadmissible selector ref.
        selector: String,
        /// The failure detail.
        detail: String,
    },
    /// An artifact benefit was produced inside a judge selector's
    /// `held_out_from` closure while naming that selector — the
    /// benefit's producer may never consume the held-out judge's
    /// verdict (§5f.4; S6.3b; mirrors hh-verification's
    /// `JudgeLeakedIntoArtifact` at the evolution boundary).
    JudgeLeakedIntoArtifact {
        /// The artifact-benefit producer inside the closure.
        producer: String,
    },
    /// The proposal carries a semantic diff but the hypothesis names no
    /// `attribution_ref` — a designed semantic change must name its
    /// `hh-attribution-design/1` doc at S2 (§5h.7; S6.3b).
    AttributionMissing {
        /// The failure detail.
        detail: String,
    },
    /// The `attribution_ref`'s doc does not decode to
    /// `hh-attribution-design/1`, or the deposited `hh-attribution/1`
    /// report's label/effects do not satisfy the hypothesis's design
    /// (§5h.7's label ceiling; S6.3b).
    AttributionReportInvalid {
        /// The failure detail.
        detail: String,
    },
    /// A monitor selector vetoed the candidate — `trusted_weaker` and
    /// `untrusted_monitored` monitors are veto-only, never votes
    /// (S6.3b; §8.1).
    MonitorVetoed {
        /// The vetoing selector ref.
        selector: String,
    },

    // ── S10 retire ──────────────────────────────────────────────────────
    /// The retirement evidence's experiment is not `kind = retirement`
    /// or `retirement_batch`, or the arms are not a single-rule removal
    /// diff (§05h §4 S10).
    NotARetirementDiff {
        /// The failure detail.
        detail: String,
    },
    /// The removal test did not establish non-inferiority within the
    /// declared margin (§05h §4 S10 — the candidate revalidates, never
    /// silently retires).
    RemovalTestInconclusive {
        /// The failure detail.
        detail: String,
    },
    /// The retirement record's seal is not human-origin (§05h §4 S10 —
    /// retirement is human-sealed like acceptance).
    RetirementSealRefused {
        /// The failure detail.
        detail: String,
    },

    // ── campaign lifecycle / structural ─────────────────────────────────
    /// The campaign is `stopped` or `closed` — no new proposals or stage
    /// transitions mint (§05h §4).
    CampaignNotOpen {
        /// The campaign status at refusal.
        status: String,
    },
    /// The `to` transition is not legal from the candidate's current
    /// state (the state-machine's own refusal — never a silent fold).
    IllegalTransition {
        /// The current state spelling.
        from: String,
        /// The refused target spelling.
        to: String,
    },
    /// The candidate id is not registered in this campaign's lineage.
    UnknownCandidate {
        /// The unknown id.
        candidate_id: String,
    },
    /// The candidate's stored base does not equal the campaign's current
    /// head — a stale base refuses until re-based (§05h §4; ADR-0194).
    StaleBase {
        /// The failure detail.
        detail: String,
    },
    /// `uniform` slot allocation cannot cover a declared floor on every
    /// slot — the budget-splitting trap is refused at open, not
    /// discovered mid-campaign (§05h §4's stop-rule machinery;
    /// AC-R-2.9.5-13).
    BudgetSplittingTrap {
        /// The failure detail.
        detail: String,
    },
    /// The stop rule's `no_addressable_failure`/`stagnation`/`budget`
    /// condition fired — the campaign is `stopped`, not failed
    /// (§05h §4's stop predicate; AC-R-2.9.5-13).
    StopRule {
        /// The stop reason spelling.
        reason: String,
    },
    /// A `learned`-policy deployable the campaign produced lacks the
    /// evidence block ({search_budget_ref, artifact_benefit report ref at
    /// `matched_total`, debt record with `expiry ∋ model_version_change`})
    /// the gateway binds (AC-R-2.3.2-14).
    LearnedEvidenceIncomplete {
        /// The failure detail.
        detail: String,
    },
    /// A member-level schema violation (strict decode).
    SchemaViolation {
        /// The failure detail.
        detail: String,
    },
}

impl Refusal {
    /// The canonical refusal spelling — the `code` member of a durable
    /// `rejected{stage, code}` transition and the `Refused{reason}` the
    /// embed boundary reports. G3-1's literal spellings
    /// (`seal_refused{not_human}`, `self_modification_refused`) are this
    /// table's outputs.
    pub fn code(&self) -> String {
        match self {
            Refusal::HeldOutLeak { .. } => "held_out_leak".to_string(),
            Refusal::ReaderViolation { .. } => "reader_violation".to_string(),
            Refusal::ExecutableBytesInCorpus { .. } => "executable_bytes_in_corpus".to_string(),
            Refusal::EvidenceStale { .. } => "evidence_stale".to_string(),
            Refusal::UnresolvableEvidence { .. } => "unresolvable_evidence".to_string(),
            Refusal::AuthorityWidening { .. } => "authority_widening".to_string(),
            Refusal::BudgetLoosening { .. } => "budget_loosening".to_string(),
            Refusal::ValidityLoosening { .. } => "validity_loosening".to_string(),
            Refusal::MustCodeTarget { .. } => "must_code_target".to_string(),
            Refusal::ExcludedTarget { .. } => "excluded_target".to_string(),
            Refusal::SelfModificationRefused { .. } => "self_modification_refused".to_string(),
            Refusal::TooManyOps { .. } => "too_many_ops".to_string(),
            Refusal::DuplicateCandidate { .. } => "duplicate_candidate".to_string(),
            Refusal::DiffNotInvertible { .. } => "diff_not_invertible".to_string(),
            Refusal::InvalidProvenance { .. } => "invalid_provenance".to_string(),
            Refusal::HostedCoordinateUnsupported { .. } => {
                "hosted_coordinate_unsupported".to_string()
            }
            Refusal::HypothesisUnfalsifiable { .. } => "hypothesis_unfalsifiable".to_string(),
            Refusal::UnknownMetric { .. } => "unknown_metric".to_string(),
            Refusal::LeakedSplit { .. } => "leaked_split".to_string(),
            Refusal::TargetMismatch { .. } => "target_mismatch".to_string(),
            Refusal::PredictionFalsified { .. } => "prediction_falsified".to_string(),
            Refusal::InsufficientReplicates { .. } => "insufficient_replicates".to_string(),
            Refusal::JudgeOnlyAcceptance { .. } => "judge_only_acceptance".to_string(),
            Refusal::UnmatchedSearchBudget { .. } => "unmatched_search_budget".to_string(),
            Refusal::UnmatchedBudget { .. } => "unmatched_budget".to_string(),
            Refusal::UnbudgetedArm { .. } => "unbudgeted_arm".to_string(),
            Refusal::SearchBudgetIncomplete { .. } => "search_budget_incomplete".to_string(),
            Refusal::IncommensurableMatch { .. } => "incommensurable_match".to_string(),
            Refusal::ReportedOnlyDimension { .. } => "reported_only_dimension".to_string(),
            Refusal::SpecNotRegistered { .. } => "spec_not_registered".to_string(),
            Refusal::ForeignExperiment { .. } => "foreign_experiment".to_string(),
            Refusal::StageKindMismatch { .. } => "stage_kind_mismatch".to_string(),
            Refusal::NoArtifactBenefit { .. } => "no_artifact_benefit".to_string(),
            Refusal::NotHeldOut { .. } => "not_held_out".to_string(),
            Refusal::RetentionRegressed { .. } => "retention_regressed".to_string(),
            Refusal::VetoRegressed { .. } => "veto_regressed".to_string(),
            Refusal::ObservationalOnly { .. } => "observational_only".to_string(),
            Refusal::TransferUnreported { .. } => "transfer_unreported".to_string(),
            Refusal::CompatibilityUnproven { .. } => "compatibility_unproven".to_string(),
            Refusal::PolicyWidening { .. } => "policy_widening".to_string(),
            Refusal::OpaqueWithoutInterface { .. } => "opaque_without_interface".to_string(),
            Refusal::InProcessCandidate { .. } => "in_process_candidate".to_string(),
            Refusal::SecurityVetoTripped { .. } => "security_veto_tripped".to_string(),
            Refusal::AcceptanceIncomplete { .. } => "acceptance_incomplete".to_string(),
            Refusal::SealRefused { reason } => format!("seal_refused{{{reason}}}"),
            Refusal::NamespaceForbidden { .. } => "namespace_forbidden".to_string(),
            Refusal::CanaryAborted { .. } => "canary_aborted".to_string(),
            Refusal::SplitRolloutNotCommittable { .. } => {
                "split_rollout_not_committable".to_string()
            }
            Refusal::ConditionedRuleIncomplete { .. } => "conditioned_rule_incomplete".to_string(),
            Refusal::CoordinationLoosening { .. } => "coordination_loosening".to_string(),
            Refusal::JudgeSelectorUndeclared { .. } => "judge_selector_undeclared".to_string(),
            Refusal::MonitorSelectorInadmissible { .. } => {
                "monitor_selector_inadmissible".to_string()
            }
            Refusal::JudgeLeakedIntoArtifact { .. } => "judge_leaked_into_artifact".to_string(),
            Refusal::AttributionMissing { .. } => "attribution_missing".to_string(),
            Refusal::AttributionReportInvalid { .. } => "attribution_report_invalid".to_string(),
            Refusal::MonitorVetoed { .. } => "monitor_vetoed".to_string(),
            Refusal::NotARetirementDiff { .. } => "not_a_retirement_diff".to_string(),
            Refusal::RemovalTestInconclusive { .. } => "removal_test_inconclusive".to_string(),
            Refusal::RetirementSealRefused { .. } => "retirement_seal_refused".to_string(),
            Refusal::CampaignNotOpen { .. } => "campaign_not_open".to_string(),
            Refusal::IllegalTransition { .. } => "illegal_transition".to_string(),
            Refusal::UnknownCandidate { .. } => "unknown_candidate".to_string(),
            Refusal::StaleBase { .. } => "stale_base".to_string(),
            Refusal::BudgetSplittingTrap { .. } => "budget_splitting_trap".to_string(),
            Refusal::StopRule { .. } => "stop_rule".to_string(),
            Refusal::LearnedEvidenceIncomplete { .. } => "learned_evidence_incomplete".to_string(),
            Refusal::SchemaViolation { .. } => "schema_violation".to_string(),
        }
    }

    /// The stage name the refusal attaches to (`S0`…`S10` / `structural`)
    /// — the durable `rejected{stage, code}` spelling's `stage` member.
    pub fn stage(&self) -> &'static str {
        match self {
            Refusal::HeldOutLeak { .. }
            | Refusal::ReaderViolation { .. }
            | Refusal::ExecutableBytesInCorpus { .. }
            | Refusal::EvidenceStale { .. }
            | Refusal::MonitorSelectorInadmissible { .. }
            | Refusal::UnresolvableEvidence { .. } => "S0",
            Refusal::AuthorityWidening { .. }
            | Refusal::BudgetLoosening { .. }
            | Refusal::ValidityLoosening { .. }
            | Refusal::MustCodeTarget { .. }
            | Refusal::ExcludedTarget { .. }
            | Refusal::SelfModificationRefused { .. }
            | Refusal::TooManyOps { .. }
            | Refusal::DuplicateCandidate { .. }
            | Refusal::DiffNotInvertible { .. }
            | Refusal::InvalidProvenance { .. }
            | Refusal::HostedCoordinateUnsupported { .. } => "S1",
            Refusal::HypothesisUnfalsifiable { .. }
            | Refusal::UnknownMetric { .. }
            | Refusal::LeakedSplit { .. }
            | Refusal::AttributionMissing { .. }
            | Refusal::TargetMismatch { .. } => "S2",
            Refusal::PredictionFalsified { .. }
            | Refusal::InsufficientReplicates { .. }
            | Refusal::MonitorVetoed { .. }
            | Refusal::JudgeOnlyAcceptance { .. } => "S3",
            Refusal::UnmatchedSearchBudget { .. }
            | Refusal::UnmatchedBudget { .. }
            | Refusal::UnbudgetedArm { .. }
            | Refusal::SearchBudgetIncomplete { .. }
            | Refusal::IncommensurableMatch { .. }
            | Refusal::ReportedOnlyDimension { .. }
            | Refusal::SpecNotRegistered { .. }
            | Refusal::ForeignExperiment { .. } => "S4",
            Refusal::NoArtifactBenefit { .. }
            | Refusal::NotHeldOut { .. }
            | Refusal::RetentionRegressed { .. }
            | Refusal::VetoRegressed { .. }
            | Refusal::ObservationalOnly { .. }
            | Refusal::JudgeLeakedIntoArtifact { .. }
            | Refusal::AttributionReportInvalid { .. } => "S5",
            Refusal::TransferUnreported { .. } | Refusal::CompatibilityUnproven { .. } => "S6",
            Refusal::PolicyWidening { .. }
            | Refusal::OpaqueWithoutInterface { .. }
            | Refusal::InProcessCandidate { .. }
            | Refusal::SecurityVetoTripped { .. } => "S7",
            Refusal::AcceptanceIncomplete { .. }
            | Refusal::SealRefused { .. }
            | Refusal::NamespaceForbidden { .. } => "S8",
            Refusal::CanaryAborted { .. }
            | Refusal::SplitRolloutNotCommittable { .. }
            | Refusal::ConditionedRuleIncomplete { .. } => "S9",
            Refusal::NotARetirementDiff { .. }
            | Refusal::RemovalTestInconclusive { .. }
            | Refusal::RetirementSealRefused { .. } => "S10",
            // The stage label is the member the refusing stage stamped —
            // `check_stage_experiment` serves S4, S5, and S10.
            Refusal::StageKindMismatch { stage, .. } => stage,
            _ => "structural",
        }
    }

    /// The refusal rendered as a `{refusal: {code, stage, detail}}` record —
    /// the member the caller's report carries (and the `report_ref` the
    /// rejected transition cites when a report body exists).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("code", Json::str(self.code())),
            ("stage", Json::str(self.stage())),
            ("detail", Json::str(format!("{self:?}"))),
        ])
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.code(), self)
    }
}

/// The pipeline's error sum — a gate [`Refusal`], a store failure, a
/// LabDocs failure, or a state-machine violation.
#[derive(Debug)]
pub enum EvolutionError {
    /// A typed stage-gate refusal — the candidate's `rejected` row has
    /// landed (or the admission refused) before the error returns.
    Refusal(Refusal),
    /// The ledger store failed.
    Store(LedgerError),
    /// A LabDocs read/write failed.
    Docs(String),
    /// A schema/decode violation on a records-in record.
    Schema(String),
}

impl EvolutionError {
    /// The `Refused{reason}` code the embed boundary reports.
    pub fn code(&self) -> String {
        match self {
            EvolutionError::Refusal(r) => r.code(),
            EvolutionError::Store(e) => format!("store:{e:?}"),
            EvolutionError::Docs(d) => format!("docs:{d}"),
            EvolutionError::Schema(d) => format!("schema:{d}"),
        }
    }
}

impl std::fmt::Display for EvolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl std::error::Error for EvolutionError {}

impl From<Refusal> for EvolutionError {
    fn from(r: Refusal) -> EvolutionError {
        EvolutionError::Refusal(r)
    }
}

impl From<LedgerError> for EvolutionError {
    fn from(e: LedgerError) -> EvolutionError {
        EvolutionError::Store(e)
    }
}
