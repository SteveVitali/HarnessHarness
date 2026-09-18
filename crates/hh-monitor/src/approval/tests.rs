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
        },
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
fn sync_only_at_stage1() {
    let mut st = ApprovalState::default();
    let mut r = request("h1");
    r.mode = ApprovalMode::Async;
    assert!(matches!(
        st.request_approval(r, false, "e1", 1),
        Err(ApprovalError::UnsupportedMode {
            mode: ApprovalMode::Async
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
    // An ApproverGrant endorser is legitimate.
    let ok = ApprovalResponse {
        decided_by: EndorserRef::ApproverGrant {
            grant_ref: "grant-1".to_string(),
        },
        ..allow_once(&r.permission_id)
    };
    assert!(st.respond(&ok, false, 6).is_ok());
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
            scope: DecisionScope::Session,
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
    let expected = lease_key(&cap(), "h1", DecisionScope::Session, "fp-1");
    assert_eq!(lease.lease_id, expected);
    assert_eq!(lease.key_hash, expected);
    assert_eq!(lease.scope, DecisionScope::Session);
    assert_eq!(lease.uses, 0);
    // The lease stage now serves the identical ask — cache decider, no
    // approval consumed.
    let input = EscalationInput {
        effect_id: "e9".to_string(),
        capability_ref: cap(),
        args_canonical_hash: "h1".to_string(),
        irreversible: false,
        mode: Mode::Attended,
        policy_fingerprint: "fp-1".to_string(),
        approvals_used: 0,
        approvals_max: None,
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
            scope: DecisionScope::Session,
            max_uses: None,
        }),
        ..allow_lease(&r.permission_id, None)
    };
    assert!(matches!(
        st.respond(&bad, true, 5),
        Err(ApprovalError::LeaseScopeViolation { .. })
    ));
    // With max_uses the irreversible lease mints.
    let ok = allow_lease(&r.permission_id, Some(3));
    assert!(st.respond(&ok, true, 6).is_ok());
}

#[test]
fn lease_max_uses_bounds_hits() {
    let mut st = ApprovalState::default();
    let r = st
        .request_approval(request("h1"), false, "e1", 1)
        .expect("request");
    st.respond_with_policy(&allow_lease(&r.permission_id, Some(1)), false, 5, "fp")
        .expect("respond");
    let key = lease_key(&cap(), "h1", DecisionScope::Session, "fp");
    let mut input = EscalationInput {
        effect_id: "e2".to_string(),
        capability_ref: cap(),
        args_canonical_hash: "h1".to_string(),
        irreversible: false,
        mode: Mode::Attended,
        policy_fingerprint: "fp".to_string(),
        approvals_used: 0,
        approvals_max: None,
    };
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
        capability_ref: cap(),
        args_canonical_hash: "h1".to_string(),
        irreversible: false,
        mode,
        policy_fingerprint: "fp".to_string(),
        approvals_used: 0,
        approvals_max: None,
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
