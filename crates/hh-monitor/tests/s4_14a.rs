//! S4.14a — the `ApproverGrant` lifecycle and the calibrated `auto_review`
//! binding (AC-R-2.8.7-3 / -8; §5g.7 §4/§5; ADR-0070; OQ-177).
//!
//! Coverage:
//! - `grant_approver` legitimacy — principal bootstraps; a delegate covered by
//!   a live grant may issue a *narrower* sub-grant; uncovered or widening
//!   issuance fails `IllegitimateGrant` (the record confers, never the name).
//! - `covers` / `covers_grant` legs — expiry, revocation, risk ceiling, prefix
//!   scoping.
//! - `revoke_approver` — grantor or principal+ revokes; an unrelated delegate
//!   cannot.
//! - The respond-path gate — an `ApproverGrant`-endorsed `allow_once` is
//!   legitimate only while a live grant covers the pending's capability and
//!   the decided risk; anything else fails `IllegitimateEndorsement` and the
//!   pending survives.
//! - `check_auto_review_binding` — each AC-R-2.8.7-8 clause is a typed
//!   refusal (`uncalibrated`, `permission_widening`, `egress_admitted`,
//!   `evidence_below_floor`), never a quiet pass.
//! - `auto_review_failure_denies` — a reviewer failure resolves `deny` above
//!   `reversible` risk.

use std::collections::BTreeMap;

use hh_compiler::plan::PinnedRef;
use hh_monitor::approval::{
    auto_review_failure_denies, check_auto_review_binding, grant_approver, revoke_approver,
    ApprovalError, ApprovalMode, ApprovalOption, ApprovalOptionId, ApprovalRequest,
    ApprovalResponse, ApprovalState, ApproverGrant, AutoReviewBindingError, EndorserRef,
    Explanation, PermissionRequest, RespondCtx, ResponseChoice, ReviewerBinding,
};
use hh_monitor::decision::{Decision, DecisionScope};
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
use hh_provenance::authority::{AuthorityClass, PersistenceScope};
use hh_provenance::origin::{HumanRole, Origin};
use hh_provenance::record::ProvenanceRecord;

/// A human-principal provenance record — the grant-bootstrap grantor.
fn principal(author: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author, HumanRole::Principal),
        PersistenceScope::Run,
        0,
    )
}

/// A delegate provenance record (authority `delegate` — never `principal+`).
fn delegate(agent_ref: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::model(agent_ref, "run-1", "resp-1"),
        PersistenceScope::Run,
        0,
    )
}

/// A pending `ApprovalRequest` for `capability_ref`.
fn pending_request(capability: &str) -> ApprovalRequest {
    ApprovalRequest {
        permission_id: String::new(),
        request: PermissionRequest {
            subject_ref: "agent:test".into(),
            capability_ref: PinnedRef {
                semantic_id: capability.to_string(),
                version_id: "v-1".into(),
            },
            args_canonical_hash: format!("sha256:args-{capability}"),
            reason: "test".into(),
            requested_grants: vec![],
        },
        options: vec![
            ApprovalOption {
                id: ApprovalOptionId::AllowOnce,
                label: "allow".into(),
            },
            ApprovalOption {
                id: ApprovalOptionId::Deny,
                label: "deny".into(),
            },
        ],
        mode: ApprovalMode::Sync,
        timeout: None,
        explanation: Explanation {
            display: "test ask".into(),
            rows: vec![],
            model_justification: None,
        },
        batch_id: None,
    }
}

/// The reversible risk class — under every non-`read_only` grant ceiling.
fn reversible() -> RiskClass {
    RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::NonIdempotent,
        scope: RiskScope::WorkspaceLocal,
    }
}

/// An `allow_once` response endorsed by `decided_by`.
fn allow_response(permission_id: &str, decided_by: EndorserRef) -> ApprovalResponse {
    ApprovalResponse {
        permission_id: permission_id.to_string(),
        choice: ResponseChoice::AllowOnce,
        scope: DecisionScope::Once,
        max_uses: None,
        justification: None,
        decided_by,
        decided_at: 7,
    }
}

// ── grant_approver legitimacy (§5g.7 §4; OQ-177) ────────────────────────────

#[test]
fn principal_bootstraps_a_grant() {
    let (grant, payload) = grant_approver(
        &principal("alice"),
        "reviewer:bob",
        vec!["fs.write".into()],
        reversible(),
        None,
        10,
        &[],
    )
    .expect("a principal+ grantor bootstraps the grant set");
    assert!(!grant.grant_ref.is_empty(), "the grant_ref is minted");
    assert_eq!(grant.grantee, "reviewer:bob");
    assert_eq!(grant.grantor, "human:alice");
    assert_eq!(
        payload.get("grant_ref").and_then(|j| j.as_str()),
        Some(grant.grant_ref.as_str()),
        "the issued payload names the minted coordinate"
    );
}

#[test]
fn a_delegate_without_a_covering_grant_cannot_confer() {
    let err = grant_approver(
        &delegate("model:mallory"),
        "reviewer:bob",
        vec![],
        reversible(),
        None,
        10,
        &[],
    )
    .unwrap_err();
    assert!(
        matches!(err, ApprovalError::IllegitimateGrant { .. }),
        "a delegate name alone confers nothing: {err:?}"
    );
}

#[test]
fn delegation_may_narrow_but_never_widen() {
    let now = 10;
    let (parent, _) = grant_approver(
        &principal("alice"),
        "model:carol",
        vec!["fs.".into()],
        reversible(),
        None,
        now,
        &[],
    )
    .unwrap();
    // The delegate record's coordinate is `model:carol` — issuance under it.
    let carol = delegate("carol");
    // Narrowing: prefix ⊆ the covering grant's, same risk ceiling.
    let (sub, _) = grant_approver(
        &carol,
        "reviewer:dave",
        vec!["fs.write".into()],
        reversible(),
        None,
        now,
        std::slice::from_ref(&parent),
    )
    .expect("a covered delegate may issue a narrower sub-grant");
    assert!(parent.covers_grant(&sub, now));
    // Widening the risk ceiling refuses.
    let err = grant_approver(
        &carol,
        "reviewer:dave",
        vec!["fs.write".into()],
        RiskClass::UNKNOWN,
        None,
        now,
        std::slice::from_ref(&parent),
    )
    .unwrap_err();
    assert!(matches!(err, ApprovalError::IllegitimateGrant { .. }));
    // Widening outside the prefix scope refuses.
    let err = grant_approver(
        &carol,
        "reviewer:dave",
        vec!["net.egress".into()],
        reversible(),
        None,
        now,
        std::slice::from_ref(&parent),
    )
    .unwrap_err();
    assert!(matches!(err, ApprovalError::IllegitimateGrant { .. }));
}

#[test]
fn covers_enforces_expiry_revocation_risk_and_prefix() {
    let mut grant = ApproverGrant {
        grant_ref: "g-1".into(),
        grantor: "human:alice".into(),
        grantee: "reviewer:bob".into(),
        capability_prefixes: vec!["fs.".into()],
        max_risk: reversible(),
        expires_at: Some(100),
        revoked_at: None,
    };
    assert!(grant.covers("fs.write", &reversible(), 50));
    assert!(
        !grant.covers("net.egress", &reversible(), 50),
        "prefix scope"
    );
    assert!(!grant.covers("fs.write", &reversible(), 101), "expired");
    assert!(
        !grant.covers("fs.write", &RiskClass::UNKNOWN, 50),
        "risk ceiling"
    );
    grant.revoked_at = Some(60);
    assert!(
        !grant.covers("fs.write", &reversible(), 50),
        "revoked never covers"
    );
    grant.expires_at = None;
    assert!(!grant.covers("fs.write", &reversible(), 50));
}

#[test]
fn revoke_approver_is_grantor_or_principal_only() {
    let now = 10;
    let (grant, _) = grant_approver(
        &principal("alice"),
        "reviewer:bob",
        vec![],
        reversible(),
        None,
        now,
        &[],
    )
    .unwrap();
    let live = std::slice::from_ref(&grant);
    // An unrelated delegate cannot revoke upward.
    assert!(matches!(
        revoke_approver(&grant.grant_ref, &delegate("mallory"), live, now + 1),
        Err(ApprovalError::IllegitimateGrant { .. })
    ));
    // The grantor revokes her own grant.
    let payload = revoke_approver(&grant.grant_ref, &principal("alice"), live, now + 1)
        .expect("the grantor revokes");
    assert_eq!(
        payload.get("revoked_at").and_then(|j| j.as_int()),
        Some(11),
        "the revoked payload carries the revocation timestamp"
    );
    // A distinct principal+ revokes too.
    assert!(revoke_approver(&grant.grant_ref, &principal("carol"), live, now + 1).is_ok());
    // An unknown coordinate names nothing.
    assert!(matches!(
        revoke_approver("g-missing", &principal("alice"), live, now + 1),
        Err(ApprovalError::IllegitimateGrant { .. })
    ));
}

// ── the respond-path legitimacy gate (AC-R-2.8.7-3) ─────────────────────────

#[test]
fn an_approver_grant_endorsement_decides_only_while_live() {
    let now = 10;
    let (grant, _) = grant_approver(
        &principal("alice"),
        "reviewer:bob",
        vec!["fs.".into()],
        reversible(),
        None,
        now,
        &[],
    )
    .unwrap();

    let ctx = || RespondCtx {
        risk_ceiling: reversible(),
        grants: vec![grant.clone()],
        ..RespondCtx::stage1()
    };

    // A covered grant endorsement decides the pending.
    let mut state = ApprovalState::default();
    let req = state
        .request_approval(pending_request("fs.write"), false, "eff-1", now)
        .unwrap();
    let outcome = state
        .respond_with_ctx(
            &allow_response(
                &req.permission_id,
                EndorserRef::ApproverGrant {
                    grant_ref: grant.grant_ref.clone(),
                },
            ),
            false,
            now + 1,
            &ctx(),
        )
        .expect("a live covering grant legitimises the allow");
    assert!(matches!(outcome.decision, Decision::Allow));

    // A grant that does not cover the capability confers nothing — the
    // pending survives the refused response.
    let mut state = ApprovalState::default();
    let req = state
        .request_approval(pending_request("net.egress"), false, "eff-2", now)
        .unwrap();
    let err = state
        .respond_with_ctx(
            &allow_response(
                &req.permission_id,
                EndorserRef::ApproverGrant {
                    grant_ref: grant.grant_ref.clone(),
                },
            ),
            false,
            now + 1,
            &ctx(),
        )
        .unwrap_err();
    assert!(matches!(err, ApprovalError::IllegitimateEndorsement { .. }));
    assert!(
        state.pending.contains_key(&req.permission_id),
        "an illegitimate response decides nothing — the owed row survives"
    );

    // A revoked grant stops covering.
    let mut revoked = grant.clone();
    revoked.revoked_at = Some(now + 2);
    let ctx_revoked = RespondCtx {
        risk_ceiling: reversible(),
        grants: vec![revoked],
        ..RespondCtx::stage1()
    };
    let mut state = ApprovalState::default();
    let req = state
        .request_approval(pending_request("fs.write"), false, "eff-3", now)
        .unwrap();
    assert!(matches!(
        state.respond_with_ctx(
            &allow_response(
                &req.permission_id,
                EndorserRef::ApproverGrant {
                    grant_ref: grant.grant_ref.clone(),
                },
            ),
            false,
            now + 3,
            &ctx_revoked,
        ),
        Err(ApprovalError::IllegitimateEndorsement { .. })
    ));

    // A non-principal human endorser confers nothing either.
    let mut state = ApprovalState::default();
    let req = state
        .request_approval(pending_request("fs.write"), false, "eff-4", now)
        .unwrap();
    assert!(matches!(
        state.respond_with_ctx(
            &allow_response(
                &req.permission_id,
                EndorserRef::Human {
                    subject_ref: "human:eve".into(),
                    authority: AuthorityClass::Delegate,
                },
            ),
            false,
            now + 1,
            &ctx(),
        ),
        Err(ApprovalError::IllegitimateEndorsement { .. })
    ));
}

// ── the auto_review calibrated binding (AC-R-2.8.7-8) ───────────────────────

fn bound_binding() -> ReviewerBinding {
    ReviewerBinding {
        validator_ref: "v:calibrated-judge@1".into(),
        calibration_ref: Some("cal:fn-rate-2026Q3".into()),
        permissions_within_parent_readonly: true,
        net_egress_empty: true,
        evidence_floor_ok: true,
    }
}

#[test]
fn the_auto_review_binding_gates_every_clause() {
    check_auto_review_binding(&bound_binding()).expect("a fully-bound reviewer admits");
    for (name, mutate, expected) in [
        (
            "uncalibrated",
            Box::new(|b: &mut ReviewerBinding| b.calibration_ref = None)
                as Box<dyn Fn(&mut ReviewerBinding)>,
            AutoReviewBindingError::Uncalibrated,
        ),
        (
            "permission_widening",
            Box::new(|b: &mut ReviewerBinding| b.permissions_within_parent_readonly = false)
                as Box<dyn Fn(&mut ReviewerBinding)>,
            AutoReviewBindingError::PermissionWidening,
        ),
        (
            "egress_admitted",
            Box::new(|b: &mut ReviewerBinding| b.net_egress_empty = false)
                as Box<dyn Fn(&mut ReviewerBinding)>,
            AutoReviewBindingError::EgressAdmitted,
        ),
        (
            "evidence_below_floor",
            Box::new(|b: &mut ReviewerBinding| b.evidence_floor_ok = false)
                as Box<dyn Fn(&mut ReviewerBinding)>,
            AutoReviewBindingError::EvidenceBelowFloor,
        ),
    ] {
        let mut b = bound_binding();
        mutate(&mut b);
        assert_eq!(
            check_auto_review_binding(&b),
            Err(expected),
            "clause {name} must refuse typed, never pass quietly"
        );
    }
}

#[test]
fn a_reviewer_failure_denies_above_reversible() {
    let mut risk = reversible();
    assert!(
        !auto_review_failure_denies(&risk),
        "reversible falls back to pass"
    );
    risk.reversibility = RiskReversibility::Irreversible;
    assert!(
        auto_review_failure_denies(&risk),
        "irreversible failure is a deny"
    );
    assert!(auto_review_failure_denies(&RiskClass::UNKNOWN));
}

// ── the fold: grant_issued / grant_revoked are durable state ────────────────

#[test]
fn grant_rows_fold_into_live_grants() {
    use hh_ledger::classes::Durability;
    use hh_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
    use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};
    use hh_wire::json::Json;
    use std::collections::BTreeSet;

    let env = |seq: u64, class: &str, payload: Json| EventEnvelope {
        event_id: format!("evt-{seq:04}"),
        run_id: "run-1".into(),
        seq,
        ts: "2026-09-16T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Lifecycle),
        class: class.to_string(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: BTreeSet::from([ObservabilityLevel::Ledger]),
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: "evt-0000".into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        prev_hash: "sha256:prev".into(),
        payload,
        hash: String::new(),
    };

    let (grant, issued) = grant_approver(
        &principal("alice"),
        "reviewer:bob",
        vec!["fs.".into()],
        reversible(),
        None,
        10,
        &[],
    )
    .unwrap();

    let events = vec![env(1, "security.permission.grant_issued", issued)];
    let projected = ApprovalState::project(&events, u64::MAX);
    assert_eq!(
        projected.grants.get(&grant.grant_ref),
        Some(&grant),
        "the fold rebuilds the grant"
    );

    let mut revoked_payload = grant.to_json();
    if let Json::Obj(m) = &mut revoked_payload {
        m.insert("revoked_at".into(), hh_wire::json::Json::Int(20));
        m.insert("revoker".into(), hh_wire::json::Json::str("human:alice"));
    }
    let events = vec![
        env(1, "security.permission.grant_issued", grant.to_json()),
        env(2, "security.permission.grant_revoked", revoked_payload),
    ];
    let projected = ApprovalState::project(&events, u64::MAX);
    assert_eq!(projected.grants.len(), 1);
    assert_eq!(
        projected.grants.get(&grant.grant_ref).map(|g| g.revoked_at),
        Some(Some(20)),
        "grant_revoked marks the durable row — it never deletes"
    );
}
