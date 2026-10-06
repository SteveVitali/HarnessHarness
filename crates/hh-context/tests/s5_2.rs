//! S5.2 — the Stage-5 compaction + context-policy + retrieval families
//! (§5c.1/§5c.2/§5c.3; R-2.4.2, `R-2.4.1¹`, `R-2.4.3²`, R-2.4.5):
//!
//! - AC-R-2.4.2-6 — conditioned-rule registration (`VariantDeclaration`,
//!   `set_compaction_policy` decode, profile-conditioned resolution,
//!   `compaction_reminder`).
//! - AC-R-2.4.2-10 — the model-owned boundary (`offload_note` /
//!   `new_window` / `fold` parse to the op grammar; `check_proposal`
//!   invariants apply to a model-authored proposal identically).
//! - AC-R-2.4.1-11 — the five policy variants, demotion wrappers, reminder
//!   placement, conditioned-rule retirement.
//! - AC-R-2.4.3-10 — `similarity_rerank` bound through the pinned
//!   `Embedder` port; `embedder_calls` counted; `deterministic = false`.
//! - R-2.4.5 — `procedure_target` rule consumption (selector match +
//!   debt completeness, `ConditionedRuleIncomplete`).

use std::collections::BTreeMap;

use hh_context::assemble::{self, AssemblyRequest};
use hh_context::compact::{
    self, CompactInput, CompactionOp, CompactionStatus, CompactionStrategy, CompactionTrigger,
    SummarizeInput, Summarizer, SummarizerError, SummarizerOutput,
};
use hh_context::compact_family::{
    self, decode_compaction_reminder, decode_compaction_rule, model_owned_ops,
    model_owned_proposal, reminder_candidate, resolve_compaction_policy, CheckpointExtractor,
    CheckpointOutput, FreshWindowWithNotes, ModelCompactCall, ProviderCompactInput,
    ProviderCompactOutcome, ProviderCompaction, ProviderCompactionVariant, StructuredCheckpoint,
    SummarizeRolling, VariantDeclaration, WorldStateRefresh, FRESH_WINDOW_WITH_NOTES_REF,
    MODEL_OWNED_REF, PROVIDER_COMPACTION_REF, STRUCTURED_CHECKPOINT_REF, SUMMARIZE_ROLLING_REF,
    WORLD_STATE_REFRESH_REF,
};
use hh_context::events::CollectSink;
use hh_context::memory::{MemoryDraft, MemoryStore, WriteContext};
use hh_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, ExcerptReport, OffloadHandle,
    PlannedItem, SlotFill, ValidityPolicy,
};
use hh_context::policy::{
    self, AdmittedCandidate, ConditionedRule, ContextPolicy, PolicyRequest, RegistrationError,
    RuleCondition,
};
use hh_context::policy_family::{
    self, DemotionWrapper, JitHandles, PriorityEviction, RecencyPlusPins, RetrievalAugmented,
    WorldStateDiff,
};
use hh_context::procedure::{resolve_procedure_target, ProcedureTargetError, ProcedureTargetRule};
use hh_context::retrieve::{
    self, Embedder, RetrievalBudget, RetrievalError, RetrievalIndexes, RetrievalRequest,
    SlotConstraints, SIMILARITY_RERANK,
};
use hh_context::vocab::{
    CandidateKind, CandidateState, ConflictPolicy, Layer, LifecycleStateKind, MemoryContent,
    MemoryKind, PriorityClass, Retention, RetrievalQuery,
};
use hh_hir::records::{AssumptionDebtRecord, DebtStatus, Validity};
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, PersistenceScope};
use hh_provenance::label::Label;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

// ── helpers ──────────────────────────────────────────────────────────────────

fn kprov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("kernel:context", 0)
}

fn cand_at(
    id: &str,
    kind: CandidateKind,
    authority: AuthorityClass,
    tokens: u64,
    priority: PriorityClass,
    seq: u64,
) -> Candidate {
    Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind,
        state: CandidateState::Expanded,
        retention: Retention::Optional(priority),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".to_string(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(authority),
        provenance: kprov(),
        validity: Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

fn planned_item(c: &Candidate, tokens: u64) -> PlannedItem {
    PlannedItem {
        candidate_id: c.candidate_id.clone(),
        context_item_id: c.context_item_id.clone().unwrap(),
        artefact_id: c.artefact_id.clone(),
        delivery_id: format!("del:{}", c.candidate_id),
        authority: c.label.authority,
        label: c.label.clone(),
        tokens,
        state: c.state,
        delivered_by_reference: false,
        derived_from: None,
    }
}

fn plan_of(
    items: Vec<(Candidate, PlannedItem)>,
    cap_occupancy: u64,
) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let mut candidates = BTreeMap::new();
    let mut planned = Vec::new();
    for (c, p) in items {
        candidates.insert(c.candidate_id.clone(), c);
        planned.push(p);
    }
    let label = planned
        .iter()
        .filter(|i| i.state == CandidateState::Expanded)
        .fold(Label::top(), |acc, i| acc.join(&i.label));
    let plan = ContextPlan {
        plan_id: "plan:test".into(),
        model_call_id: "mc1".into(),
        derived_from: DerivedFrom {
            run_id: "run1".into(),
            seq: 10,
            view_hash: "sha256:view".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items: planned,
        }],
        omitted: vec![],
        context_label: label,
        occupancy_estimate: cap_occupancy,
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

fn compact_input<'a>(
    plan: &'a ContextPlan,
    candidates: BTreeMap<String, Candidate>,
    trigger: CompactionTrigger,
    cap: u64,
    needed: u64,
) -> CompactInput<'a> {
    CompactInput {
        plan,
        candidates,
        window_cap: cap,
        trigger,
        needed,
        target_fraction_ppm: compact::DEFAULT_TARGET_FRACTION_PPM,
        scope: PersistenceScope::Run,
        at: 20,
        run_id: "run1".into(),
        summarizer: None,
        slot_min_authority: [("transcript".to_string(), AuthorityClass::Unverified)]
            .into_iter()
            .collect(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    }
}

fn debt(rule_id: &str) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: hh_hir::leaves::Text::new("h", "alice", kprov()),
        evidence_refs: vec![hh_hir::EvidenceRef::legacy("sha256:ev")],
        owner: hh_hir::OwnerRef::principal("alice"),
        expiry_condition: hh_hir::ExpiryCondition {
            kind: hh_hir::ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "sha256:test".into(),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: None,
        created_by: None,
        created_at: None,
        supersedes: None,
    }
}

struct StubSummarizer;

impl Summarizer for StubSummarizer {
    fn summarize(&self, input: &SummarizeInput) -> Result<SummarizerOutput, SummarizerError> {
        Ok(SummarizerOutput {
            text: format!("summary of {}", input.items.len()),
            tokens: 12,
            usage: Json::obj([("calls", Json::Int(1))]),
        })
    }
}

struct StubExtractor;

impl CheckpointExtractor for StubExtractor {
    fn extractor_ref(&self) -> &str {
        "extractor/checkpoint@1"
    }
    fn extract(
        &self,
        items: &[compact::SummarizeItem],
        _schema_ref: &str,
        _section_order: &[String],
    ) -> Result<CheckpointOutput, String> {
        Ok(CheckpointOutput {
            body: format!("checkpoint over {}", items.len()),
            tokens: 10,
        })
    }
}

struct StubProvider {
    forgotten_ids: Option<Vec<String>>,
}

impl ProviderCompaction for StubProvider {
    fn provider_compact(
        &self,
        input: &ProviderCompactInput,
    ) -> Result<ProviderCompactOutcome, String> {
        Ok(ProviderCompactOutcome {
            summary_text: format!("provider summary over {}", input.items.len()),
            summary_tokens: 8,
            forgotten_ids: self.forgotten_ids.clone(),
            usage: Json::obj([("calls", Json::Int(1))]),
        })
    }
}

// ── AC-R-2.4.2-6 — conditioned rules at registration + resolution ────────────

#[test]
fn ac_r_2_4_2_6_variant_declaration_refuses_model_identity_and_free_ops() {
    let mut d = VariantDeclaration::minimal("hh/x@1");
    // A model-identity branch fails registration (T-LCD-01).
    d.model_conditioned_rules = vec![ConditionedRule {
        rule_id: "r-mi".into(),
        conditioned_on: RuleCondition::ModelIdentity("model/x".into()),
        debt: Some(debt("r-mi")),
    }];
    assert!(matches!(
        compact_family::check_variant_declaration(&d),
        Err(RegistrationError::ModelIdentityCondition { .. })
    ));
    // A conditioned rule without a complete debt record fails.
    let mut d = VariantDeclaration::minimal("hh/x@1");
    d.model_conditioned_rules = vec![ConditionedRule {
        rule_id: "r-nodebt".into(),
        conditioned_on: RuleCondition::Profile("profile/p".into()),
        debt: None,
    }];
    assert!(compact_family::check_variant_declaration(&d).is_err());
    // An op kind outside the closed grammar refuses (no silent extension).
    let mut d = VariantDeclaration::minimal("hh/x@1");
    d.op_kinds.insert("conjure".to_string());
    assert!(matches!(
        compact_family::check_variant_declaration(&d),
        Err(RegistrationError::UndeclaredNonDeterminism { .. })
    ));
    // A clean declaration passes — every shipped variant's `declare()` does.
    for v in [
        SummarizeRolling {
            keep_recent_tokens: 64,
            reserve_tokens: 0,
            summarizer_profile: "summ/pinned".into(),
            instructions_ref: None,
            incremental: false,
            split_turn: false,
            retain_principal_tokens: 0,
            max_summary_tokens: None,
            input_reduction: None,
        }
        .declare(),
        WorldStateRefresh.declare(),
        ProviderCompactionVariant {
            capability_ref: "cap/provider_compact".into(),
        }
        .declare(),
        FreshWindowWithNotes {
            reminder_rule: "rule/rem".into(),
            note_capability: "cap/note".into(),
            history_capability: "cap/hist".into(),
            fallback_buffer_tokens: 32,
        }
        .declare(),
    ] {
        compact_family::check_variant_declaration(&v)
            .unwrap_or_else(|e| panic!("{} refused: {e:?}", v.variant_id));
    }
}

#[test]
fn ac_r_2_4_2_6_conditioned_rules_resolve_by_profile_never_identity() {
    let params = Json::obj([
        ("variant_ref", Json::str(SUMMARIZE_ROLLING_REF)),
        (
            "soft_threshold",
            Json::obj([("occupancy_ppm", Json::Int(800_000))]),
        ),
        ("schedule", Json::obj([("every_turns", Json::Int(4))])),
    ]);
    let rule = decode_compaction_rule(
        "rule/compact-soft",
        &params,
        Some(RuleCondition::Profile("profile/pro".into())),
        Some(debt("rule/compact-soft")),
    )
    .unwrap();
    let unconditional = decode_compaction_rule("rule/base", &Json::obj([]), None, None).unwrap();
    let measured = decode_compaction_rule(
        "rule/measured",
        &Json::obj([("variant_ref", Json::str(WORLD_STATE_REFRESH_REF))]),
        Some(RuleCondition::ComplianceMeasurement("snap/1".into())),
        Some(debt("rule/measured")),
    )
    .unwrap();
    // Fires on the bound profile; never on another, never on identity.
    let resolved = resolve_compaction_policy(
        &[unconditional.clone(), rule.clone(), measured.clone()],
        "profile/pro",
    );
    assert_eq!(resolved.variant_ref.as_deref(), Some(SUMMARIZE_ROLLING_REF));
    assert_eq!(resolved.soft_thresholds.len(), 1);
    assert_eq!(resolved.schedules.len(), 1);
    let resolved_other =
        resolve_compaction_policy(&[unconditional, rule, measured], "profile/other");
    assert_eq!(resolved_other.variant_ref, None);
    assert!(resolved_other.soft_thresholds.is_empty());
    // The soft triggers mint with the rule ids as their coordinates.
    let triggers = compact_family::soft_triggers(&resolved);
    assert!(triggers.iter().any(|t| matches!(
        t,
        CompactionTrigger::OccupancySoft { rule_id } if rule_id == "rule/compact-soft"
    )));
    assert!(triggers.iter().any(|t| matches!(
        t,
        CompactionTrigger::Schedule { rule_id } if rule_id == "rule/compact-soft"
    )));
    // A model-identity rule is refused at decode.
    assert!(matches!(
        decode_compaction_rule(
            "rule/mi",
            &Json::obj([]),
            Some(RuleCondition::ModelIdentity("m".into())),
            Some(debt("rule/mi")),
        ),
        Err(RegistrationError::ModelIdentityCondition { .. })
    ));
}

#[test]
fn ac_r_2_4_2_6_compaction_reminder_is_a_profile_owned_notice() {
    let reminder = decode_compaction_reminder(&Json::obj([
        ("text_ref", Json::str("text:reminder")),
        ("placement", Json::str("external")),
    ]))
    .unwrap();
    assert_eq!(reminder.text_ref, "text:reminder");
    assert_eq!(reminder.placement, "external");
    // A missing text ref is a typed refusal, never a silent empty string.
    assert!(decode_compaction_reminder(&Json::obj([])).is_err());
    // The candidate mints only on an applied/fallback record (a failed
    // compaction carries no reminder) and is `required` + kernel-labelled —
    // an advisory, never an authority.
    let c = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        50,
        PriorityClass::Commentary,
        1,
    );
    let (plan, cands) = plan_of(vec![(c.clone(), planned_item(&c, 50))], 50);
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 40, 10);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    assert_eq!(out.record.status, CompactionStatus::Applied);
    let cand = reminder_candidate(&reminder, &out.record, "run1", 21, "est/pinned")
        .expect("applied record mints the reminder candidate");
    assert_eq!(cand.kind, CandidateKind::KernelNotice);
    assert!(cand.retention.is_required());
    assert_eq!(cand.slot_hint.as_deref(), Some("external"));
    assert_eq!(cand.artefact_id.as_deref(), Some("text:reminder"));
}

// ── AC-R-2.4.2-10 — the model-owned boundary ─────────────────────────────────

#[test]
fn ac_r_2_4_2_10_model_owned_calls_parse_to_ops_and_face_the_same_invariants() {
    let keep = cand_at(
        "keep",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        40,
        PriorityClass::TranscriptTail,
        1,
    );
    let drop = cand_at(
        "drop",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        60,
        PriorityClass::Commentary,
        2,
    );
    let req_c = {
        let mut c = cand_at(
            "req",
            CandidateKind::DefinitionInstruction,
            AuthorityClass::Definition,
            20,
            PriorityClass::TranscriptTail,
            0,
        );
        c.retention = Retention::Required;
        c
    };
    let (plan, cands) = plan_of(
        vec![
            (req_c.clone(), planned_item(&req_c, 20)),
            (keep.clone(), planned_item(&keep, 40)),
            (drop.clone(), planned_item(&drop, 60)),
        ],
        120,
    );
    let input = compact_input(&plan, cands, CompactionTrigger::RequestPrincipal, 100, 30);
    // `new_window{keep_ids}` → Evict{optional ∉ keep}; `required` items are
    // never in the model's forget set (the parse itself respects I-REQ).
    let ops = model_owned_ops(
        &input,
        &ModelCompactCall::NewWindow {
            keep_ids: vec!["sha256:item-keep".to_string()],
        },
    );
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        CompactionOp::Evict { item_ids, .. } => {
            assert_eq!(item_ids, &["sha256:item-drop".to_string()]);
        }
        other => panic!("expected evict, found {other:?}"),
    }
    // `fold` → Summarize.
    let ops = model_owned_ops(
        &input,
        &ModelCompactCall::Fold {
            item_ids: vec!["sha256:item-drop".to_string()],
            insert_at: 1,
        },
    );
    assert!(matches!(ops[0], CompactionOp::Summarize { .. }));
    // A model-authored proposal touching a `required` item is refused with
    // `PolicyViolation` — the proposal's provenance is the model; the
    // kernel invariants apply identically (AC-R-2.4.2-10).
    let bad = model_owned_proposal(
        &input,
        vec![CompactionOp::Evict {
            item_ids: vec!["sha256:item-req".to_string()],
            placeholder: compact::Placeholder::KernelOmission,
        }],
    );
    assert!(matches!(
        bad,
        Err(compact::CompactError::PolicyViolation { .. })
    ));
    // The full model-owned path applies and mints a `model_owned` record.
    let mut sink = CollectSink::default();
    let out = compact_family::compact_model_owned(
        &input,
        &ModelCompactCall::NewWindow {
            keep_ids: vec![
                "sha256:item-keep".to_string(),
                "sha256:item-req".to_string(),
            ],
        },
        &mut sink,
    )
    .unwrap();
    assert_eq!(out.record.variant_ref, MODEL_OWNED_REF);
    assert!(sink
        .events
        .iter()
        .any(|(k, _)| k == "context.compaction.completed"));
}

// ── The family variants — propose + execute legs ─────────────────────────────

#[test]
fn summarize_rolling_proposes_summarize_with_deterministic_fallback() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        120,
        PriorityClass::Commentary,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        120,
        PriorityClass::TranscriptTail,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let mut input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 264, 20);
    input.item_texts = [
        ("sha256:item-c1".to_string(), "body one".to_string()),
        ("sha256:item-c2".to_string(), "body two".to_string()),
    ]
    .into_iter()
    .collect();
    let sz = StubSummarizer;
    input.summarizer = Some(&sz);
    let v = SummarizeRolling {
        keep_recent_tokens: 60,
        reserve_tokens: 0,
        summarizer_profile: "summ/pinned".into(),
        instructions_ref: Some("text:compact-prompt".into()),
        incremental: true,
        split_turn: false,
        retain_principal_tokens: 0,
        max_summary_tokens: Some(32),
        input_reduction: None,
    };
    assert_eq!(v.variant_ref(), SUMMARIZE_ROLLING_REF);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&v], &mut sink).unwrap();
    assert_eq!(out.record.variant_ref, SUMMARIZE_ROLLING_REF);
    assert_eq!(out.record.status, CompactionStatus::Applied);
    assert_eq!(out.view.summary_items.len(), 1);
    assert!(out.record.summariser_usage.is_some());
    // The proposal carries the declared deterministic fallback + the
    // model-call members (summarizer_profile, instructions_ref).
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let proposal = v.propose(&input, &assessment).unwrap();
    assert!(proposal
        .fallback
        .as_ref()
        .is_some_and(|f| f.iter().all(|op| matches!(op, CompactionOp::Evict { .. }))));
    assert_eq!(proposal.summarizer_profile.as_deref(), Some("summ/pinned"));
    assert_eq!(
        proposal.instructions_ref.as_deref(),
        Some("text:compact-prompt")
    );
}

#[test]
fn structured_checkpoint_executes_through_the_deterministic_extractor() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        120,
        PriorityClass::Commentary,
        1,
    );
    let (plan, cands) = plan_of(vec![(c1.clone(), planned_item(&c1, 120))], 120);
    let mut input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 60, 60);
    input.item_texts = [("sha256:item-c1".to_string(), "body one".to_string())]
        .into_iter()
        .collect();
    let ext = StubExtractor;
    input.extractor = Some(&ext);
    let v = StructuredCheckpoint {
        schema_ref: "schema/checkpoint".into(),
        section_order: vec!["goal".into(), "state".into()],
        projection_ref: "extractor/checkpoint@1".into(),
    };
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&v], &mut sink).unwrap();
    assert_eq!(out.record.variant_ref, STRUCTURED_CHECKPOINT_REF);
    assert_eq!(out.view.summary_items.len(), 1);
    // `Restructure` is deterministic — no summariser usage posts.
    assert!(out.record.summariser_usage.is_none());
}

#[test]
fn provider_compaction_delegates_and_marks_unreported_forgotten_all_prior() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        120,
        PriorityClass::Commentary,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        120,
        PriorityClass::Commentary,
        2,
    );
    // The provider cannot report the forgotten ids → `all_prior` (T-LCD-11).
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let mut input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 60, 100);
    input.item_texts = [
        ("sha256:item-c1".to_string(), "body one".to_string()),
        ("sha256:item-c2".to_string(), "body two".to_string()),
    ]
    .into_iter()
    .collect();
    let provider = StubProvider {
        forgotten_ids: None,
    };
    input.provider = Some(&provider);
    let v = ProviderCompactionVariant {
        capability_ref: "cap/provider_compact".into(),
    };
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&v], &mut sink).unwrap();
    assert_eq!(out.record.variant_ref, PROVIDER_COMPACTION_REF);
    assert_eq!(out.view.forgotten_range.as_deref(), Some("all_prior"));
    assert_eq!(out.view.summary_items.len(), 1);
    // When the provider reports the forgotten set the range stands absent.
    let (plan2, cands2) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let mut input2 = compact_input(&plan2, cands2, CompactionTrigger::OccupancyHard, 60, 100);
    input2.item_texts = [
        ("sha256:item-c1".to_string(), "body one".to_string()),
        ("sha256:item-c2".to_string(), "body two".to_string()),
    ]
    .into_iter()
    .collect();
    let provider2 = StubProvider {
        forgotten_ids: Some(vec![
            "sha256:item-c1".to_string(),
            "sha256:item-c2".to_string(),
        ]),
    };
    input2.provider = Some(&provider2);
    let mut sink2 = CollectSink::default();
    let out2 = compact::compact(&input2, &[&v], &mut sink2).unwrap();
    assert!(out2.view.forgotten_range.is_none());
}

#[test]
fn world_state_refresh_and_fresh_window_declare_their_boundaries() {
    let env_old = cand_at(
        "e1",
        CandidateKind::EnvironmentState,
        AuthorityClass::External,
        60,
        PriorityClass::Commentary,
        1,
    );
    let obs = cand_at(
        "o1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        60,
        PriorityClass::Commentary,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (env_old.clone(), planned_item(&env_old, 60)),
            (obs.clone(), planned_item(&obs, 60)),
        ],
        120,
    );
    let mut input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 60, 60);
    input.item_kinds = [
        (
            "sha256:item-e1".to_string(),
            "environment_state".to_string(),
        ),
        ("sha256:item-o1".to_string(), "observation".to_string()),
    ]
    .into_iter()
    .collect();
    let v = WorldStateRefresh;
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let p = v.propose(&input, &assessment).unwrap();
    // Only `environment_state` items are re-observation targets.
    assert_eq!(p.ops.len(), 1);
    assert_eq!(p.ops[0].consumed(), &["sha256:item-e1".to_string()]);
    // `fresh_window_with_notes` is the model-owned variant: deterministic =
    // false, control_boundary_compact = model.
    let fw = FreshWindowWithNotes {
        reminder_rule: "rule/rem".into(),
        note_capability: "cap/offload_note".into(),
        history_capability: "cap/history".into(),
        fallback_buffer_tokens: 60,
    };
    let d = fw.declare();
    assert_eq!(d.variant_id, FRESH_WINDOW_WITH_NOTES_REF);
    assert!(!d.deterministic);
    assert_eq!(
        d.control_boundary_compact,
        compact_family::ControlBoundaryCompact::Model
    );
    assert_eq!(v.declare().variant_id, WORLD_STATE_REFRESH_REF);
}

// ── AC-R-2.4.1-11 — the context-policy family ────────────────────────────────

fn admitted(c: &Candidate, slots: &[&str]) -> AdmittedCandidate {
    AdmittedCandidate {
        candidate: c.clone(),
        admissible_slots: slots.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn ac_r_2_4_1_11_all_five_variants_declare_and_select() {
    let c_old_env = cand_at(
        "e-old",
        CandidateKind::EnvironmentState,
        AuthorityClass::External,
        20,
        PriorityClass::Commentary,
        1,
    );
    let c_new_env = cand_at(
        "e-new",
        CandidateKind::EnvironmentState,
        AuthorityClass::External,
        20,
        PriorityClass::Commentary,
        5,
    );
    let c_mem = cand_at(
        "m1",
        CandidateKind::Memory,
        AuthorityClass::Delegate,
        20,
        PriorityClass::MemoryIndex,
        3,
    );
    let cands = [
        admitted(&c_old_env, &["external"]),
        admitted(&c_new_env, &["external"]),
        admitted(&c_mem, &["external"]),
    ];
    let layout = hh_context::plan::default_layout();
    let null = Json::Null;
    // Every variant's declaration passes the shared conditioned-rule check.
    for (i, d) in [
        PriorityEviction.declare(),
        WorldStateDiff.declare(),
        JitHandles.declare(),
        RecencyPlusPins.declare(),
        RetrievalAugmented.declare(),
    ]
    .iter()
    .enumerate()
    {
        policy::check(d).unwrap_or_else(|e| panic!("variant {i} refused: {e:?}"));
    }
    // world_state_diff — the stale state heads the eviction order.
    let req = PolicyRequest {
        candidates: &cands,
        layout: &layout,
        budget_remaining: 1_000_000,
        params: &null,
    };
    let sel = WorldStateDiff.select(&req).unwrap();
    assert_eq!(sel.evict_order.first().map(String::as_str), Some("e-old"));
    // priority_eviction — declared classes order; `evict_first_kinds` hoists.
    let params = Json::obj([("evict_first_kinds", Json::Arr(vec![Json::str("memory")]))]);
    let req = PolicyRequest {
        candidates: &cands,
        layout: &layout,
        budget_remaining: 1_000_000,
        params: &params,
    };
    let sel = PriorityEviction.select(&req).unwrap();
    assert_eq!(sel.evict_order.first().map(String::as_str), Some("m1"));
    // recency_plus_pins — the pinned candidate is last to evict, first in
    // its slot order.
    let params = Json::obj([("pins", Json::Arr(vec![Json::str("e-old")]))]);
    let req = PolicyRequest {
        candidates: &cands,
        layout: &layout,
        budget_remaining: 1_000_000,
        params: &params,
    };
    let sel = RecencyPlusPins.select(&req).unwrap();
    assert_eq!(sel.evict_order.last().map(String::as_str), Some("e-old"));
    // jit_handles — a candidate with a readable handle delivers
    // `by_reference`.
    let mut handled = cand_at(
        "h1",
        CandidateKind::Memory,
        AuthorityClass::Delegate,
        20,
        PriorityClass::MemoryIndex,
        4,
    );
    handled.handle = Some(OffloadHandle {
        content_address: "sha256:full".into(),
        media_type: "text/plain".into(),
        size: 999,
        label: Label::at(AuthorityClass::Delegate),
        excerpt_report: ExcerptReport {
            truncated_by: None,
            total_lines: 10,
            total_bytes: 999,
            retained_range: (0, 20),
        },
        read_capability: "cap/read".into(),
    });
    let cands2 = [admitted(&handled, &["external"])];
    let req = PolicyRequest {
        candidates: &cands2,
        layout: &layout,
        budget_remaining: 1_000_000,
        params: &null,
    };
    let sel = JitHandles.select(&req).unwrap();
    assert_eq!(sel.by_reference, vec!["h1".to_string()]);
    // retrieval_augmented — `rank_order` leads the slot order for
    // retrievable kinds; the policy never fetches.
    let params = Json::obj([(
        "rank_order",
        Json::Arr(vec![Json::str("m1"), Json::str("e-new")]),
    )]);
    let req = PolicyRequest {
        candidates: &cands,
        layout: &layout,
        budget_remaining: 1_000_000,
        params: &params,
    };
    let sel = RetrievalAugmented.select(&req).unwrap();
    let ext_order = sel.order.get("external").expect("external slot order");
    assert_eq!(ext_order.first().map(String::as_str), Some("m1"));
}

#[test]
fn ac_r_2_4_1_11_demotion_wrappers_route_to_external_never_inward() {
    // Decode refuses a non-external target (a wrapper never routes inward).
    assert!(policy_family::decode_demotion_wrappers(&Json::obj([(
        "demotion_wrappers",
        Json::Arr(vec![Json::obj([
            ("wrapper_id", Json::str("w1")),
            ("applies_to", Json::Arr(vec![Json::str("transcript_item")])),
            ("target_slot", Json::str("transcript")),
            ("text_ref", Json::str("text:demoted")),
        ])]),
    )]))
    .is_err());
    let wrappers = policy_family::decode_demotion_wrappers(&Json::obj([(
        "demotion_wrappers",
        Json::Arr(vec![Json::obj([
            ("wrapper_id", Json::str("w1")),
            ("applies_to", Json::Arr(vec![Json::str("transcript_item")])),
            ("target_slot", Json::str("external")),
            ("text_ref", Json::str("text:demoted")),
        ])]),
    )]))
    .unwrap();
    assert_eq!(wrappers.len(), 1);
    // A `transcript_item` at `external` authority is admissible nowhere —
    // the transcript slot admits the kind but floors at `delegate`; the
    // `unverified` slot excludes the kind. With the wrapper it lands in
    // `external` (floor `external` — satisfied) and the demotion is
    // recorded.
    let c = cand_at(
        "pm",
        CandidateKind::TranscriptItem,
        AuthorityClass::External,
        10,
        PriorityClass::TranscriptTail,
        1,
    );
    let req = |wrappers: Vec<DemotionWrapper>| AssemblyRequest {
        model_call_id: "mc1".to_string(),
        view: DerivedFrom {
            run_id: "run1".to_string(),
            seq: 10,
            view_hash: "sha256:view".to_string(),
        },
        at_seq: 10,
        min_view_seq: None,
        candidates: vec![c.clone()],
        advisories: Vec::new(),
        layout: hh_context::plan::default_layout(),
        budget: hh_context::plan::ContextBudget {
            window_cap: 10_000,
            margin: 0,
            reservations: Vec::new(),
            hard: true,
        },
        estimator_ref: "est/pinned".to_string(),
        policy_params: Json::Null,
        demotion_wrappers: wrappers,
    };
    let mut sink = CollectSink::default();
    let bare = assemble::assemble(
        &req(Vec::new()),
        &hh_context::policy::DefaultPolicy::default(),
        "layout/default",
        "none",
        1,
        &mut sink,
    )
    .unwrap();
    assert!(bare
        .plan
        .slots
        .iter()
        .all(|s| s.items.iter().all(|i| i.candidate_id != "pm")));
    let mut sink = CollectSink::default();
    let out = assemble::assemble(
        &req(wrappers),
        &hh_context::policy::DefaultPolicy::default(),
        "layout/default",
        "none",
        1,
        &mut sink,
    )
    .unwrap();
    assert_eq!(out.demotions.len(), 1);
    assert_eq!(out.demotions[0].wrapper_id, "w1");
    assert_eq!(out.demotions[0].target_slot, "external");
    assert_eq!(out.demotions[0].text_ref, "text:demoted");
    let ext = out
        .plan
        .slots
        .iter()
        .find(|s| s.slot_id == "external")
        .expect("external slot");
    assert!(ext.items.iter().any(|i| i.candidate_id == "pm"));
}

#[test]
fn ac_r_2_4_1_11_reminder_placement_and_rule_retirement() {
    let p = policy_family::decode_reminder_placement(&Json::obj([(
        "reminder_placement",
        Json::obj([("kernel_notice", Json::str("external"))]),
    )]));
    assert_eq!(p.slot_for("kernel_notice"), "external");
    // Undeclared kinds keep the kernel default.
    assert_eq!(p.slot_for("budget_reminder"), "kernel");
    // `retire_rule` — the residual set re-vets clean (the valid residual
    // arm AC-R-2.4.1-11 names).
    let rules = vec![
        ConditionedRule {
            rule_id: "r1".into(),
            conditioned_on: RuleCondition::Profile("p".into()),
            debt: Some(debt("r1")),
        },
        ConditionedRule {
            rule_id: "r2".into(),
            conditioned_on: RuleCondition::Profile("p".into()),
            debt: Some(debt("r2")),
        },
    ];
    let inputs = compact_family::required_variant_inputs();
    let residual = policy_family::retire_rule(&rules, "r1", &inputs).unwrap();
    assert_eq!(residual.len(), 1);
    assert_eq!(residual[0].rule_id, "r2");
    // Retiring a rule does not launder a bad residual — a model-identity
    // member still refuses on the recheck.
    let bad = vec![ConditionedRule {
        rule_id: "r-mi".into(),
        conditioned_on: RuleCondition::ModelIdentity("m".into()),
        debt: Some(debt("r-mi")),
    }];
    assert!(matches!(
        policy_family::retire_rule(&bad, "absent", &inputs),
        Err(RegistrationError::ModelIdentityCondition { .. })
    ));
}

// ── AC-R-2.4.3-10 — similarity_rerank under the pinned embedder ──────────────

struct StubEmbedder {
    reference: &'static str,
}

impl Embedder for StubEmbedder {
    fn embedder_ref(&self) -> &str {
        self.reference
    }
    fn embed(&self, text: &str) -> Result<Vec<i64>, String> {
        // A deterministic toy embedding — term counts over a fixed basis.
        let mut v = vec![0i64; 4];
        for (i, t) in ["alpha", "beta", "gamma", "delta"].iter().enumerate() {
            v[i] = text.matches(t).count() as i64;
        }
        Ok(v)
    }
}

fn write_ctx(store: &mut MemoryStore, scope: PersistenceScope, at: u64) -> WriteContext {
    let g = store.lease(scope);
    WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: at,
        run_id: "run1".to_string(),
    }
}

fn draft_text(text: &str, scope: PersistenceScope) -> MemoryDraft {
    MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            text,
            "owner",
            ProvenanceRecord::kernel("kernel:mem", 0),
        ))),
        contract: None,
        scope,
        declared_inputs: Vec::new(),
        justifications: Vec::new(),
        supersedes: None,
        validity: None,
        provenance: Some(ProvenanceRecord::minted(
            Origin::model("m1", "run1", "r1"),
            PersistenceScope::Run,
            0,
        )),
        semantic_id: None,
        validator_endorsed: false,
    }
}

fn sim_req(query: RetrievalQuery, at: u64) -> RetrievalRequest {
    RetrievalRequest {
        model_call_id: "mc1".to_string(),
        at: ("run1".to_string(), at),
        layers: [Layer::Episodic, Layer::Procedural, Layer::Session]
            .into_iter()
            .collect(),
        query,
        constraints: SlotConstraints {
            slot_min_authority: AuthorityClass::Unverified,
            validity_policy: ValidityPolicy {
                admitted_states: [
                    LifecycleStateKind::Valid,
                    LifecycleStateKind::StaleByDependency,
                    LifecycleStateKind::Unknown,
                ]
                .into_iter()
                .collect(),
                conflict_policy: ConflictPolicy::DeliverAllAnnotated,
                max_stale: None,
            },
            readers_required: None,
        },
        reader: "model".to_string(),
        budget: RetrievalBudget {
            tokens: 10_000,
            k: 100,
        },
        ranker: SIMILARITY_RERANK.to_string(),
        mode: ResolveMode::Execute,
    }
}

#[test]
fn ac_r_2_4_3_10_similarity_rerank_pinned_embedder_and_accounting() {
    // cosine_ppm is integer-exact — identical inputs, identical score
    // (the float-free canonical record).
    assert_eq!(retrieve::cosine_ppm(&[1, 0], &[1, 0]), 1_000_000);
    assert_eq!(retrieve::cosine_ppm(&[1, 0], &[0, 1]), 0);
    assert!(retrieve::cosine_ppm(&[1, 1], &[1, 0]) > 0);

    let mut store = MemoryStore::new("ms");
    let ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    store
        .put(draft_text("alpha alpha beta", PersistenceScope::Run), &ctx)
        .unwrap();
    let ctx2 = write_ctx(&mut store, PersistenceScope::Run, 2);
    store
        .put(draft_text("gamma delta", PersistenceScope::Run), &ctx2)
        .unwrap();

    let embedder = StubEmbedder {
        reference: "profile/embedder-pinned",
    };
    let q = |embedder_ref: &str| RetrievalQuery::Similarity {
        text: "alpha".to_string(),
        embedder: embedder_ref.to_string(),
    };
    // Unbound → `EmbedderUnpinned` (declared C2, never a silent default).
    let mut sink = CollectSink::default();
    let unbound = RetrievalIndexes {
        lexical: None,
        structural: None,
        embedder: None,
    };
    assert!(matches!(
        retrieve::retrieve_indexed(
            &mut store,
            &sim_req(q("profile/embedder-pinned"), 2),
            &mut sink,
            &unbound,
            || 1
        ),
        Err(RetrievalError::EmbedderUnpinned)
    ));
    // A *different* ref is still unbound — the pin is the snapshot, never a
    // loose name (ADR-0120).
    let wrong = RetrievalIndexes {
        lexical: None,
        structural: None,
        embedder: Some(&StubEmbedder {
            reference: "profile/other",
        }),
    };
    assert!(matches!(
        retrieve::retrieve_indexed(
            &mut store,
            &sim_req(q("profile/embedder-pinned"), 2),
            &mut sink,
            &wrong,
            || 1
        ),
        Err(RetrievalError::EmbedderUnpinned)
    ));
    // The pinned ref binds — the similarity arm serves, `embedder_calls`
    // counts the port calls, `deterministic` marks false.
    let bound = RetrievalIndexes {
        lexical: None,
        structural: None,
        embedder: Some(&embedder),
    };
    let mut sink = CollectSink::default();
    let (items, report) = retrieve::retrieve_indexed(
        &mut store,
        &sim_req(q("profile/embedder-pinned"), 2),
        &mut sink,
        &bound,
        || 1,
    )
    .unwrap();
    assert!(report.cost.embedder_calls >= 1);
    assert!(
        !report.deterministic,
        "similarity_rerank is the declared C2 arm"
    );
    assert_eq!(report.ranker_ref, SIMILARITY_RERANK);
    assert!(!items.is_empty(), "the alpha item hits");
}

// ── R-2.4.5 — `procedure_target` rule consumption ────────────────────────────

#[test]
fn r_2_4_5_procedure_target_selector_and_debt_gate() {
    let rule = |id: &str, selector: Json, debt: Option<AssumptionDebtRecord>| ProcedureTargetRule {
        rule_id: id.to_string(),
        selector,
        compile_hint: "workflow_node".to_string(),
        debt,
    };
    let rules = vec![
        rule(
            "rule/target-by-id",
            Json::obj([("procedures", Json::Arr(vec![Json::str("proc/deploy")]))]),
            Some(debt("rule/target-by-id")),
        ),
        rule(
            "rule/target-by-tag",
            Json::obj([("tags", Json::Arr(vec![Json::str("risky")]))]),
            Some(debt("rule/target-by-tag")),
        ),
    ];
    // Selector membership — by id.
    let hit = resolve_procedure_target(&rules, "proc/deploy", &[])
        .unwrap()
        .unwrap();
    assert_eq!(hit.0, "workflow_node");
    assert_eq!(hit.1, "rule/target-by-id");
    // … by tag.
    let hit = resolve_procedure_target(&rules, "proc/x", &["risky".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(hit.1, "rule/target-by-tag");
    // No selector match → `None` (the default rule stands).
    assert!(resolve_procedure_target(&rules, "proc/none", &[])
        .unwrap()
        .is_none());
    // A matching rule without a complete debt record is
    // `ConditionedRuleIncomplete` — the per-skill model binding refusal
    // (ADR-0086 d4), never a silent admit.
    let incomplete = vec![rule(
        "rule/naked",
        Json::obj([("procedures", Json::Arr(vec![Json::str("proc/deploy")]))]),
        None,
    )];
    assert!(matches!(
        resolve_procedure_target(&incomplete, "proc/deploy", &[]),
        Err(ProcedureTargetError::ConditionedRuleIncomplete { .. })
    ));
}
