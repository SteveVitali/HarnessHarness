//! §5g.7 C0 unit tests — every gating behavior gets a removal-sensitive test
//! (AC-R-2.8.7-1/-3/-6/-7/-14 land here and in the hh-ledger/hh-env seams).

use super::*;

fn cap() -> PinnedRef {
    PinnedRef {
        semantic_id: "cap:fs.write".to_string(),
        version_id: "capv-1".to_string(),
    }
}

fn request(args_hash: &str) -> ApprovalRequest {
    ApprovalRequest {
        permission_id: String::new(),
        request: PermissionRequest {
            subject_ref: "agent-1".to_string(),
            capability_ref: cap(),
            args_canonical_hash: args_hash.to_string(),
            reason: "needs write".to_string(),
            requested_grants: Vec::new(),
        },
        options: vec![
            ApprovalOption {
                id: ApprovalOptionId::AllowOnce,
                label: "Allow once".to_string(),
            },
            ApprovalOption {
                id: ApprovalOptionId::Deny,
                label: "Deny".to_string(),
            },
        ],
        mode: ApprovalMode::Sync,
        timeout: None,
        explanation: Explanation {
            display: "Π asked".to_string(),
            rows: Vec::new(),
            model_justification: None,
        },
        batch_id: None,
    }
}

fn principal_endorser() -> EndorserRef {
    EndorserRef::Human {
        subject_ref: "principal-1".to_string(),
        authority: AuthorityClass::Principal,
    }
}

fn delegate_endorser() -> EndorserRef {
    EndorserRef::Human {
        subject_ref: "agent-1".to_string(),
        authority: AuthorityClass::Delegate,
    }
}

fn allow_once(permission_id: &str) -> ApprovalResponse {
    ApprovalResponse {
        permission_id: permission_id.to_string(),
        choice: ResponseChoice::AllowOnce,
        scope: DecisionScope::Once,
        max_uses: None,
        justification: None,
        decided_by: principal_endorser(),
        decided_at: 10,
    }
}

// ── request_approval ──────────────────────────────────────────────────────────

#[test]
fn sync_and_async_admit_immediate_refused() {
    let mut st = ApprovalState::default();
    let mut r = request("h1");
    r.mode = ApprovalMode::Async;
    // `async` (defer) is admitted at Stage 2 — the pending outlives the
    // dispatch; the run suspends on `awaiting_approval`. The pending keys on
    // the minted id (returned on the request — the caller's field is blank).
    let admitted = st
        .request_approval(r, false, "e1", 1)
        .expect("async admits");
    assert!(st.pending.contains_key(&admitted.permission_id));
    // `immediate` never reaches `request_approval` — it is the chain-internal
    // "resolved before the human stage" marker.
    let mut r2 = request("h2");
    r2.mode = ApprovalMode::Immediate;
    assert!(matches!(
        st.request_approval(r2, false, "e2", 1),
        Err(ApprovalError::UnsupportedMode {
            mode: ApprovalMode::Immediate
        })
    ));
}

#[test]
fn ac_14_identical_requests_coalesce() {
    let mut st = ApprovalState::default();
    // Two identical (capability_ref, args_canonical_hash) asks mint the same
    // permission_id and share ONE pending row.
    let r1 = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let r2 = st
        .request_approval(request("h1"), false, "e2", 2)
        .expect("request");
    assert_eq!(r1.permission_id, r2.permission_id);
    assert_eq!(st.pending.len(), 1);
    assert_eq!(st.stats.requested, 1); // one pending row, not two
    let row = &st.pending[&r1.permission_id];
    assert_eq!(row.effect_ids, vec!["e1".to_string(), "e2".to_string()]);
    // A different args hash is a different pending.
    let r3 = st
        .request_approval(request("h2"), false, "e3", 3)
        .expect("request");
    assert_ne!(r3.permission_id, r1.permission_id);
    assert_eq!(st.pending.len(), 2);
    assert_eq!(st.stats.requested, 2);
}

#[test]
fn ac_14_irreversible_never_coalesces() {
    let mut st = ApprovalState::default();
    let r1 = st
        .request_approval(request("h1"), true, "e1", 1)
        .expect("request");
    let r2 = st
        .request_approval(request("h1"), true, "e2", 2)
        .expect("request");
    // The salt is per-effect — identical irreversible asks never share a
    // pending (never batched).
    assert_ne!(r1.permission_id, r2.permission_id);
    assert_eq!(st.pending.len(), 2);
}

/// §5g.7 §5 batching (ADR-0070 D5) — `mint_batch_id` is the deterministic
/// key over `(effective_risk_class, requested_of, scope)`: same members share
/// the batch (one prompt may render N pendings); any member's change splits
/// it; the fold carries `batch_id` on the pending row (the `batchable` admit
/// at `EscalationInput` — `irreversible`/`permission_request` — gates the
/// mint upstream, so a batch id only ever attaches to a batchable ask).
#[test]
fn batch_id_groups_one_risk_class_reviewer_scope() {
    use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
    let a = RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::WorkspaceLocal,
    };
    let b = RiskClass {
        reversibility: RiskReversibility::Irreversible,
        repeat_safety: RepeatSafety::NonIdempotent,
        scope: RiskScope::External,
    };
    // Deterministic: the same members mint the same batch.
    assert_eq!(
        mint_batch_id(&a, "principal", "run-1"),
        mint_batch_id(&a, "principal", "run-1")
    );
    // Each member splits the batch — class, reviewer, scope.
    assert_ne!(
        mint_batch_id(&a, "principal", "run-1"),
        mint_batch_id(&b, "principal", "run-1")
    );
    assert_ne!(
        mint_batch_id(&a, "principal", "run-1"),
        mint_batch_id(&a, "other-reviewer", "run-1")
    );
    assert_ne!(
        mint_batch_id(&a, "principal", "run-1"),
        mint_batch_id(&a, "principal", "run-2")
    );

    // The fold round-trips the batch member: a batched pending carries the
    // id; the response still decides per `permission_id` (a batch allow is
    // recorded as one endorsement per attached effect — AC-R-2.8.7-14).
    let mut st = ApprovalState::default();
    let mut r = request("h1");
    r.batch_id = Some(mint_batch_id(&a, "principal", "run-1"));
    let r1 = st
        .request_approval(r.clone(), false, "e1", 1)
        .expect("request");
    let r2 = st.request_approval(r, false, "e2", 2).expect("request");
    assert_eq!(r1.permission_id, r2.permission_id);
    let row = &st.pending[&r1.permission_id];
    assert_eq!(
        row.request.batch_id,
        Some(mint_batch_id(&a, "principal", "run-1"))
    );
    let out = st
        .respond(&allow_once(&r1.permission_id), false, 5)
        .expect("respond");
    assert_eq!(out.endorsement_count, 2);
}

// ── respond ───────────────────────────────────────────────────────────────────

#[test]
fn respond_unknown_permission_fails() {
    let mut st = ApprovalState::default();
    assert!(matches!(
        st.respond(&allow_once("perm-missing"), false, 5),
        Err(ApprovalError::UnknownPermission { .. })
    ));
}

#[test]
fn ac_14_duplicate_respond_returns_recorded() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let out = st
        .respond(&allow_once(&r.permission_id), false, 5)
        .expect("respond");
    assert_eq!(out.decision, Decision::Allow);
    assert!(!out.already_decided);
    assert_eq!(st.stats.granted, 1);
    // A second response for the same id returns the recorded decision — no
    // second decided row, no extra granted count.
    let out2 = st
        .respond(&allow_once(&r.permission_id), false, 6)
        .expect("respond");
    assert!(out2.already_decided);
    assert_eq!(out2.decision, Decision::Allow);
    assert_eq!(st.stats.granted, 1);
    // Even a *different* choice returns the recorded decision.
    let deny = ApprovalResponse {
        choice: ResponseChoice::Deny {
            reason: "changed mind".to_string(),
        },
        ..allow_once(&r.permission_id)
    };
    let out3 = st.respond(&deny, false, 7).expect("respond");
    assert!(out3.already_decided);
    assert_eq!(out3.decision, Decision::Allow);
}

#[test]
fn ac_3_delegate_endorser_illegitimate() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let bad = ApprovalResponse {
        decided_by: delegate_endorser(),
        ..allow_once(&r.permission_id)
    };
    assert!(matches!(
        st.respond(&bad, false, 5),
        Err(ApprovalError::IllegitimateEndorsement { .. })
    ));
    // The pending survives — an illegitimate response decides nothing.
    assert!(st.pending.contains_key(&r.permission_id));
    assert!(st.decisions.is_empty());
    // An ApproverGrant endorser is legitimate only under a *covering live
    // grant* (AC-R-2.8.7-3) — an unrecorded ref fails IllegitimateEndorsement.
    let ok = ApprovalResponse {
        decided_by: EndorserRef::ApproverGrant {
            grant_ref: "grant-1".to_string(),
        },
        ..allow_once(&r.permission_id)
    };
    assert!(matches!(
        st.respond(&ok, false, 6),
        Err(ApprovalError::IllegitimateEndorsement { .. })
    ));
    // A covering grant admits it (the grant record confers — never the name).
    let ctx = RespondCtx {
        grants: vec![ApproverGrant {
            grant_ref: "grant-1".to_string(),
            grantor: "principal-1".to_string(),
            grantee: "reviewer-1".to_string(),
            capability_prefixes: vec!["cap:fs".to_string()],
            max_risk: hh_ontology::risk::RiskClass::UNKNOWN,
            expires_at: None,
            revoked_at: None,
        }],
        ..RespondCtx::stage1()
    };
    assert!(st.respond_with_ctx(&ok, false, 7, &ctx).is_ok());
}

#[test]
fn ac_14_batch_allow_endorses_every_effect() {
    let mut st = ApprovalState::default();
    let r1 = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("r1");
    let _r2 = st
        .request_approval(request("h1"), false, "e2", 2)
        .expect("r2");
    let out = st
        .respond(&allow_once(&r1.permission_id), false, 5)
        .expect("respond");
    // One allow → N endorsements, one per coalesced effect.
    assert_eq!(out.endorsement_count, 2);
}

#[test]
fn allow_once_requires_once_scope() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let bad = ApprovalResponse {
        scope: DecisionScope::Session,
        ..allow_once(&r.permission_id)
    };
    assert!(matches!(
        st.respond(&bad, false, 5),
        Err(ApprovalError::ScopeMismatch { .. })
    ));
    assert!(st.pending.contains_key(&r.permission_id));
}

// ── Leases ────────────────────────────────────────────────────────────────────

fn allow_lease(permission_id: &str, max_uses: Option<u64>) -> ApprovalResponse {
    ApprovalResponse {
        permission_id: permission_id.to_string(),
        choice: ResponseChoice::AllowLease(LeaseSpec {
            pattern: None,
            scope: LeaseScope::Run,
            max_uses,
        }),
        scope: DecisionScope::Session,
        max_uses,
        justification: None,
        decided_by: principal_endorser(),
        decided_at: 10,
    }
}

#[test]
fn allow_lease_mints_exact_hash_lease() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let out = st
        .respond_with_policy(&allow_lease(&r.permission_id, None), false, 5, "fp-1")
        .expect("respond");
    let lease = out.lease.expect("lease");
    // The key is the exact-hash tuple.
    let expected = lease_key(&cap(), "h1", LeaseScope::Run, "fp-1");
    assert_eq!(lease.lease_id, expected);
    assert_eq!(lease.key_hash, expected);
    assert_eq!(lease.scope, LeaseScope::Run);
    assert_eq!(lease.uses, 0);
    assert_eq!(lease.holder, "agent-1");
    assert_eq!(lease.origin_permission_id, r.permission_id);
    assert_eq!(lease.policy_fingerprint, "fp-1");
    // The lease stage now serves the identical ask — cache decider, no
    // approval consumed.
    let input = EscalationInput {
        effect_id: "e9".to_string(),
        scope_ref: "run".to_string(),
        capability_ref: cap(),
        args_canonical_hash: "h1".to_string(),
        pattern_keys: Vec::new(),
        domain: hh_hir::kinds::EffectDomain::FsWrite,
        risk: hh_ontology::risk::RiskClass::READ_ONLY,
        eff: AuthorityClass::Principal,
        holder: "agent-1".to_string(),
        irreversible: false,
        mode: Mode::Attended,
        unattended_policy: crate::policy::UnattendedPolicy::Deny,
        approval_mode: ApprovalMode::Sync,
        policy_fingerprint: "fp-1".to_string(),
        approvals_used: 0,
        approvals_max: None,
        explicit_ask: false,
        consent_step: false,
        revoked_or_stale: false,
        user_scope_persistence: false,
        batchable: false,
        context_authority: AuthorityClass::Principal,
    };
    let d = escalate(&input, &st.leases);
    assert!(d.lease_hit);
    assert_eq!(d.decision, Decision::Allow);
    assert_eq!(d.cache_key.as_deref(), Some(expected.as_str()));
    // A policy-fingerprint change revokes by construction — the old key
    // never serves the new fingerprint.
    let mut input2 = input.clone();
    input2.policy_fingerprint = "fp-2".to_string();
    let d2 = escalate(&input2, &st.leases);
    assert!(!d2.lease_hit);
    assert!(matches!(d2.decision, Decision::Ask { .. }));
}

#[test]
fn irreversible_lease_requires_max_uses() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), true, "e1", 1)
        .expect("request");
    let bad = ApprovalResponse {
        max_uses: None,
        choice: ResponseChoice::AllowLease(LeaseSpec {
            pattern: Some(ActionPattern {
                fields: ["target".to_string()].into_iter().collect(),
            }),
            scope: LeaseScope::Run,
            max_uses: None,
        }),
        ..allow_lease(&r.permission_id, None)
    };
    assert!(matches!(
        st.respond(&bad, true, 5),
        Err(ApprovalError::LeaseScopeViolation { .. })
    ));
    // max_uses alone is not enough — an irreversible lease also requires a
    // declared ActionPattern (ADR-0071 D1).
    let no_pattern = allow_lease(&r.permission_id, Some(3));
    assert!(matches!(
        st.respond(&no_pattern, true, 6),
        Err(ApprovalError::LeaseScopeViolation { .. })
    ));
    // With a declared pattern + max_uses + scope ≤ run the irreversible lease
    // mints.
    let ok = ApprovalResponse {
        choice: ResponseChoice::AllowLease(LeaseSpec {
            pattern: Some(ActionPattern {
                fields: ["target".to_string()].into_iter().collect(),
            }),
            scope: LeaseScope::Run,
            max_uses: Some(3),
        }),
        ..allow_lease(&r.permission_id, Some(3))
    };
    let out = st.respond(&ok, true, 7).expect("respond");
    let lease = out.lease.expect("lease");
    assert!(lease.pattern.is_some());
    assert_eq!(lease.max_uses, Some(3));
}

#[test]
fn lease_max_uses_bounds_hits() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    st.respond_with_policy(&allow_lease(&r.permission_id, Some(1)), false, 5, "fp")
        .expect("respond");
    let key = lease_key(&cap(), "h1", LeaseScope::Run, "fp");
    let mut input = input(Mode::Attended);
    input.effect_id = "e2".to_string();
    input.policy_fingerprint = "fp".to_string();
    // First hit allowed.
    assert!(escalate(&input, &st.leases).lease_hit);
    st.mark_lease_used(&key);
    // max_uses = 1 → the second ask does NOT hit.
    input.effect_id = "e3".to_string();
    assert!(!escalate(&input, &st.leases).lease_hit);
    // Revocation kills the lease.
    st.revoke_lease(&key, 9);
    assert!(st.lease_lookup(&key).is_none());
}

// ── escalate ──────────────────────────────────────────────────────────────────

fn input(mode: Mode) -> EscalationInput {
    EscalationInput {
        effect_id: "e1".to_string(),
        scope_ref: "run".to_string(),
        capability_ref: cap(),
        args_canonical_hash: "h1".to_string(),
        pattern_keys: Vec::new(),
        domain: hh_hir::kinds::EffectDomain::FsWrite,
        risk: hh_ontology::risk::RiskClass::READ_ONLY,
        eff: AuthorityClass::Principal,
        holder: "agent-1".to_string(),
        irreversible: false,
        mode,
        unattended_policy: crate::policy::UnattendedPolicy::Deny,
        approval_mode: ApprovalMode::Sync,
        policy_fingerprint: "fp".to_string(),
        approvals_used: 0,
        approvals_max: None,
        explicit_ask: false,
        consent_step: false,
        revoked_or_stale: false,
        user_scope_persistence: false,
        batchable: false,
        context_authority: AuthorityClass::Principal,
    }
}

#[test]
fn unattended_ask_denies_before_the_chain() {
    let d = escalate(&input(Mode::Unattended), &BTreeMap::new());
    assert_eq!(d.reason, EscalationReason::UnattendedAsk);
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::UnattendedAsk,
            ..
        }
    ));
    assert!(d.target.is_none());
}

#[test]
fn exhausted_budget_denies() {
    let mut i = input(Mode::Attended);
    i.approvals_used = 2;
    i.approvals_max = Some(2);
    let d = escalate(&i, &BTreeMap::new());
    assert_eq!(d.reason, EscalationReason::ApprovalsExhausted);
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::ApprovalsExhausted,
            ..
        }
    ));
}

#[test]
fn no_lease_hit_escalates_to_human() {
    let d = escalate(&input(Mode::Attended), &BTreeMap::new());
    assert_eq!(d.reason, EscalationReason::EscalateToHuman);
    assert!(matches!(d.decision, Decision::Ask { .. }));
    assert!(matches!(d.target, Some(ReviewerRef::Human { .. })));
    // The cache key is always carried — a future allow_lease mints under it.
    assert!(d.cache_key.is_some());
}

// ── never_auto (I-P1) ─────────────────────────────────────────────────────────

#[test]
fn never_auto_set() {
    use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
    assert!(never_auto(RiskClass::UNKNOWN));
    assert!(never_auto(RiskClass {
        reversibility: RiskReversibility::Irreversible,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::WorkspaceLocal,
    }));
    assert!(!never_auto(RiskClass::READ_ONLY));
    assert!(!never_auto(RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::NonIdempotent,
        scope: RiskScope::WorkspaceLocal,
    }));
}

// ── withdraw + stats ──────────────────────────────────────────────────────────

#[test]
fn withdraw_resolves_pending_never_unknown() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let p = st
        .withdraw(&r.permission_id, PendingTerminal::Cancelled)
        .expect("pending row");
    assert_eq!(p.permission_id, r.permission_id);
    assert!(st.pending.is_empty());
    // Withdrawing a decided id is a no-op (history is append-only).
    let r2 = st
        .request_approval(request("h2"), false, "e2", 2)
        .expect("request");
    st.respond(&allow_once(&r2.permission_id), false, 5)
        .expect("respond");
    assert!(st
        .withdraw(&r2.permission_id, PendingTerminal::Cancelled)
        .is_none());
    assert!(st.decisions.contains_key(&r2.permission_id));
}

#[test]
fn ac_7_counting_requested_granted_wait() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 10)
        .expect("request");
    // requested counts pending rows, not prompt renderings.
    assert_eq!(st.stats.requested, 1);
    st.respond(&allow_once(&r.permission_id), false, 40)
        .expect("respond");
    assert_eq!(st.stats.granted, 1);
    assert_eq!(st.stats.human_wait_ms, 30);
    // A deny counts requested but not granted.
    let r2 = st
        .request_approval(request("h2"), false, "e2", 100)
        .expect("request");
    let deny = ApprovalResponse {
        choice: ResponseChoice::Deny {
            reason: "no".to_string(),
        },
        ..allow_once(&r2.permission_id)
    };
    st.respond(&deny, false, 130).expect("respond");
    assert_eq!(st.stats.requested, 2);
    assert_eq!(st.stats.granted, 1);
    assert_eq!(st.stats.human_wait_ms, 60);
}

// ── Codec round-trips ─────────────────────────────────────────────────────────

#[test]
fn request_payload_roundtrip_shape() {
    let r = st_request();
    let j = request_payload(&r);
    assert_eq!(
        j.get("permission_id").and_then(Json::as_str),
        Some(r.permission_id.as_str())
    );
    assert_eq!(
        j.get("args_canonical_hash").and_then(Json::as_str),
        Some("h1")
    );
    assert!(j.get("options_presented").is_some());
    assert!(j.get("explanation").is_some());
}

fn st_request() -> ApprovalRequest {
    let mut r = request("h1");
    r.permission_id = "perm-1".to_string();
    r
}

#[test]
fn response_payload_shape() {
    let r = allow_lease("perm-1", Some(2));
    let j = response_payload(&r);
    assert!(j.get("choice").is_some());
    assert!(j.get("decided_by").is_some());
    // The lease spec decodes.
    let c = decode_choice(j.get("choice").unwrap()).expect("choice");
    assert!(matches!(c, ResponseChoice::AllowLease(_)));
    let e = decode_endorser(j.get("decided_by").unwrap()).expect("endorser");
    assert!(matches!(e, EndorserRef::Human { .. }));
}

// ── Stage-2: the reviewer chain (AC-R-2.8.7-{2,4,5}) ────────────────────────

#[test]
fn chain_hook_deny_resolves_before_human() {
    let hooks = vec![HookReport {
        hook_ref: "hook-1".to_string(),
        attestation_ref: "att-1".to_string(),
        verdict: HookVerdict::Deny,
    }];
    let out = run_chain(
        &input(Mode::Attended),
        &BTreeMap::new(),
        &hooks,
        &[],
        LeaseScope::Run,
    );
    assert!(matches!(
        out.decision,
        ChainDecision::Deny {
            stage: ReviewStage::Hook,
            ..
        }
    ));
    // The human stage never ran — the hook resolved the ask.
    assert!(out.stages.iter().all(|s| s.stage != ReviewStage::Human));
}

#[test]
fn chain_hook_allow_never_endorses_without_rule() {
    // I-P2: a hook `allow` alone is a `pass` — only a matching sealed
    // auto_review rule turns it into an endorsement.
    let hooks = vec![HookReport {
        hook_ref: "hook-1".to_string(),
        attestation_ref: "att-1".to_string(),
        verdict: HookVerdict::Allow,
    }];
    let out = run_chain(
        &input(Mode::Attended),
        &BTreeMap::new(),
        &hooks,
        &[],
        LeaseScope::Run,
    );
    // No rule matched → the human stage gets the ask.
    assert!(matches!(out.decision, ChainDecision::AskHuman { .. }));
    // With a matching rule the allow resolves at `auto_reviewer` with the
    // `policy_rule` basis — never the hook's bare allow.
    let rules = vec![AutoReviewRule {
        rule_ref: "rule-1".to_string(),
        domains: Vec::new(),
        max_risk: hh_ontology::risk::RiskClass::UNKNOWN,
        eff_at_most: None,
        verdict: ReviewVerdictKind::Allow,
    }];
    let out2 = run_chain(
        &input(Mode::Attended),
        &BTreeMap::new(),
        &hooks,
        &rules,
        LeaseScope::Run,
    );
    match out2.decision {
        ChainDecision::Allow {
            decider,
            basis: LeaseBasis::PolicyRule { ref rule_ref, .. },
            ..
        } => {
            assert_eq!(decider, crate::decision::Decider::AutoReviewer);
            assert_eq!(rule_ref, "rule-1");
        }
        other => panic!("expected auto_reviewer allow, got {other:?}"),
    }
}

/// R-2.8.5 attestation admission — a `HookReport` with no `attestation_ref`
/// is never consulted: its `deny` cannot resolve the chain (the human stage
/// gets the ask). The attested twin denies at the hook stage.
#[test]
fn chain_unattested_hook_report_never_admitted() {
    let unattested = vec![HookReport {
        hook_ref: "hook-unattested".to_string(),
        attestation_ref: String::new(),
        verdict: HookVerdict::Deny,
    }];
    let out = run_chain(
        &input(Mode::Attended),
        &BTreeMap::new(),
        &unattested,
        &[],
        LeaseScope::Run,
    );
    assert!(
        matches!(out.decision, ChainDecision::AskHuman { .. }),
        "an unattested deny never resolves: {out:?}"
    );
    let attested = vec![HookReport {
        hook_ref: "hook-attested".to_string(),
        attestation_ref: "att-9".to_string(),
        verdict: HookVerdict::Deny,
    }];
    let out = run_chain(
        &input(Mode::Attended),
        &BTreeMap::new(),
        &attested,
        &[],
        LeaseScope::Run,
    );
    assert!(matches!(
        out.decision,
        ChainDecision::Deny {
            stage: ReviewStage::Hook,
            ..
        }
    ));
}

#[test]
fn chain_never_auto_skips_nonhuman_stages() {
    // Π-8 — a `permission_request` effect is constitutionally never-auto: the
    // lease/hook/auto_reviewer stages are recorded `pass` with the member tag
    // and the human stage gets the ask (AC-R-2.8.7-2).
    let mut i = input(Mode::Attended);
    i.domain = hh_hir::kinds::EffectDomain::PermissionRequest;
    let hooks = vec![HookReport {
        hook_ref: "hook-1".to_string(),
        attestation_ref: "att-1".to_string(),
        verdict: HookVerdict::Allow,
    }];
    let rules = vec![AutoReviewRule {
        rule_ref: "rule-1".to_string(),
        domains: Vec::new(),
        max_risk: hh_ontology::risk::RiskClass::UNKNOWN,
        eff_at_most: None,
        verdict: ReviewVerdictKind::Allow,
    }];
    let out = run_chain(&i, &BTreeMap::new(), &hooks, &rules, LeaseScope::Run);
    assert!(matches!(out.decision, ChainDecision::AskHuman { .. }));
    // No non-human stage resolved an allow for a never-auto member.
    assert!(out
        .stages
        .iter()
        .all(|s| { s.stage == ReviewStage::Human || s.outcome != StageOutcome::ResolveAllow }));
}

#[test]
fn chain_async_mode_defers_to_human_stage() {
    let mut i = input(Mode::Attended);
    i.approval_mode = ApprovalMode::Async;
    let out = run_chain(&i, &BTreeMap::new(), &[], &[], LeaseScope::Run);
    assert!(matches!(out.decision, ChainDecision::Defer { .. }));
}

#[test]
fn chain_unattended_auto_review_resolves_or_denies() {
    // Π-12 `auto_review`: a matching sealed rule resolves (never-auto members
    // deny); no rule → deny (fail-closed — never a surface trip).
    let mut i = input(Mode::Unattended);
    i.unattended_policy = crate::policy::UnattendedPolicy::AutoReview;
    let rules = vec![AutoReviewRule {
        rule_ref: "rule-1".to_string(),
        domains: Vec::new(),
        max_risk: hh_ontology::risk::RiskClass::UNKNOWN,
        eff_at_most: None,
        verdict: ReviewVerdictKind::Allow,
    }];
    let out = run_chain(&i, &BTreeMap::new(), &[], &rules, LeaseScope::Run);
    assert!(matches!(
        out.decision,
        ChainDecision::Allow {
            decider: crate::decision::Decider::AutoReviewer,
            ..
        }
    ));
    let out2 = run_chain(&i, &BTreeMap::new(), &[], &[], LeaseScope::Run);
    assert!(matches!(
        out2.decision,
        ChainDecision::Deny {
            reason: DenyReason::UnattendedAsk,
            ..
        }
    ));
    // Π-12 `defer` suspends instead of denying.
    let mut i3 = input(Mode::Unattended);
    i3.unattended_policy = crate::policy::UnattendedPolicy::Defer;
    let out3 = run_chain(&i3, &BTreeMap::new(), &[], &[], LeaseScope::Run);
    assert!(matches!(out3.decision, ChainDecision::Defer { .. }));
}

#[test]
fn chain_lease_stage_serves_pattern_key() {
    // An `ActionPattern` lease serves an ask whose declared projection key
    // matches — the `pattern:` key material leg (ADR-0071 D1).
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    let pat = ActionPattern {
        fields: ["target".to_string()].into_iter().collect(),
    };
    let resp = ApprovalResponse {
        choice: ResponseChoice::AllowLease(LeaseSpec {
            pattern: Some(pat.clone()),
            scope: LeaseScope::Run,
            max_uses: Some(2),
        }),
        ..allow_lease(&r.permission_id, Some(2))
    };
    let out = st.respond(&resp, false, 5).expect("respond");
    let lease = out.lease.expect("lease");
    // The lease keyed on `pattern:{pattern_id}:{args_hash}` — the exact-args
    // leg does NOT serve (the lease binds the projection, not the whole tuple).
    let material = format!("pattern:{}:{}", pat.pattern_id(), "h1");
    let key = lease_key(&cap(), &material, LeaseScope::Run, "policy");
    assert_eq!(lease.lease_id, key);
    let mut i = input(Mode::Attended);
    i.pattern_keys = vec![material];
    i.policy_fingerprint = "policy".to_string();
    let d = escalate(&i, &st.leases);
    assert!(d.lease_hit);
    assert_eq!(d.decision, Decision::Allow);
    // Without the declared pattern key the exact-args ask misses the lease —
    // the lease binds only its declared projection.
    let i2 = input(Mode::Attended);
    let d2 = escalate(&i2, &st.leases);
    assert!(!d2.lease_hit);
}

#[test]
fn repeated_denial_fires_typed_fallback() {
    // §5g.7 §5 — the denial ceiling: `deny` decisions count per
    // (capability, args) key; crossing `max_denials` fires the sealed
    // fallback — never a widening, never a silent retry.
    let pol = DenialPolicy {
        max_denials: 1,
        fallback: DenialFallback::StopRun,
    };
    let ctx = RespondCtx {
        denial_policy: Some(pol),
        ..RespondCtx::stage1()
    };
    let mut st = ApprovalState::default();
    for i in 0..2usize {
        // Irreversible requests salt the id by effect — each is its own
        // pending under the shared (capability, args) denial key.
        let r = st
            .request_approval(request("h1"), true, &format!("e{i}"), i as u64 + 1)
            .expect("request");
        let deny = ApprovalResponse {
            choice: ResponseChoice::Deny {
                reason: "no".to_string(),
            },
            ..allow_once(&r.permission_id)
        };
        let out = st
            .respond_with_ctx(&deny, false, (i as u64 + 2) * 10, &ctx)
            .expect("respond");
        if i == 0 {
            assert_eq!(out.denial_fallback, None);
        } else {
            assert_eq!(out.denial_fallback, Some(DenialFallback::StopRun));
        }
    }
    // One (capability, args) key accumulated both denials.
    assert_eq!(st.denial_counts.values().next(), Some(&2));
    assert_eq!(st.denial_counts.len(), 1);
}

// ── ApprovalState::project — the CC1 trail fold ───────────────────────────────

fn env(seq: u64, class: &str, payload: Json) -> hh_ledger::event::EventEnvelope {
    hh_ledger::event::EventEnvelope {
        event_id: format!("evt-{seq:04}"),
        run_id: "run-1".into(),
        seq,
        ts: "2026-09-15T00:00:00.000Z".into(),
        hlc: None,
        plane: hh_ledger::event::EventPlane::Security,
        class: class.into(),
        schema_version: 1,
        producer: hh_ledger::event::Producer::kernel("kernel:test"),
        participant_class: hh_ledger::manifest::ParticipantClass::Native,
        observability_level: std::collections::BTreeSet::from([
            hh_ledger::manifest::ObservabilityLevel::Ledger,
        ]),
        durability: hh_ledger::classes::Durability::Ledger,
        scope: hh_ledger::event::Scope::default(),
        lease_generation: 1,
        parent_event_id: "root".into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: std::collections::BTreeMap::new(),
        provenance: None,
        prev_hash: String::new(),
        payload,
        hash: String::new(),
    }
}

fn pending_row(
    seq: u64,
    pid: &str,
    args_hash: &str,
    effect_id: &str,
) -> hh_ledger::event::EventEnvelope {
    env(
        seq,
        "security.permission.pending",
        Json::obj([
            ("permission_id", Json::str(pid)),
            ("effect_id", Json::str(effect_id)),
            (
                "request",
                Json::obj([
                    ("subject_ref", Json::str("agent-1")),
                    (
                        "capability_ref",
                        Json::obj([
                            ("semantic_id", Json::str("cap:fs.write")),
                            ("version_id", Json::str("capv-1")),
                        ]),
                    ),
                    ("args_canonical_hash", Json::str(args_hash)),
                    ("reason", Json::str("needs write")),
                ]),
            ),
            ("requested_at", Json::Int(100)),
            ("mode", Json::str("async")),
        ]),
    )
}

#[test]
fn fold_pending_decided_allow_serves_resume() {
    // The defer slice's fold: `pending` opens the owed row, the non-final
    // `decided{ask}` never occupies the exactly-one slot, and the final
    // `decided{allow}` resolves it — `decision_for_effect` serves the
    // re-dispatch.
    let events = vec![
        // The attempt cycle's non-final ask verdict (dispatch mints it with
        // the permission_id attached).
        env(
            1,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-1")),
                ("decision", Json::str("ask")),
                ("decider", Json::str("policy")),
            ]),
        ),
        pending_row(2, "perm-1", "h1", "e1"),
        env(
            3,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-1")),
                ("decision", Json::str("allow")),
                ("decider", Json::str("human")),
                ("decider_ref", Json::str("human:principal")),
                ("decision_scope", Json::str("once")),
                ("requested_at", Json::Int(100)),
                ("wait_ms", Json::Int(50)),
            ]),
        ),
    ];
    let st = ApprovalState::project(&events, u64::MAX);
    assert!(
        st.pending.is_empty(),
        "the decided row resolved the pending"
    );
    assert_eq!(st.stats.requested, 1);
    assert_eq!(st.stats.granted, 1);
    assert_eq!(st.stats.human_wait_ms, 50);
    let (pid, rec) = st
        .decision_for_effect("e1")
        .expect("the recorded decision covers the effect");
    assert_eq!(pid, "perm-1");
    assert!(matches!(rec.decision, Decision::Allow));
}

#[test]
fn fold_exactly_one_decided_and_lease_lifecycle() {
    let lease = ApprovalLease {
        lease_id: "lease-1".into(),
        key_hash: "lease-1".into(),
        capability_ref: cap(),
        args_canonical_hash: "h1".into(),
        pattern: None,
        pattern_args_hash: None,
        scope: LeaseScope::Run,
        scope_ref: "run-1".into(),
        holder: "agent-1".into(),
        basis: LeaseBasis::Human,
        origin_permission_id: "perm-1".into(),
        policy_fingerprint: "fp".into(),
        risk_ceiling: hh_ontology::risk::RiskClass::UNKNOWN,
        grant_authority: AuthorityClass::Principal,
        max_uses: Some(3),
        uses: 0,
        granted_at: 150,
        revoked_at: None,
    };
    let events = vec![
        pending_row(1, "perm-1", "h1", "e1"),
        // The lease row before the decided row — either order resolves the
        // link (the fold back-fills `decisions[].lease_id`).
        env(
            2,
            "security.permission.lease.granted",
            lease_granted_payload(&lease),
        ),
        env(
            3,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-1")),
                ("decision", Json::str("allow")),
                ("decider", Json::str("human")),
                ("decision_scope", Json::str("session")),
            ]),
        ),
        // A second `decided` for the id never rewrites the record
        // (the exactly-one gate — I-H7's fold half).
        env(
            4,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-1")),
                ("decision", Json::str("deny")),
                ("decider", Json::str("human")),
            ]),
        ),
        env(
            5,
            "security.permission.lease.used",
            Json::obj([("lease_id", Json::str("lease-1")), ("uses", Json::Int(2))]),
        ),
        env(
            6,
            "security.permission.lease.revoked",
            Json::obj([
                ("lease_id", Json::str("lease-1")),
                ("revoked_at", Json::Int(500)),
            ]),
        ),
    ];
    let st = ApprovalState::project(&events, u64::MAX);
    let rec = st.decisions.get("perm-1").expect("recorded");
    assert!(
        matches!(rec.decision, Decision::Allow),
        "the first row stands"
    );
    assert_eq!(rec.lease_id.as_deref(), Some("lease-1"));
    let l = st.leases.get("lease-1").expect("the lease folded");
    assert_eq!(l.uses, 2);
    assert_eq!(l.revoked_at, Some(500));
    assert_eq!(l.holder, "agent-1");
    assert_eq!(l.policy_fingerprint, "fp");
    assert!(st.lease_lookup("lease-1").is_none(), "revoked leases miss");
}

#[test]
fn fold_deny_counts_repeated_denials() {
    let events = vec![
        pending_row(1, "perm-1", "h1", "e1"),
        env(
            2,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-1")),
                ("decision", Json::str("deny")),
                ("decider", Json::str("human")),
                ("reason", Json::str("PolicyDenied")),
            ]),
        ),
        pending_row(3, "perm-2", "h1", "e2"),
        env(
            4,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-2")),
                ("decision", Json::str("deny")),
                ("decider", Json::str("human")),
                ("reason", Json::str("PolicyDenied")),
            ]),
        ),
    ];
    let st = ApprovalState::project(&events, u64::MAX);
    // One (capability, args) key accumulated both denials.
    assert_eq!(st.denial_counts.values().next(), Some(&2));
    assert!(st.pending.is_empty());
    assert!(st.decision_for_effect("e1").is_some());
}

// ── sealed-extraction (§5g.1 §9 — `auto_review_rules(sealed)` is the chain's
// `auto_reviewer` leg; only sealed records confer) ─────────────────────────

fn sealed_with_rule(action: hh_hir::records::RuleAction) -> hh_hir::SealedDefinition {
    use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
    use hh_hir::records::{HarnessRuleRecord, KindRecord};
    use hh_hir::refs::Ref;
    let mut n = Node::new(
        hh_hir::kinds::EntityKind::HarnessRule,
        KindRecord::HarnessRule(HarnessRuleRecord {
            rule_id: "test:rule.auto".to_string(),
            trigger: Json::Null,
            action,
            scope: Json::Null,
            conditioned_on: None,
            assumption_debt: None,
        }),
        hh_provenance::ProvenanceRecord::kernel("test", 0),
    );
    n.version.semantic_id = Some("test:rule.auto".to_string());
    let mut doc = HirDocument::new(Ref::selected("test:agent", "latest"));
    doc.nodes = vec![n];
    hh_hir::SealedDefinition {
        document: doc,
        definition_ref: DefinitionVersionRef {
            semantic_id: "test:agent".into(),
            version_id: "sha256:def".into(),
        },
        closed_world_tools: Default::default(),
    }
}

#[test]
fn auto_review_rules_extracts_sealed_legs() {
    let spec = Json::obj([
        ("verdict", Json::str("allow")),
        ("domains", Json::Arr(vec![Json::str("fs_write")])),
        ("max_risk", hh_ontology::risk::RiskClass::UNKNOWN.to_json()),
        ("eff_at_most", Json::str("delegate")),
    ]);
    let sealed = sealed_with_rule(hh_hir::records::RuleAction::AutoReview(spec));
    let rules = auto_review_rules(&sealed);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].rule_ref, "test:rule.auto");
    assert_eq!(rules[0].verdict, ReviewVerdictKind::Allow);
    assert_eq!(rules[0].domains, vec![hh_hir::kinds::EffectDomain::FsWrite]);
    assert_eq!(rules[0].eff_at_most, Some(AuthorityClass::Delegate));
    // A malformed spec (unknown verdict) extracts nothing — the extractor
    // never guesses.
    let bad = sealed_with_rule(hh_hir::records::RuleAction::AutoReview(Json::obj([(
        "verdict",
        Json::str("widen"),
    )])));
    assert!(auto_review_rules(&bad).is_empty());
}

/// §5g.1 §9 cap clamp on the `pre_authorize` mint — `min(issuer.authority,
/// caps)`: a global `authority_cap` lowers the minted ceiling; an `of`-named
/// cap binds only its named entity (the rule's coordinate); the issuer's own
/// authority bounds the mint even with no caps.
#[test]
fn pre_authorize_mint_clamps_to_caps_and_issuer() {
    use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
    use hh_hir::records::{HarnessRuleRecord, KindRecord, RuleAction};
    use hh_hir::refs::Ref;
    use hh_provenance::AuthorityClass;
    let grant = hh_hir::records::Grant {
        effect: hh_hir::kinds::EffectClass::domain_only(hh_hir::kinds::EffectDomain::FsRead),
        scope: "workspace/**".to_string(),
        constraints: hh_hir::records::GrantConstraints {
            budget: None,
            time: None,
            count: None,
        },
        delegable: false,
    };
    let grants_json = Json::Arr(vec![hh_hir::grant_json(&grant, false)]);
    let node = |rule_id: &str| {
        let mut n = Node::new(
            hh_hir::kinds::EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: rule_id.to_string(),
                trigger: Json::Null,
                action: RuleAction::PreAuthorize(Json::obj([("grants", grants_json.clone())])),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            hh_provenance::ProvenanceRecord::kernel("test", 0),
        );
        n.version.semantic_id = Some(rule_id.to_string());
        n
    };
    let cap = |of: &str, ceiling: &str| {
        Json::obj([
            ("kind", Json::str("authority_cap")),
            (
                "subject",
                Json::obj([("of", Json::str(of)), ("ceiling", Json::str(ceiling))]),
            ),
        ])
    };
    let sealed = |nodes: Vec<Node>, constraints: Vec<Json>| hh_hir::SealedDefinition {
        document: HirDocument {
            nodes,
            assembly: Some(Json::obj([("constraints", Json::Arr(constraints))])),
            ..HirDocument::new(Ref::selected("test:agent", "latest"))
        },
        definition_ref: DefinitionVersionRef {
            semantic_id: "test:agent".into(),
            version_id: "sha256:def".into(),
        },
        closed_world_tools: Default::default(),
    };
    let holder = Ref::selected("agent-1", "latest");
    let kernel = hh_provenance::ProvenanceRecord::kernel("test", 0);
    let mut n = 0u64;
    let mut alloc = move |k: &str| {
        n += 1;
        format!("{k}-{n}")
    };

    // A global cap (`of = "*"`) lowers the minted ceiling to `external`.
    let doc = sealed(vec![node("test:rule.a")], vec![cap("*", "external")]);
    let out =
        crate::mint::mint_preauthorization_handles(&doc, &holder, &kernel, "run-1", &mut alloc);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0.ceiling, AuthorityClass::External);

    // An `of`-named cap binds only the named entity — a cap on `rule.b`
    // never lowers `rule.a`'s mint; uncapped, the mint issues at the spec's
    // `definition` class (§5g.1 — "issued at `definition`; bounded by
    // `authority_cap`"), never the issuer's own `kernel`.
    let doc = sealed(
        vec![node("test:rule.a")],
        vec![cap("test:rule.b", "unverified")],
    );
    let out =
        crate::mint::mint_preauthorization_handles(&doc, &holder, &kernel, "run-1", &mut alloc);
    assert_eq!(out[0].0.ceiling, AuthorityClass::Definition);

    // The issuer's own authority bounds the mint with no caps at all.
    let delegate = hh_provenance::ProvenanceRecord {
        authority: AuthorityClass::Delegate,
        ..hh_provenance::ProvenanceRecord::kernel("test", 0)
    };
    let doc = sealed(vec![node("test:rule.a")], vec![]);
    let out =
        crate::mint::mint_preauthorization_handles(&doc, &holder, &delegate, "run-1", &mut alloc);
    assert_eq!(out[0].0.ceiling, AuthorityClass::Delegate);
}

#[test]
fn pre_authorize_mint_skips_grantless_rules() {
    use hh_hir::records::RuleAction;
    // A `pre_authorize` rule with no decodable `grants` mints nothing.
    let sealed = sealed_with_rule(RuleAction::PreAuthorize(Json::obj([(
        "scope",
        Json::str("run"),
    )])));
    let holder = hh_hir::refs::Ref::selected("agent-1", "latest");
    let issuer = hh_provenance::ProvenanceRecord::kernel("test", 0);
    let mut n = 0u64;
    let mut alloc = move |k: &str| {
        n += 1;
        format!("{k}-{n}")
    };
    assert!(crate::mint::mint_preauthorization_handles(
        &sealed, &holder, &issuer, "run-1", &mut alloc
    )
    .is_empty());
}
