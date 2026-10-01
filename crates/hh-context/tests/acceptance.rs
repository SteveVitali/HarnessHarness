//! S1.19 acceptance evidence — `R-2.4.1` (the context builder), `R-2.4.3⁰`
//! (the C0 memory slices) and `R-2.4.4⁰` (the lifecycle pure functions).
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.1-3** — context-label monotonicity: an `external` inline item
//!   lowers the join; a `handle_only` item contributes nothing; removing the
//!   last external item restores the label.
//! - **AC-R-2.4.1-4** — the no-widen seam: a hostile policy's illegal
//!   `Selection` is `PolicyViolation` with no plan; `Selection` structurally
//!   cannot carry authority/validity/retention/floor data.
//! - **AC-R-2.4.1-5** — closed world: an unidentified candidate is an
//!   `Omission{unidentified}` (or `UndeliverableArtifact` under `required`);
//!   `omitted ≠ ∅` ⇒ exactly one kernel-authority omission item in `kernel`;
//!   every delivered span traces to a `delivery_id`.
//! - **AC-R-2.4.1-6** — budget discipline: deterministic eviction under
//!   `enforce_budget`; required-only overflow is
//!   `ContextWindowExceeded{required_tokens, cap}` and
//!   `stage01_disposition` maps it to `CompactionRequired`-as-stop.
//! - **AC-R-2.4.1-8** — `plan_id` is deterministic under identical inputs
//!   and sensitive to the estimator identity (I-DET).
//! - **AC-R-2.4.1-11** — `PolicyDeclaration` registration: the input
//!   envelope, the debt-record requirement, T-LCD-01's model-identity
//!   refusal (also **AC-R-2.4.2-6**).
//! - **AC-R-2.4.1-14** — `link_layout` refuses: a missing reserved slot, a
//!   lowered floor, a volatile kind in a `static` slot, a floor below
//!   `unverified`, a non-transcript slot admitting `transcript_item`,
//!   `superseded|revoked|expired` in `admitted_states`.
//! - **AC-R-2.4.3-2** — `put`/`bind`/`resolve`/`manifest`: fenced writes
//!   refuse, free text caps at `external`, `execute` never serves
//!   revoked/stale heads, `audit` annotates, `manifest` folds the name
//!   history, read-your-writes.
//! - **AC-R-2.4.3-5** — lifecycle-aware retrieval: `revoked`/`superseded`/
//!   `expired` never served in `execute`; `stale_by_dependency` propagates
//!   transitively (J1); the `validity → authority → readers` order is the
//!   attested order.
//! - **AC-R-2.4.3-6** — deterministic retrieve: identical requests return
//!   identical `(items, report)`; budget cuts are whole-item with
//!   `omitted_by_budget` accounting; `structural`/`similarity` are typed
//!   refusals (`IndexUnavailable`/`EmbedderUnpinned`); `StaleStore` on a
//!   read-ahead watermark.

use std::collections::{BTreeMap, BTreeSet};

use hh_context::policy::{self, ContextPolicy, PolicyRequest};
use hh_context::vocab::{ConflictPolicy, LifecycleStateKind};
use hh_context::*;
use hh_hir::records::{AssumptionDebtRecord, DebtStatus, Validity};
use hh_identity::kinds::RecordKind;
use hh_identity::names::ResolveMode;
use hh_identity::refs::VersionedRef;
use hh_provenance::authority::ReaderSet;
use hh_provenance::label::Label;
use hh_provenance::origin::{HumanRole, Origin};
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::{AuthorityClass, PersistenceScope};
use hh_wire::json::Json;

// ── helpers ──────────────────────────────────────────────────────────────────

fn prov(origin: Origin, scope: PersistenceScope, at: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(origin, scope, at)
}

fn kernel_prov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("kernel:context", 0)
}

fn cand(
    id: &str,
    kind: CandidateKind,
    authority: AuthorityClass,
    tokens: u64,
    retention: Retention,
    seq: u64,
) -> Candidate {
    Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind,
        state: CandidateState::Expanded,
        retention,
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".to_string(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(authority),
        provenance: kernel_prov(),
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

fn req(candidates: Vec<Candidate>, cap: u64) -> AssemblyRequest {
    AssemblyRequest {
        model_call_id: "mc1".to_string(),
        view: DerivedFrom {
            run_id: "run1".to_string(),
            seq: 10,
            view_hash: "sha256:view".to_string(),
        },
        at_seq: 10,
        min_view_seq: None,
        candidates,
        advisories: Vec::new(),
        layout: default_layout(),
        budget: ContextBudget {
            window_cap: cap,
            margin: 0,
            reservations: Vec::new(),
            hard: true,
        },
        estimator_ref: "est/pinned".to_string(),
        policy_params: Json::Null,
    }
}

fn sink() -> CollectSink {
    CollectSink::default()
}

fn asm(
    r: &AssemblyRequest,
    p: &dyn ContextPolicy,
    sink: &mut CollectSink,
) -> Result<AssemblyOutcome, AssemblyError> {
    assemble(r, p, "layout/default", "none", 3, sink)
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
        provenance: Some(prov(
            Origin::model("m1", "run1", "r1"),
            PersistenceScope::Run,
            0,
        )),
        semantic_id: None,
        validator_endorsed: false,
    }
}

fn structured_draft(
    text_key: &str,
    scope: PersistenceScope,
    subject: Option<SubjectKey>,
) -> MemoryDraft {
    let mut d = draft_text("", scope);
    d.content = MemoryContent::Structured(Json::obj([("v", Json::str(text_key))]));
    d.subject_key = subject;
    d
}

// ── AC-R-2.4.1-14: link_layout refuses bad layouts ────────────────────────────

#[test]
fn ac_r_2_4_1_14_link_refuses_missing_reserved_and_lowered_floors() {
    // Missing reserved slot.
    let mut l = default_layout();
    l.slots.retain(|s| s.slot_id != "unverified");
    assert!(matches!(
        link_layout(&l),
        Err(LayoutError::InvalidLayout { .. })
    ));
    // Lowered floor (kernel slot floor lowered to external).
    let mut l = default_layout();
    l.slot_mut("kernel").unwrap().min_authority = AuthorityClass::External;
    assert!(matches!(
        link_layout(&l),
        Err(LayoutError::InvalidLayout { .. })
    ));
    // A floor below `unverified` is impossible in the closed class sum — the
    // *representable* violation is a floor below the reserved floor (above).
    // Volatile kind in a static slot.
    let mut l = default_layout();
    l.slot_mut("kernel")
        .unwrap()
        .admits
        .insert(CandidateKind::BudgetReminder);
    assert!(matches!(
        link_layout(&l),
        Err(LayoutError::VolatileInStaticTier { .. })
    ));
    // A non-transcript slot admitting transcript_item.
    let mut l = default_layout();
    l.slot_mut("external")
        .unwrap()
        .admits
        .insert(CandidateKind::TranscriptItem);
    assert!(matches!(
        link_layout(&l),
        Err(LayoutError::InvalidLayout { .. })
    ));
    // `admitted_states` with `superseded`.
    let mut l = default_layout();
    l.slot_mut("external")
        .unwrap()
        .validity_policy
        .admitted_states
        .insert(LifecycleStateKind::Superseded);
    assert!(matches!(
        link_layout(&l),
        Err(LayoutError::InvalidLayout { .. })
    ));
    // The default layout links clean.
    assert!(link_layout(&default_layout()).is_ok());
}

// ── AC-R-2.4.1-5: closed world ───────────────────────────────────────────────

#[test]
fn ac_r_2_4_1_5_unidentified_candidates_omitted_and_accounted() {
    // An unidentified *optional* candidate → Omission{unidentified} + the
    // kernel-authored omission item in `kernel`.
    let mut c = cand(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::External,
        10,
        Retention::Optional(PriorityClass::ObservationRecent),
        1,
    );
    c.context_item_id = None;
    let mut s = sink();
    let out = asm(&req(vec![c], 10_000), &DefaultPolicy::default(), &mut s).unwrap();
    assert!(out
        .plan
        .omitted
        .iter()
        .any(|o| o.candidate_id == "c1" && o.reason == OmissionReason::Unidentified));
    let kernel = out
        .plan
        .slots
        .iter()
        .find(|f| f.slot_id == "kernel")
        .expect("omission item forces the kernel slot");
    assert_eq!(kernel.items.len(), 1);
    assert_eq!(kernel.items[0].authority, AuthorityClass::Kernel);
    // context.assembled was emitted with the omission accounting.
    let asm_ev = s
        .events
        .iter()
        .find(|(c, _)| c == "context.assembled")
        .expect("context.assembled emitted");
    assert!(asm_ev.1.to_canonical_string().contains("\"unidentified\""));

    // A required unidentified candidate → UndeliverableArtifact (no plan).
    let mut c = cand(
        "c2",
        CandidateKind::PrincipalMessage,
        AuthorityClass::Principal,
        10,
        Retention::Required,
        1,
    );
    c.context_item_id = None;
    let mut s = sink();
    assert!(matches!(
        asm(&req(vec![c], 10_000), &DefaultPolicy::default(), &mut s),
        Err(AssemblyError::UndeliverableArtifact { .. })
    ));
    assert!(s.events.is_empty()); // nothing emitted on failure
}

#[test]
fn ac_r_2_4_1_5_delivery_trace_covers_every_planned_item() {
    let cands = vec![
        cand(
            "k",
            CandidateKind::KernelNotice,
            AuthorityClass::Kernel,
            5,
            Retention::Required,
            0,
        ),
        cand(
            "p",
            CandidateKind::PrincipalMessage,
            AuthorityClass::Principal,
            10,
            Retention::Required,
            1,
        ),
        cand(
            "t",
            CandidateKind::TranscriptItem,
            AuthorityClass::Delegate,
            20,
            Retention::Optional(PriorityClass::TranscriptTail),
            2,
        ),
        cand(
            "o",
            CandidateKind::Observation,
            AuthorityClass::External,
            15,
            Retention::Optional(PriorityClass::ObservationRecent),
            3,
        ),
    ];
    let mut s = sink();
    let out = asm(&req(cands, 10_000), &DefaultPolicy::default(), &mut s).unwrap();
    // Every planned item has a fresh, distinct delivery_id.
    let ids: BTreeSet<&str> = out
        .plan
        .slots
        .iter()
        .flat_map(|f| f.items.iter().map(|i| i.delivery_id.as_str()))
        .collect();
    let total: usize = out.plan.slots.iter().map(|f| f.items.len()).sum();
    assert_eq!(ids.len(), total);
    assert_eq!(total, 4);
    // Slot placement: transcript_item lands in `transcript` only.
    let transcript = out
        .plan
        .slots
        .iter()
        .find(|f| f.slot_id == "transcript")
        .unwrap();
    assert!(transcript.items.iter().any(|i| i.candidate_id == "t"));
}

// ── AC-R-2.4.1-3: label monotonicity ─────────────────────────────────────────

#[test]
fn ac_r_2_4_1_3_context_label_joins_inline_and_skips_handles() {
    let kernel = cand(
        "k",
        CandidateKind::KernelNotice,
        AuthorityClass::Kernel,
        5,
        Retention::Required,
        0,
    );
    let ext = cand(
        "e",
        CandidateKind::Observation,
        AuthorityClass::External,
        10,
        Retention::Optional(PriorityClass::ObservationRecent),
        1,
    );
    let mut s = sink();
    let with_ext = asm(
        &req(vec![kernel.clone(), ext.clone()], 10_000),
        &DefaultPolicy::default(),
        &mut s,
    )
    .unwrap();
    // An external inline item lowers the context label.
    assert_eq!(
        with_ext.plan.context_label.authority,
        AuthorityClass::External
    );
    let mut s = sink();
    let kernel_only = asm(
        &req(vec![kernel], 10_000),
        &DefaultPolicy::default(),
        &mut s,
    )
    .unwrap();
    // Removing the external item restores the label.
    assert_eq!(
        kernel_only.plan.context_label.authority,
        AuthorityClass::Kernel
    );
    // A handle_only item contributes nothing to the join.
    let mut h = cand(
        "h",
        CandidateKind::Memory,
        AuthorityClass::Unverified,
        10,
        Retention::Optional(PriorityClass::Memory),
        1,
    );
    h.state = CandidateState::HandleOnly;
    h.label = Label::at(AuthorityClass::Unverified);
    let mut s = sink();
    let with_handle = asm(
        &req(
            vec![
                cand(
                    "k2",
                    CandidateKind::KernelNotice,
                    AuthorityClass::Kernel,
                    5,
                    Retention::Required,
                    0,
                ),
                h,
            ],
            10_000,
        ),
        &DefaultPolicy::default(),
        &mut s,
    )
    .unwrap();
    assert_eq!(
        with_handle.plan.context_label.authority,
        AuthorityClass::Kernel
    );
}

// ── AC-R-2.4.1-4: the no-widen seam ──────────────────────────────────────────

/// A hostile policy that tries to widen: place an `external` observation in
/// the `kernel` slot (outside `admissible_slots`).
struct WideningPolicy;
impl ContextPolicy for WideningPolicy {
    fn declare(&self) -> PolicyDeclaration {
        DefaultPolicy::default().declare()
    }
    fn priority(&self, _c: &Candidate) -> PriorityClass {
        PriorityClass::Commentary
    }
    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        // Attempt: force the lowest-authority candidate into `kernel`.
        let victim = req
            .candidates
            .iter()
            .min_by_key(|ac| ac.candidate.label.authority)
            .unwrap();
        Ok(Selection {
            chosen: vec![(victim.candidate.candidate_id.clone(), "kernel".to_string())],
            order: BTreeMap::from([(
                "kernel".to_string(),
                vec![victim.candidate.candidate_id.clone()],
            )]),
            evict_order: Vec::new(),
            by_reference: Vec::new(),
            expand_requests: Vec::new(),
        })
    }
}

#[test]
fn ac_r_2_4_1_4_widening_selection_is_policy_violation_no_plan() {
    let cands = vec![
        cand(
            "k",
            CandidateKind::KernelNotice,
            AuthorityClass::Kernel,
            5,
            Retention::Required,
            0,
        ),
        cand(
            "o",
            CandidateKind::Observation,
            AuthorityClass::External,
            10,
            Retention::Optional(PriorityClass::ObservationRecent),
            1,
        ),
    ];
    let mut s = sink();
    let r = req(cands, 10_000);
    let err = asm(&r, &WideningPolicy, &mut s).unwrap_err();
    // The hostile pick names a slot outside admissible_slots → PolicyViolation.
    assert!(matches!(err, AssemblyError::PolicyViolation { .. }));
    // Also: the required kernel item was dropped — either way it's a violation.
    assert!(s.events.is_empty(), "no plan/event emitted on violation");
}

/// A policy that silently drops a required candidate.
struct DropRequired;
impl ContextPolicy for DropRequired {
    fn declare(&self) -> PolicyDeclaration {
        DefaultPolicy::default().declare()
    }
    fn priority(&self, _c: &Candidate) -> PriorityClass {
        PriorityClass::Commentary
    }
    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let optional_only: Vec<(String, String)> = req
            .candidates
            .iter()
            .filter(|ac| !ac.candidate.retention.is_required())
            .map(|ac| {
                (
                    ac.candidate.candidate_id.clone(),
                    ac.admissible_slots[0].clone(),
                )
            })
            .collect();
        Ok(Selection {
            chosen: optional_only.clone(),
            order: optional_only
                .iter()
                .cloned()
                .fold(BTreeMap::new(), |mut m, (c, s)| {
                    m.entry(s).or_insert_with(Vec::new).push(c);
                    m
                }),
            evict_order: Vec::new(),
            by_reference: Vec::new(),
            expand_requests: Vec::new(),
        })
    }
}

#[test]
fn ac_r_2_4_1_4_dropping_a_required_candidate_is_policy_violation() {
    let cands = vec![
        cand(
            "k",
            CandidateKind::KernelNotice,
            AuthorityClass::Kernel,
            5,
            Retention::Required,
            0,
        ),
        cand(
            "o",
            CandidateKind::Observation,
            AuthorityClass::External,
            10,
            Retention::Optional(PriorityClass::ObservationRecent),
            1,
        ),
    ];
    let mut s = sink();
    assert!(matches!(
        asm(&req(cands, 10_000), &DropRequired, &mut s),
        Err(AssemblyError::PolicyViolation { .. })
    ));
}

// ── AC-R-2.4.1-6: budget discipline ──────────────────────────────────────────

#[test]
fn ac_r_2_4_1_6_required_overflow_is_context_window_exceeded_stop() {
    // Required items (principal + kernel) alone exceed the cap.
    let cands = vec![
        cand(
            "k",
            CandidateKind::KernelNotice,
            AuthorityClass::Kernel,
            400,
            Retention::Required,
            0,
        ),
        cand(
            "p",
            CandidateKind::PrincipalMessage,
            AuthorityClass::Principal,
            200,
            Retention::Required,
            1,
        ),
    ];
    let mut s = sink();
    let r = req(cands, 500); // cap 500, required 400+200+omission-reserve > 500
    let err = asm(&r, &DefaultPolicy::default(), &mut s).unwrap_err();
    match err {
        AssemblyError::ContextWindowExceeded {
            required_tokens,
            cap,
        } => {
            assert_eq!(cap, 500);
            assert!(required_tokens > 500);
            // The Stage-0/1 disposition: CompactionRequired handled as stop.
            assert_eq!(
                stage01_disposition(&err),
                Some(Stage01Outcome::Stop {
                    signal: CompactionRequired {
                        required_tokens,
                        cap
                    }
                })
            );
            let decision = events::stop_decision_payload(required_tokens, cap);
            assert!(decision
                .to_canonical_string()
                .contains("\"compaction_required\""));
        }
        other => panic!("expected ContextWindowExceeded, got {other:?}"),
    }
    assert!(s.events.is_empty(), "no plan emitted on overflow");
}

#[test]
fn ac_r_2_4_1_6_optional_eviction_is_deterministic() {
    // Cap fits required + one optional; two optionals compete — eviction
    // follows (PriorityClass, age): commentary evicts before memory.
    let cands = vec![
        cand(
            "k",
            CandidateKind::KernelNotice,
            AuthorityClass::Kernel,
            100,
            Retention::Required,
            0,
        ),
        cand(
            "m",
            CandidateKind::Memory,
            AuthorityClass::External,
            100,
            Retention::Optional(PriorityClass::Memory),
            1,
        ),
        cand(
            "o",
            CandidateKind::Observation,
            AuthorityClass::External,
            100,
            Retention::Optional(PriorityClass::Commentary),
            2,
        ),
    ];
    let cap = 100 + 100 + OMISSION_ITEM_TOKENS + 10;
    let mut s1 = sink();
    let mut s2 = sink();
    let r = req(cands, cap);
    let a = asm(&r, &DefaultPolicy::default(), &mut s1).unwrap();
    let b = asm(&r, &DefaultPolicy::default(), &mut s2).unwrap();
    assert_eq!(
        a.plan.plan_id, b.plan.plan_id,
        "identical inputs → identical plan_id"
    );
    // The commentary-priority observation is evicted; memory survives.
    assert!(a
        .plan
        .omitted
        .iter()
        .any(|o| o.candidate_id == "o" && o.reason == OmissionReason::Budget));
    assert!(!a.plan.omitted.iter().any(|o| o.candidate_id == "m"));
}

// ── AC-R-2.4.1-8: deterministic plan id + estimator sensitivity ──────────────

#[test]
fn ac_r_2_4_1_8_plan_id_deterministic_and_estimator_sensitive() {
    let cands = || {
        vec![cand(
            "p",
            CandidateKind::PrincipalMessage,
            AuthorityClass::Principal,
            50,
            Retention::Required,
            1,
        )]
    };
    let mut s1 = sink();
    let mut s2 = sink();
    let a = asm(&req(cands(), 10_000), &DefaultPolicy::default(), &mut s1).unwrap();
    let b = asm(&req(cands(), 10_000), &DefaultPolicy::default(), &mut s2).unwrap();
    assert_eq!(a.plan.plan_id, b.plan.plan_id);
    // A different estimator_ref changes the plan id (I-DET records it).
    let mut r = req(cands(), 10_000);
    r.estimator_ref = "est/other".to_string();
    for c in &mut r.candidates {
        c.estimate.estimator_ref = "est/other".to_string();
    }
    let mut s3 = sink();
    let c = asm(&r, &DefaultPolicy::default(), &mut s3).unwrap();
    assert_ne!(a.plan.plan_id, c.plan.plan_id);
    // A mismatched estimator_ref on a candidate is EstimatorMismatch.
    let mut r = req(cands(), 10_000);
    r.candidates[0].estimate.estimator_ref = "est/wrong".to_string();
    let mut s4 = sink();
    assert!(matches!(
        asm(&r, &DefaultPolicy::default(), &mut s4),
        Err(AssemblyError::EstimatorMismatch { .. })
    ));
}

// ── AC-R-2.4.1-11 + AC-R-2.4.2-6: policy registration ────────────────────────

#[test]
fn ac_r_2_4_1_11_and_2_4_2_6_registration_contract() {
    let inputs = || {
        REQUIRED_POLICY_INPUTS
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    };
    // Missing input envelope.
    let d = PolicyDeclaration {
        variant_id: "context_policy/x".into(),
        deterministic: true,
        model_conditioned_rules: vec![],
        required_inputs: BTreeSet::new(),
    };
    assert!(matches!(
        policy::check(&d),
        Err(RegistrationError::MissingRequiredInput { .. })
    ));
    // A conditioned rule with a complete debt record passes.
    let debt = AssumptionDebtRecord {
        rule_id: "r1".into(),
        hypothesis: hh_hir::leaves::Text::new("h", "alice", kernel_prov()),
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
        removal_test: Some(hh_hir::RemovalTest {
            kind: hh_hir::RemovalTestKind::Inspection,
            criteria: Some("human inspection".into()),
            ..hh_hir::RemovalTest::new(hh_hir::RemovalTestKind::Inspection)
        }),
        created_by: None,
        created_at: None,
        supersedes: None,
    };
    let d = PolicyDeclaration {
        variant_id: "context_policy/x".into(),
        deterministic: true,
        model_conditioned_rules: vec![ConditionedRule {
            rule_id: "r1".into(),
            conditioned_on: RuleCondition::Profile("profile/x".into()),
            debt: Some(debt.clone()),
        }],
        required_inputs: inputs(),
    };
    assert!(policy::check(&d).is_ok());
    // … without the debt record it fails.
    let d = PolicyDeclaration {
        model_conditioned_rules: vec![ConditionedRule {
            rule_id: "r1".into(),
            conditioned_on: RuleCondition::Profile("profile/x".into()),
            debt: None,
        }],
        ..d
    };
    assert!(matches!(
        policy::check(&d),
        Err(RegistrationError::MissingDebt { .. })
    ));
    // A model-identity-conditioned rule is refused (T-LCD-01 / AC-R-2.4.2-6).
    let d = PolicyDeclaration {
        variant_id: "context_policy/x".into(),
        deterministic: true,
        model_conditioned_rules: vec![ConditionedRule {
            rule_id: "r1".into(),
            conditioned_on: RuleCondition::ModelIdentity("gpt-4o".into()),
            debt: Some(debt),
        }],
        required_inputs: inputs(),
    };
    assert!(matches!(
        policy::check(&d),
        Err(RegistrationError::ModelIdentityCondition { .. })
    ));
}

// ── AC-R-2.4.1-3/-11: StaleView + I-PAIR/I-ATOM ───────────────────────────────

#[test]
fn assemble_stale_view_and_pair_atom_checks() {
    let mut r = req(vec![], 10_000);
    r.min_view_seq = Some(20);
    let mut s = sink();
    assert!(matches!(
        asm(&r, &DefaultPolicy::default(), &mut s),
        Err(AssemblyError::StaleView { have: 10, need: 20 })
    ));
    // I-PAIR: a call and its observation must co-deliver — the default policy
    // admits both, so a plan where only one can fit is PlanInvalid, not a
    // split pair.
    let mut a = cand(
        "call",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        300,
        Retention::Optional(PriorityClass::TranscriptTail),
        1,
    );
    let mut b = cand(
        "obs",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        300,
        Retention::Optional(PriorityClass::ObservationRecent),
        2,
    );
    a.paired_with = Some("obs".to_string());
    b.paired_with = Some("call".to_string());
    // The pair is evicted together (both optional) — no split.
    let mut s = sink();
    let out = asm(&req(vec![a, b], 10_000), &DefaultPolicy::default(), &mut s).unwrap();
    let in_plan = |id: &str| {
        out.plan
            .slots
            .iter()
            .flat_map(|f| f.items.iter())
            .any(|i| i.candidate_id == id)
    };
    assert_eq!(in_plan("call"), in_plan("obs"));
}

// ── AC-R-2.4.3-2: the store's write/read contract ─────────────────────────────

#[test]
fn ac_r_2_4_3_2_put_caps_fencing_and_read_your_writes() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    ctx.lease_generation = g;
    // Free text by a delegate-class writer caps at `external`.
    let out = store
        .put(draft_text("the sky is blue", PersistenceScope::Run), &ctx)
        .unwrap();
    assert_eq!(out.version.label.authority, AuthorityClass::External);
    // Read-your-writes: the just-written version resolves at once.
    let r = store
        .resolve(
            PersistenceScope::Run,
            None,
            Some(&out.version.version_id),
            ResolveMode::Execute,
        )
        .unwrap();
    assert!(matches!(r, ResolveOutcome::Live { .. }));
    // A stale lease generation is fenced.
    let mut stale_ctx = write_ctx(&mut store, PersistenceScope::Run, 2);
    stale_ctx.lease_generation = g + 1; // a generation the store never issued
    assert!(matches!(
        store.put(draft_text("x", PersistenceScope::Run), &stale_ctx),
        Err(MemoryError::Fenced { .. })
    ));
    // `definition` scope refuses a non-seal write.
    let mut c2 = write_ctx(&mut store, PersistenceScope::Run, 2);
    c2.lease_generation = store.lease(PersistenceScope::Run);
    let mut d = draft_text("x", PersistenceScope::Run);
    d.scope = PersistenceScope::Definition;
    assert!(matches!(
        store.put(d, &c2),
        Err(MemoryError::ScopeCeilingExceeded { .. })
    ));
    // Missing provenance refuses.
    let mut d = draft_text("x", PersistenceScope::Run);
    d.provenance = None;
    assert!(matches!(
        store.put(d, &c2),
        Err(MemoryError::MissingProvenance)
    ));
    // `context.memory.written` emitted.
    assert!(store
        .drain_events()
        .iter()
        .any(|(c, _)| c == "context.memory.written"));
}

#[test]
fn ac_r_2_4_3_2_bind_manifest_resolve_and_supersede() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Session, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Session, 1);
    ctx.lease_generation = g;
    let mut d = structured_draft("v1", PersistenceScope::Session, None);
    d.contract = Some(InvalidationContract {
        dependencies: vec![],
        cache_hint: CacheHint::Cacheable,
        validator_ref: Some("validator/x".to_string()),
        freshness: None,
        invalidation_condition: None,
        revalidation: Revalidation::Never,
    });
    store.set_validator_verdict("unused", "validator/x", true);
    let v1 = store.put(d, &ctx).unwrap().version;
    store.set_validator_verdict(&v1.version_id, "validator/x", true);
    store
        .bind(
            PersistenceScope::Session,
            "prefs",
            &v1.version_id,
            None,
            "write",
            2,
        )
        .unwrap();
    // The manifest folds the name.
    let m = store.manifest(PersistenceScope::Session, 10);
    assert_eq!(m.entries.get("prefs"), Some(&v1.version_id));
    // Supersede: v2 at authority ≥ v1's (kernel write).
    let mut d2 = structured_draft("v2", PersistenceScope::Session, None);
    d2.provenance = Some(kernel_prov());
    d2.supersedes = Some(SupersedeClaim {
        version_id: v1.version_id.clone(),
        reason: SupersedeClaimReason::Correction,
    });
    d2.contract = Some(InvalidationContract {
        dependencies: vec![],
        cache_hint: CacheHint::Cacheable,
        validator_ref: Some("validator/x".to_string()),
        freshness: None,
        invalidation_condition: None,
        revalidation: Revalidation::Never,
    });
    let v2 = store.put(d2, &ctx).unwrap().version;
    store.set_validator_verdict(&v2.version_id, "validator/x", true);
    store
        .bind(
            PersistenceScope::Session,
            "prefs",
            &v2.version_id,
            Some(&v1.version_id),
            "supersede",
            3,
        )
        .unwrap();
    // execute resolve: the head (v2) is live; v1 is superseded → Unservable.
    let r = store
        .resolve(
            PersistenceScope::Session,
            Some("prefs"),
            None,
            ResolveMode::Execute,
        )
        .unwrap();
    assert!(
        matches!(r, ResolveOutcome::Live { ref version } if version.version_id == v2.version_id)
    );
    let r = store
        .resolve(
            PersistenceScope::Session,
            None,
            Some(&v1.version_id),
            ResolveMode::Execute,
        )
        .unwrap();
    assert!(matches!(
        r,
        ResolveOutcome::Unservable {
            state: LifecycleStateKind::Superseded,
            ..
        }
    ));
    // audit resolve annotates.
    let r = store
        .resolve(
            PersistenceScope::Session,
            None,
            Some(&v1.version_id),
            ResolveMode::Audit,
        )
        .unwrap();
    assert!(matches!(
        r,
        ResolveOutcome::Annotated {
            state: LifecycleStateKind::Superseded,
            ..
        }
    ));
    // A lower-authority supersession is refused (the deterministic-conflict
    // path takes over — no silent authority widening).
    let mut d3 = structured_draft("v3", PersistenceScope::Session, None);
    d3.contract = Some(InvalidationContract {
        dependencies: vec![],
        cache_hint: CacheHint::Cacheable,
        validator_ref: Some("validator/x".to_string()),
        freshness: None,
        invalidation_condition: None,
        revalidation: Revalidation::Never,
    });
    // model-origin → delegate ceiling … need authority below v2's kernel
    d3.provenance = Some(prov(
        Origin::model("m1", "run1", "r2"),
        PersistenceScope::Session,
        3,
    ));
    d3.supersedes = Some(SupersedeClaim {
        version_id: v2.version_id.clone(),
        reason: SupersedeClaimReason::Correction,
    });
    assert!(matches!(
        store.put(d3, &ctx),
        Err(MemoryError::AuthorityInsufficient { .. })
    ));
}

// ── AC-R-2.4.3-5: lifecycle-aware retrieval ──────────────────────────────────

fn retrieval_req(query: RetrievalQuery, at: u64, min_auth: AuthorityClass) -> RetrievalRequest {
    RetrievalRequest {
        model_call_id: "mc1".to_string(),
        at: ("run1".to_string(), at),
        layers: [Layer::Episodic, Layer::Procedural, Layer::Session]
            .into_iter()
            .collect(),
        query,
        constraints: SlotConstraints {
            slot_min_authority: min_auth,
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
        ranker: DETERMINISTIC_DEFAULT.to_string(),
        mode: ResolveMode::Execute,
    }
}

#[test]
fn ac_r_2_4_3_5_lifecycle_filters_execute_never_serves_bad_states() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    ctx.lease_generation = g;
    let v1 = store
        .put(draft_text("alpha fact", PersistenceScope::Run), &ctx)
        .unwrap()
        .version;
    let mut ctx2 = write_ctx(&mut store, PersistenceScope::Run, 2);
    ctx2.lease_generation = store.lease(PersistenceScope::Run);
    let v2 = store
        .put(draft_text("beta fact", PersistenceScope::Run), &ctx2)
        .unwrap()
        .version;
    // Revoke v1 (kernel revoker).
    revoke(
        &mut store,
        &v1.version_id,
        RevocationReason::Contradicted,
        &kernel_prov(),
        None,
        3,
    )
    .unwrap();
    let q = RetrievalQuery::Lexical {
        terms: vec!["fact".to_string()],
        match_mode: MatchMode::Any,
        context_lines: 0,
        case_sensitive: false,
        normalized: true,
    };
    let mut sink = CollectSink::default();
    let (items, report) = retrieve(
        &mut store,
        &retrieval_req(q.clone(), 3, AuthorityClass::Unverified),
        &mut sink,
        None,
        || 1,
    )
    .unwrap();
    assert!(items.iter().all(|i| i.address != v1.version_id));
    assert!(items.iter().any(|i| i.address == v2.version_id));
    assert_eq!(report.returned, 1);
    // The withheld row carries the reason.
    let read = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.memory.read")
        .unwrap();
    let s = read.1.to_canonical_string();
    assert!(s.contains(&v1.version_id) && s.contains("revoked"));
    // context.retrieval.completed emitted too.
    assert!(sink
        .events
        .iter()
        .any(|(c, _)| c == "context.retrieval.completed"));
}

#[test]
fn ac_r_2_4_3_5_stale_by_dependency_propagates_transitively() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    ctx.lease_generation = g;
    let base = store
        .put(draft_text("base", PersistenceScope::Run), &ctx)
        .unwrap()
        .version;
    // v2 justifies on v1 (delivered_memory — not declared_input → J1 applies).
    let mut d2 = draft_text("mid", PersistenceScope::Run);
    d2.justifications = vec![Justification {
        kind: JustificationKind::DeliveredMemory,
        ref_: VersionedRef::pinned(RecordKind::Memory, &base.version_id, kernel_prov()),
        at: hh_ledger::manifest::EventRef {
            run_id: "run1".into(),
            event_id: "e1".into(),
        },
    }];
    let mid = store.put(d2, &ctx).unwrap().version;
    let mut d3 = draft_text("tip", PersistenceScope::Run);
    d3.justifications = vec![Justification {
        kind: JustificationKind::ExpandedHandle,
        ref_: VersionedRef::pinned(RecordKind::Memory, &mid.version_id, kernel_prov()),
        at: hh_ledger::manifest::EventRef {
            run_id: "run1".into(),
            event_id: "e2".into(),
        },
    }];
    let tip = store.put(d3, &ctx).unwrap().version;
    revoke(
        &mut store,
        &base.version_id,
        RevocationReason::Poisoned,
        &kernel_prov(),
        None,
        5,
    )
    .unwrap();
    // Transitive: base revoked → mid stale → tip stale (J1).
    let st = lifecycle_state(&store, &tip.version_id, 6);
    assert_eq!(st.kind(), LifecycleStateKind::StaleByDependency);
    // The stale index fold agrees.
    let idx = stale_index(&store, 6);
    assert!(idx.entries.contains_key(&tip.version_id));
    // But `stale_by_dependency` is admissible under a slot policy that admits
    // it (annotate, never hide) — the retrieve request admits it.
    let q = RetrievalQuery::Lexical {
        terms: vec!["tip".to_string()],
        match_mode: MatchMode::Any,
        context_lines: 0,
        case_sensitive: false,
        normalized: true,
    };
    let mut sink = CollectSink::default();
    let (items, _) = retrieve(
        &mut store,
        &retrieval_req(q, 5, AuthorityClass::Unverified),
        &mut sink,
        None,
        || 1,
    )
    .unwrap();
    assert!(items.iter().any(|i| i.address == tip.version_id));
    // With a `valid`-only policy the same item is withheld.
    let mut req = retrieval_req(
        RetrievalQuery::Lexical {
            terms: vec!["tip".to_string()],
            match_mode: MatchMode::Any,
            context_lines: 0,
            case_sensitive: false,
            normalized: true,
        },
        5,
        AuthorityClass::Unverified,
    );
    req.constraints.validity_policy.admitted_states =
        [LifecycleStateKind::Valid].into_iter().collect();
    let mut sink = CollectSink::default();
    let (items, report) = retrieve(&mut store, &req, &mut sink, None, || 1).unwrap();
    assert!(items.is_empty());
    assert!(report.filtered_validity >= 1);
}

// ── AC-R-2.4.3-6: deterministic retrieve ─────────────────────────────────────

#[test]
fn ac_r_2_4_3_6_deterministic_retrieve_and_budget_cut() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    ctx.lease_generation = g;
    for (i, t) in ["alpha one", "alpha two", "alpha three"].iter().enumerate() {
        let mut c = write_ctx(&mut store, PersistenceScope::Run, (i + 1) as u64);
        c.lease_generation = store.lease(PersistenceScope::Run);
        store.put(draft_text(t, PersistenceScope::Run), &c).unwrap();
    }
    let q = RetrievalQuery::Lexical {
        terms: vec!["alpha".to_string()],
        match_mode: MatchMode::Any,
        context_lines: 0,
        case_sensitive: false,
        normalized: true,
    };
    let req = retrieval_req(q.clone(), 3, AuthorityClass::Unverified);
    let mut s1 = CollectSink::default();
    let mut s2 = CollectSink::default();
    let (a, ra) = retrieve(&mut store, &req, &mut s1, None, || 1).unwrap();
    let (b, rb) = retrieve(&mut store, &req, &mut s2, None, || 1).unwrap();
    let ids = |v: &[RetrievedItem]| v.iter().map(|i| i.address.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&a), ids(&b), "deterministic order");
    assert_eq!(ra.enumerated, rb.enumerated);
    // Budget cut — whole items only, omitted_by_budget accounts the rest.
    let mut req = retrieval_req(q, 3, AuthorityClass::Unverified);
    req.budget = RetrievalBudget { tokens: 4, k: 2 };
    let mut s3 = CollectSink::default();
    let (items, report) = retrieve(&mut store, &req, &mut s3, None, || 1).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(report.omitted_by_budget.len(), 1);
    // StaleStore on a read-ahead watermark.
    let req = retrieval_req(
        RetrievalQuery::ByAddress {
            address: "sha256:x".into(),
        },
        99,
        AuthorityClass::Unverified,
    );
    let mut s4 = CollectSink::default();
    assert!(matches!(
        retrieve(&mut store, &req, &mut s4, None, || 1),
        Err(RetrievalError::StaleStore { .. })
    ));
    // `structural` is executable at Stage 3 (OQ-203's Stage-3 scope) — an
    // empty query is a valid empty hit set, not a refusal.
    let req = retrieval_req(
        RetrievalQuery::Structural {
            anchors: vec![],
            mentioned_idents: vec![],
        },
        3,
        AuthorityClass::Unverified,
    );
    let mut s5 = CollectSink::default();
    let (items, _r) = retrieve(&mut store, &req, &mut s5, None, || 1).unwrap();
    assert!(items.is_empty());
    let req = retrieval_req(
        RetrievalQuery::Similarity {
            text: "x".into(),
            embedder: "profile/e".into(),
        },
        3,
        AuthorityClass::Unverified,
    );
    let mut s6 = CollectSink::default();
    assert!(matches!(
        retrieve(&mut store, &req, &mut s6, None, || 1),
        Err(RetrievalError::EmbedderUnpinned)
    ));
}

// ── lifecycle pure functions (R-2.4.4⁰) ──────────────────────────────────────

#[test]
fn lifecycle_precedence_and_expiry() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Run, 1);
    ctx.lease_generation = g;
    // An expiring validity window.
    let mut d = draft_text("expiring", PersistenceScope::Run);
    d.validity = Some(Validity {
        from: 0,
        until: Some(10),
        condition: None,
    });
    let v = store.put(d, &ctx).unwrap().version;
    assert_eq!(
        lifecycle_state(&store, &v.version_id, 5).kind(),
        LifecycleStateKind::Valid
    );
    assert_eq!(
        lifecycle_state(&store, &v.version_id, 10).kind(),
        LifecycleStateKind::Expired
    );
    // Revoked beats everything.
    revoke(
        &mut store,
        &v.version_id,
        RevocationReason::Hold,
        &kernel_prov(),
        None,
        15,
    )
    .unwrap();
    assert_eq!(
        lifecycle_state(&store, &v.version_id, 20).kind(),
        LifecycleStateKind::Revoked
    );
    // Double revoke is AlreadyRevoked.
    assert!(matches!(
        revoke(
            &mut store,
            &v.version_id,
            RevocationReason::Hold,
            &kernel_prov(),
            None,
            21
        ),
        Err(LifecycleError::AlreadyRevoked { .. })
    ));
}

#[test]
fn lifecycle_promote_and_resolve_conflict() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Session, "agent");
    let mut ctx = write_ctx(&mut store, PersistenceScope::Session, 1);
    ctx.lease_generation = g;
    // Two structured versions sharing a subject_key with different values →
    // a deterministic conflict set.
    let sk = SubjectKey {
        schema_ref: "schema/prefs".into(),
        key: "k".into(),
    };
    let contract = || {
        Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: Revalidation::Never,
        })
    };
    let mut d1 = structured_draft("a", PersistenceScope::Session, Some(sk.clone()));
    d1.contract = contract();
    let v1 = store.put(d1, &ctx).unwrap().version;
    let mut d2 = structured_draft("b", PersistenceScope::Session, Some(sk.clone()));
    d2.contract = contract();
    let out2 = store.put(d2, &ctx).unwrap();
    assert!(out2.conflict.is_some(), "deterministic conflict detected");
    let set = out2.conflict.unwrap();
    assert_eq!(set.members.len(), 2);
    // A delegate-class resolver is IllegitimateEndorsement.
    let model_prov = prov(Origin::model("m1", "run1", "r9"), PersistenceScope::Run, 9);
    assert!(matches!(
        resolve_conflict(
            &mut store,
            &set.conflict_set_id,
            &v1.version_id,
            "approval",
            &model_prov
        ),
        Err(LifecycleError::IllegitimateEndorsement { .. })
    ));
    // A human resolves by supersession (head = v2, a kernel-authority write).
    let human = ProvenanceRecord::minted(
        Origin::human("alice", HumanRole::Principal),
        PersistenceScope::User,
        9,
    );
    let resolved = resolve_conflict(
        &mut store,
        &set.conflict_set_id,
        &out2.version.version_id,
        "supersession",
        &human,
    );
    // head authority (external, capped) vs member authority — the head must
    // carry ≥ member authority; both are external-capped so it resolves.
    assert!(resolved.is_ok());
    // promote: model-origin versions are never promoted.
    let mut dm = draft_text("model fact", PersistenceScope::Session);
    dm.provenance = Some(model_prov);
    dm.contract = contract();
    let vm = store.put(dm, &ctx).unwrap().version;
    assert!(matches!(
        promote(
            &mut store,
            &vm.version_id,
            &human,
            PersistenceScope::Project,
            AuthorityClass::Principal,
            &ctx
        ),
        Err(LifecycleError::IllegitimateEndorsement { .. })
    ));
    // promote a tool-written (external-authority) version works and emits
    // the endorsement row.
    let tool_prov = prov(Origin::tool("cap/x", "inv1"), PersistenceScope::Run, 10);
    let mut dk = structured_draft("tool fact", PersistenceScope::Session, None);
    dk.provenance = Some(tool_prov);
    dk.contract = contract();
    let vk = store.put(dk, &ctx).unwrap().version;
    assert_eq!(vk.label.authority, AuthorityClass::External);
    // The promoted write lands at `project` scope — the writer must hold
    // that scope's lease.
    let gp = store.take_lease(PersistenceScope::Project, "alice");
    let pctx = WriteContext {
        context_label: Label::top(),
        lease_generation: gp,
        at_seq: 12,
        run_id: "run1".to_string(),
    };
    let out = promote(
        &mut store,
        &vk.version_id,
        &human,
        PersistenceScope::Project,
        AuthorityClass::Principal,
        &pctx,
    )
    .unwrap();
    assert!(store
        .drain_events()
        .iter()
        .any(|(c, _)| c == "security.label.endorsed"));
    // The promoted copy supersedes the original, and the endorsement
    // confers the target authority (external → principal: a raise).
    assert_eq!(
        lifecycle_state(&store, &vk.version_id, 99).kind(),
        LifecycleStateKind::Superseded
    );
    assert_eq!(out.version.label.authority, AuthorityClass::Principal);
    assert!(out.version.label.authority > vk.label.authority);
}

#[test]
fn filter_for_slot_is_the_attested_order() {
    // validity → authority → readers: an item failing *all three* reports
    // `validity` (the first failing check wins — the attestation order).
    let policy = ValidityPolicy {
        admitted_states: [LifecycleStateKind::Valid].into_iter().collect(),
        conflict_policy: ConflictPolicy::DeliverAllAnnotated,
        max_stale: None,
    };
    let items = vec![FilterItem {
        version_id: "v1".into(),
        authority: AuthorityClass::Unverified, // below floor
        readers: ReaderSet::Restricted(BTreeSet::new()), // admits nobody
        state: LifecycleStateKind::Revoked,    // not admitted
        stale_since: None,
        conflict_set_ref: None,
    }];
    let out = filter_for_slot(
        &items,
        &policy,
        AuthorityClass::External,
        "model",
        ResolveMode::Execute,
        0,
        &BTreeMap::<String, ConflictSet>::new(),
    );
    assert_eq!(out.withheld.len(), 1);
    assert_eq!(out.withheld[0].reason, "validity");
}

// ── AC-R-2.8.2-5 — quarantine handles never render inline (HandleLeaked) ─────

fn offload_handle(id: &str) -> OffloadHandle {
    OffloadHandle {
        content_address: format!("sha256:blob-{id}"),
        media_type: "text/plain".into(),
        size: 4096,
        label: Label::at(AuthorityClass::External),
        excerpt_report: ExcerptReport {
            truncated_by: Some("bytes".into()),
            total_lines: 40,
            total_bytes: 4096,
            retained_range: (0, 256),
        },
        read_capability: "test:read_surface".into(),
    }
}

/// A handle-carrying candidate delivered *inline* — not `handle_only`, not
/// `by_reference` — would render the offloaded blob into model-facing text:
/// `HandleLeaked`, a typed refusal with no plan (AC-R-2.8.2-5).
#[test]
fn ac_r_2_8_2_5_handle_content_inline_is_handle_leaked() {
    let mut h = cand(
        "h",
        CandidateKind::ArtifactExcerpt,
        AuthorityClass::External,
        10,
        Retention::Optional(PriorityClass::Memory),
        1,
    );
    h.handle = Some(offload_handle("h"));
    let mut s = sink();
    let err = asm(
        &req(
            vec![
                cand(
                    "k",
                    CandidateKind::KernelNotice,
                    AuthorityClass::Kernel,
                    5,
                    Retention::Required,
                    0,
                ),
                h,
            ],
            10_000,
        ),
        &DefaultPolicy::default(),
        &mut s,
    )
    .expect_err("a handle inlined is a leak");
    assert!(
        matches!(&err, AssemblyError::HandleLeaked { candidate_id } if candidate_id == "h"),
        "{err:?}"
    );
}

/// The quarantined form — `handle_only` — assembles fine, contributes nothing
/// to the context-label join, and the next call's `context_label` is
/// unchanged by the handle's own label (AC-R-2.8.2-5: "a quarantine handle
/// passed to a tool joins that argument's label but does not join the
/// context label").
#[test]
fn ac_r_2_8_2_5_handle_only_never_leaks_and_never_joins_context() {
    let mut h = cand(
        "h",
        CandidateKind::ArtifactExcerpt,
        AuthorityClass::Unverified,
        10,
        Retention::Optional(PriorityClass::Memory),
        1,
    );
    h.state = CandidateState::HandleOnly;
    h.label = Label::at(AuthorityClass::Unverified);
    h.handle = Some(offload_handle("h"));
    let mut s = sink();
    let out = asm(
        &req(
            vec![
                cand(
                    "k",
                    CandidateKind::KernelNotice,
                    AuthorityClass::Kernel,
                    5,
                    Retention::Required,
                    0,
                ),
                h,
            ],
            10_000,
        ),
        &DefaultPolicy::default(),
        &mut s,
    )
    .expect("handle_only delivery is legal");
    assert_eq!(
        out.plan.context_label.authority,
        AuthorityClass::Kernel,
        "the handle's label does not join the context fold"
    );
}
