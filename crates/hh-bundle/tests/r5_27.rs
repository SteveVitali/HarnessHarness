//! R5.27 — §6.4 rights/losses/quotas/recipient-handoff validation
//! (`admit_handoff` / `sanitize_replace`; `rights-record/1`,
//! `handoff-report/1`, `rights-supersession/1`, `rights-tombstone/1`).
//! AC-1: per-item rights/access/expiry/loss + bounded local handoff,
//! sanitized replacement → new identity + supersession + dependent
//! claims recalculated, actual dates bound before commitments.
//! AC-2: unknown rights, secret material, sensitive low-entropy
//! identifiers, unauthorized/expired fetch, quota/expansion breach,
//! deletion-marker-only evidence refuse; controlled closure is not
//! public closure; optional S3/account/provisioning → separate
//! authority.

use std::collections::BTreeMap;

use hharness_bundle::codec::Decoded;
use hharness_bundle::error::BundleError;
use hharness_bundle::manifest::{
    BundleManifest, FetchEntry, MemberRef, MemberStatus, SubjectSection,
};
use hharness_bundle::rights::{
    admit_handoff, sanitize_replace, Departure, HandoffPlan, HandoffScope, RightsDecision,
    RightsRecord, SanitizeTransform, HANDOFF_REPORT_SCHEMA, SUPERSESSION_SCHEMA, TOMBSTONE_SCHEMA,
};
use hharness_bundle::trust::FetchTrustPolicy;
use hharness_wire::json::Json;

/// 2026-01-02T00:00:00Z in ms — the deterministic evaluation clock.
const NOW: u64 = 1_767_312_000_000;
const PAST: &str = "2026-01-01T00:00:00.000Z";
const LATER: &str = "2026-07-01T00:00:00.000Z";
const FUTURE: &str = "2027-01-01T00:00:00.000Z";

fn member(role: &str, bytes: &[u8], status: MemberStatus) -> (MemberRef, String, Vec<u8>) {
    let addr = hharness_identity::address(bytes, "application/octet-stream").id();
    let mut m = MemberRef::present(
        role,
        addr.clone(),
        "application/octet-stream",
        bytes.len() as u64,
    );
    m.status = status;
    (m, addr, bytes.to_vec())
}

fn manifest(members: Vec<MemberRef>, fetch: Vec<FetchEntry>, def_member: &str) -> BundleManifest {
    BundleManifest {
        bundle_kind: "run".into(),
        created_at: PAST.into(),
        producer: Json::Null,
        participant_class: "native".into(),
        observability_levels: vec![],
        claims: vec![],
        name_bindings: vec![],
        fetch_policy: "self_contained".into(),
        fetch,
        subject: SubjectSection {
            run_ids: vec![],
            heads: BTreeMap::new(),
            lineage: vec![],
            watermarks: BTreeMap::new(),
            status: "finished".into(),
            experiment: BTreeMap::new(),
        },
        definition: Json::obj([("member", Json::str(def_member))]),
        configuration: Json::Null,
        resolved_dependencies: Json::Null,
        model: Json::Null,
        instrument: Json::Null,
        traces: BTreeMap::new(),
        results: Json::Null,
        reproducibility: Json::Null,
        members,
        unpinned: vec![],
        composition: Json::Null,
        evidence_profile: Json::Null,
        ext: BTreeMap::new(),
        version_id: "idp:bundle-r527".into(),
    }
}

fn record(member: &str, decision: RightsDecision) -> RightsRecord {
    RightsRecord {
        member: member.into(),
        source: "suite".into(),
        version: "v1".into(),
        license: "apache-2.0".into(),
        consent: "n/a".into(),
        recipients: vec!["reviewer-x".into()],
        uses: vec!["review".into()],
        owner: "maintainer".into(),
        decision,
        expiry: None,
    }
}

fn plan<'a>(scope: HandoffScope, items: Vec<String>) -> HandoffPlan<'a> {
    HandoffPlan {
        scope,
        items,
        rights: vec![],
        departures: vec![],
        contact: "maintainer@example.org".into(),
        host: String::new(),
        region: "local".into(),
        recipient: "reviewer-x".into(),
        quoted_at: PAST.into(),
        decided_at: PAST.into(),
        review_before: LATER.into(),
        grant_days: 0,
        link_expiry_ms: 0,
        write_delete_list: false,
        restricted_identifiers: vec![],
        compressed_bytes: 1 << 20,
        expanded_bytes: 4 << 20,
        first_wave_bytes: 8 << 20,
        trust: None,
    }
}

fn rig() -> (Decoded, String, String, String) {
    let (doc, a_doc, b_doc) = member("report", b"public numerical rows", MemberStatus::Present);
    let (rows, a_rows, b_rows) = member("rows", b"row payload", MemberStatus::Present);
    let (dep, a_dep, b_dep) = member("rows", b"departed rows", MemberStatus::Present);
    let d = Decoded {
        manifest: manifest(vec![doc, rows, dep], vec![], &a_doc),
        members: BTreeMap::from([
            (a_doc.clone(), b_doc),
            (a_rows.clone(), b_rows),
            (a_dep.clone(), b_dep),
        ]),
    };
    (d, a_doc, a_rows, a_dep)
}

fn refused_code(e: &BundleError) -> &str {
    match e {
        BundleError::RightsRefused { code, .. } => code,
        other => panic!("expected RightsRefused, got {other}"),
    }
}

fn transform() -> SanitizeTransform {
    SanitizeTransform {
        kind: "pseudonymize".into(),
        version: "1".into(),
        mapping_permitted: true,
        procedures: vec!["replay".into(), "recompute".into()],
        restricted_identifiers: vec![],
    }
}

#[test]
fn public_review_released_items_admitted() {
    let (d, a_doc, a_rows, _) = rig();
    let mut p = plan(
        HandoffScope::PublicReview,
        vec![a_doc.clone(), a_rows.clone()],
    );
    p.rights = vec![
        record(&a_doc, RightsDecision::Release),
        record(&a_rows, RightsDecision::Release),
    ];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "admitted");
    assert_eq!(r.scope, "public_review");
    assert!(!r.report_id.is_empty());
    assert_eq!(r.items.len(), 2);
    assert!(r
        .items
        .iter()
        .all(|i| i.get("decision").and_then(Json::as_str) == Some("release")));
}

#[test]
fn controlled_review_restricted_named_recipient_admitted() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::ControlledReview, vec![a_doc.clone()]);
    let mut rec = record(&a_doc, RightsDecision::Restricted);
    rec.recipients = vec!["reviewer-x".into()];
    p.rights = vec![rec];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "admitted");
}

#[test]
fn fetch_member_admitted_https_leg() {
    let (fx, a_fx, _) = member("evidence", b"remote", MemberStatus::Fetch);
    let mut d = Decoded {
        manifest: manifest(
            vec![fx],
            vec![FetchEntry {
                address: a_fx.clone(),
                size: None,
                locations: vec!["https://files.hh.dev/m".into()],
                expires: Some(LATER.into()),
            }],
            "",
        ),
        members: BTreeMap::new(),
    };
    d.manifest.fetch_policy = "detached_allowed".into();
    let policy = FetchTrustPolicy {
        admission_classes: vec!["first_party".into()],
        admitted_origins: vec!["files.hh.dev".into()],
        accepted_signers: vec![],
        require_ledgered_arrival: true,
    };
    let mut p = plan(HandoffScope::ControlledReview, vec![a_fx.clone()]);
    p.rights = vec![record(&a_fx, RightsDecision::Restricted)];
    p.trust = Some(&policy);
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "admitted");
}

#[test]
fn departures_emit_tombstones_and_disable_procedures() {
    let (d, _, _, a_dep) = rig();
    let mut p = plan(HandoffScope::ControlledReview, vec![]);
    p.departures = vec![Departure {
        member: a_dep.clone(),
        disposition: "deleted".into(),
        occurred_at: PAST.into(),
        evidence: "receipt:tomb-1".into(),
    }];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "admitted");
    assert_eq!(r.tombstones.len(), 1);
    assert_eq!(
        r.tombstones[0].get("schema").and_then(Json::as_str),
        Some(TOMBSTONE_SCHEMA)
    );
    assert!(r.disabled_procedures.iter().any(|x| x == "verify"));
    assert!(r.disabled_procedures.iter().any(|x| x == "recompute"));
}

#[test]
fn shorter_rights_override_retention() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
    let mut rec = record(&a_doc, RightsDecision::Release);
    rec.expiry = Some("2026-03-01T00:00:00.000Z".into());
    p.rights = vec![rec];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "admitted");
    assert_eq!(
        r.retention.get("effective_until").and_then(Json::as_str),
        Some("2026-03-01T00:00:00.000Z")
    );
    assert_eq!(
        r.retention.get("override").and_then(Json::as_str),
        Some("shorter_rights")
    );
}

#[test]
fn sanitize_replacement_new_identity_supersession_recalculated() {
    let (d, a_doc, _, _) = rig();
    let out = sanitize_replace(
        &d,
        &a_doc,
        b"public numerical rows (sanitized)",
        &transform(),
    )
    .unwrap();
    assert_ne!(out.replacement, a_doc);
    assert!(out
        .dependent_claims
        .iter()
        .any(
            |c| c.get("from").and_then(Json::as_str) == Some(a_doc.as_str())
                && c.get("to").and_then(Json::as_str) == Some(out.replacement.as_str())
        ));
    assert!(out
        .losses
        .iter()
        .all(
            |l| l.get("loss").and_then(Json::as_str) == Some("native_identity")
                && l.get("r_levels").and_then(Json::as_str) == Some("R0")
        ));
    let s = &out.supersession;
    assert_eq!(
        s.get("schema").and_then(Json::as_str),
        Some(SUPERSESSION_SCHEMA)
    );
    assert!(s
        .get("supersession_id")
        .and_then(Json::as_str)
        .is_some_and(|i| !i.is_empty()));
    assert_eq!(
        s.get("original").and_then(Json::as_str),
        Some(a_doc.as_str())
    );
}

#[test]
fn unknown_and_withheld_rights_withhold() {
    let (d, a_doc, a_rows, _) = rig();
    let mut p = plan(
        HandoffScope::PublicReview,
        vec![a_doc.clone(), a_rows.clone(), "sha256:ghost".into()],
    );
    p.rights = vec![record(&a_doc, RightsDecision::Unknown)];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "withheld");
    assert!(r
        .items
        .iter()
        .any(|i| i.get("detail").and_then(Json::as_str) == Some("unknown_rights")));
    let mut p2 = plan(HandoffScope::ControlledReview, vec![a_rows.clone()]);
    p2.rights = vec![record(&a_rows, RightsDecision::Withheld)];
    assert_eq!(admit_handoff(&d, &p2, NOW).unwrap().verdict, "withheld");
}

#[test]
fn secret_material_and_sensitive_identifier_refuse() {
    let (sec, a_sec, b_sec) = member(
        "report",
        br#"{"token":"AKIAIOSFODNN7EXAMPLE"}"#,
        MemberStatus::Present,
    );
    let (idm, a_idm, b_idm) = member(
        "report",
        b"subject alice@example.com raw id",
        MemberStatus::Present,
    );
    let d = Decoded {
        manifest: manifest(vec![sec, idm], vec![], ""),
        members: BTreeMap::from([(a_sec.clone(), b_sec), (a_idm.clone(), b_idm)]),
    };
    let mut p = plan(HandoffScope::PublicReview, vec![a_sec.clone()]);
    p.rights = vec![record(&a_sec, RightsDecision::Release)];
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "secret_material"
    );
    let mut p2 = plan(HandoffScope::PublicReview, vec![a_idm.clone()]);
    p2.rights = vec![record(&a_idm, RightsDecision::Release)];
    p2.restricted_identifiers = vec!["alice@example.com".into()];
    assert_eq!(
        refused_code(&admit_handoff(&d, &p2, NOW).unwrap_err()),
        "sensitive_identifier"
    );
}

#[test]
fn unauthorized_and_expired_fetch_refuse() {
    let mk = |loc: &str, exp: Option<&str>, addr: &mut String| {
        let (fx, a_fx, _) = member("evidence", b"remote", MemberStatus::Fetch);
        *addr = a_fx;
        Decoded {
            manifest: manifest(
                vec![fx],
                vec![FetchEntry {
                    address: addr.clone(),
                    size: None,
                    locations: vec![loc.into()],
                    expires: exp.map(str::to_string),
                }],
                "",
            ),
            members: BTreeMap::new(),
        }
    };
    let policy = FetchTrustPolicy {
        admission_classes: vec!["first_party".into()],
        admitted_origins: vec!["files.hh.dev".into()],
        accepted_signers: vec![],
        require_ledgered_arrival: true,
    };
    // Expired entry expiry.
    let mut a = String::new();
    let d = mk("https://files.hh.dev/m", Some(PAST), &mut a);
    let mut p = plan(HandoffScope::ControlledReview, vec![a.clone()]);
    p.rights = vec![record(&a, RightsDecision::Restricted)];
    p.trust = Some(&policy);
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "refused");
    assert!(r
        .items
        .iter()
        .any(|i| i.get("detail").and_then(Json::as_str) == Some("fetch_expired")));
    // Unadmitted origin.
    let d = mk("https://other.example.net/m", Some(LATER), &mut a);
    let mut p = plan(HandoffScope::ControlledReview, vec![a.clone()]);
    p.rights = vec![record(&a, RightsDecision::Restricted)];
    p.trust = Some(&policy);
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert!(r
        .items
        .iter()
        .any(|i| i.get("detail").and_then(Json::as_str) == Some("unauthorized_fetch")));
    // Remote leg with no declared policy.
    let d = mk("https://files.hh.dev/m", Some(LATER), &mut a);
    let mut p = plan(HandoffScope::ControlledReview, vec![a.clone()]);
    p.rights = vec![record(&a, RightsDecision::Restricted)];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "refused");
    // Fetch member with no manifest fetch[] entry at all.
    let (fx, a_fx, _) = member("evidence", b"remote", MemberStatus::Fetch);
    let d = Decoded {
        manifest: manifest(vec![fx], vec![], ""),
        members: BTreeMap::new(),
    };
    let mut p = plan(HandoffScope::ControlledReview, vec![a_fx.clone()]);
    p.rights = vec![record(&a_fx, RightsDecision::Restricted)];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert!(r
        .items
        .iter()
        .any(|i| i.get("detail").and_then(Json::as_str) == Some("unauthorized_fetch")));
}

#[test]
fn quota_and_expansion_breaches_refuse() {
    let (d, a_doc, _, _) = rig();
    for (name, mutate) in [
        (
            "compressed",
            Box::new(|p: &mut HandoffPlan| p.compressed_bytes = 3 << 30)
                as Box<dyn Fn(&mut HandoffPlan)>,
        ),
        (
            "expanded",
            Box::new(|p: &mut HandoffPlan| p.expanded_bytes = 9 << 30),
        ),
        (
            "first_wave",
            Box::new(|p: &mut HandoffPlan| p.first_wave_bytes = 11 << 30),
        ),
    ] {
        let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
        p.rights = vec![record(&a_doc, RightsDecision::Release)];
        mutate(&mut p);
        assert_eq!(
            refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
            "quota_breach",
            "{name}"
        );
    }
    // Per-asset ceiling: a member larger than 512 MiB refuses.
    let (big, a_big, _) = member("report", b"x", MemberStatus::Present);
    let mut big = big;
    big.size = 600 * (1 << 20);
    let d = Decoded {
        manifest: manifest(vec![big], vec![], ""),
        members: BTreeMap::new(),
    };
    let mut p = plan(HandoffScope::PublicReview, vec![a_big.clone()]);
    p.rights = vec![record(&a_big, RightsDecision::Release)];
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "quota_breach"
    );
}

#[test]
fn marker_only_deletion_refuses() {
    let (d, _, _, a_dep) = rig();
    for dep in [
        Departure {
            member: a_dep.clone(),
            disposition: "deleted".into(),
            occurred_at: PAST.into(),
            evidence: String::new(),
        },
        Departure {
            member: a_dep.clone(),
            disposition: "marker".into(),
            occurred_at: PAST.into(),
            evidence: "receipt:1".into(),
        },
    ] {
        let mut p = plan(HandoffScope::ControlledReview, vec![]);
        p.departures = vec![dep];
        assert_eq!(
            refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
            "marker_not_deletion"
        );
    }
}

#[test]
fn restricted_in_public_and_controlled_as_public_refuse() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Restricted)];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "refused");
    assert!(r
        .items
        .iter()
        .any(|i| i.get("detail").and_then(Json::as_str) == Some("scope_mismatch")));
}

#[test]
fn optional_hosting_and_s3_host_require_separate_authority() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::OptionalHosting, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Release)];
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "separate_authority"
    );
    let mut p = plan(HandoffScope::ControlledReview, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Restricted)];
    p.host = "s3:bucket/prefix".into();
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "separate_authority"
    );
}

#[test]
fn missing_and_future_commitment_dates_refuse() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Release)];
    p.quoted_at = String::new();
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "missing_input"
    );
    let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Release)];
    p.decided_at = FUTURE.into();
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "future_date"
    );
}

#[test]
fn access_grant_ceilings_and_overreach_refuse() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::AccessGrant, vec![a_doc.clone()]);
    let mut rec = record(&a_doc, RightsDecision::Restricted);
    rec.recipients = vec!["reviewer-x".into()];
    p.rights = vec![rec.clone()];
    p.grant_days = 45;
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "quota_breach"
    );
    let mut p = plan(HandoffScope::AccessGrant, vec![a_doc.clone()]);
    p.rights = vec![rec.clone()];
    p.grant_days = 30;
    p.link_expiry_ms = 3_600_001;
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "quota_breach"
    );
    let mut p = plan(HandoffScope::AccessGrant, vec![a_doc.clone()]);
    p.rights = vec![rec];
    p.grant_days = 30;
    p.link_expiry_ms = 3_600_000;
    p.write_delete_list = true;
    assert_eq!(
        refused_code(&admit_handoff(&d, &p, NOW).unwrap_err()),
        "grant_overreach"
    );
    // The honest access grant admits.
    let mut p = plan(HandoffScope::AccessGrant, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Restricted)];
    p.grant_days = 30;
    p.link_expiry_ms = 3_600_000;
    assert_eq!(admit_handoff(&d, &p, NOW).unwrap().verdict, "admitted");
}

#[test]
fn held_out_roles_default_restricted() {
    let (sol, a_sol, b_sol) = member("held_out_solution", b"answer", MemberStatus::Present);
    let d = Decoded {
        manifest: manifest(vec![sol], vec![], ""),
        members: BTreeMap::from([(a_sol.clone(), b_sol)]),
    };
    let mut rec = record(&a_sol, RightsDecision::Release);
    rec.license = String::new();
    rec.consent = String::new();
    let mut p = plan(HandoffScope::ControlledReview, vec![a_sol.clone()]);
    p.rights = vec![rec];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    assert_eq!(r.verdict, "withheld");
}

#[test]
fn sanitize_refusals() {
    let (d, a_doc, _, _) = rig();
    // Identical bytes — no new identity.
    let old = d.members.get(&a_doc).unwrap().clone();
    assert_eq!(
        refused_code(&sanitize_replace(&d, &a_doc, &old, &transform()).unwrap_err()),
        "unchanged_bytes"
    );
    // Unpermitted mapping.
    let mut t = transform();
    t.mapping_permitted = false;
    assert_eq!(
        refused_code(&sanitize_replace(&d, &a_doc, b"changed", &t).unwrap_err()),
        "unpermitted_mapping"
    );
    // Secret in the replacement bytes.
    assert_eq!(
        refused_code(
            &sanitize_replace(
                &d,
                &a_doc,
                br#"{"token":"AKIAIOSFODNN7EXAMPLE"}"#,
                &transform()
            )
            .unwrap_err()
        ),
        "secret_material"
    );
    // Unknown member.
    assert_eq!(
        refused_code(&sanitize_replace(&d, "sha256:ghost", b"x", &transform()).unwrap_err()),
        "malformed"
    );
}

#[test]
fn handoff_report_seals_and_records() {
    let (d, a_doc, _, _) = rig();
    let mut p = plan(HandoffScope::PublicReview, vec![a_doc.clone()]);
    p.rights = vec![record(&a_doc, RightsDecision::Release)];
    let r = admit_handoff(&d, &p, NOW).unwrap();
    let j = r.to_json();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some(HANDOFF_REPORT_SCHEMA)
    );
    assert_eq!(
        j.get("checked_at").and_then(Json::as_str),
        Some("2026-01-02T00:00:00.000Z")
    );
    assert!(j
        .get("report_id")
        .and_then(Json::as_str)
        .is_some_and(|s| !s.is_empty()));
}
