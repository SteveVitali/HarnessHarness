//! `audit_bundle` — the manifest-internal audit pass (§5h.3 §2; the
//! reader-side hygiene complement to S7/S9). It walks the manifest's own
//! invariants: member roles/addresses/sizes well-formed, `fetch[]`
//! entries bound to `fetch`-status members, `contains[]` resolvable
//! within the supplied bundle set, the `subject`/`derived_from` links
//! non-empty where required. Findings are `{code, detail}` diagnostics —
//! never silent fixes, never a panic.

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::manifest::MemberStatus;

/// The audit outcome — `ok` is `true` only when no `*_invalid` finding
/// was produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditReport {
    /// The audited bundle.
    pub bundle_id: String,
    /// `{code, detail, member?}` rows, deterministic order.
    pub findings: Vec<Json>,
    /// No findings at all.
    pub ok: bool,
}

/// `audit_bundle(decoded, known_bundles)` — the member/link hygiene
/// pass. `known_bundles` are the bundle ids reachable for `contains[]`
/// resolution (empty = containment links report `contains_unresolved`
/// as informational, not a failure — the audit is intra-manifest plus
/// the supplied universe).
pub fn audit_bundle(decoded: &Decoded, known_bundles: &BTreeSet<String>) -> AuditReport {
    let m = &decoded.manifest;
    let mut findings: Vec<Json> = Vec::new();
    let mut push = |code: &str, detail: String, member: Option<&str>| {
        let mut f = std::collections::BTreeMap::new();
        f.insert("code".into(), Json::str(code));
        f.insert("detail".into(), Json::str(detail));
        if let Some(mb) = member {
            f.insert("member".into(), Json::str(mb));
        }
        findings.push(Json::Obj(f));
    };

    if m.version_id.is_empty() {
        push(
            "manifest_invalid",
            "manifest has no version_id".into(),
            None,
        );
    }
    if m.subject.run_ids.is_empty() && m.subject.experiment.is_empty() {
        push(
            "subject_empty",
            "manifest binds no run, participant, experiment or lineage".into(),
            None,
        );
    }
    let member_addrs: BTreeSet<&str> = m.members.iter().map(|mm| mm.address.as_str()).collect();
    for mm in &m.members {
        if mm.role.is_empty() {
            push(
                "member_invalid",
                "member with empty role".into(),
                Some(&mm.address),
            );
        }
        if mm.address.is_empty() {
            push(
                "member_invalid",
                "member with empty address".into(),
                Some(&mm.role),
            );
        }
        match mm.status {
            MemberStatus::Fetch if !m.fetch.iter().any(|f| f.address == mm.address) => {
                push(
                    "fetch_entry_missing",
                    format!(
                        "member `{}` is fetch-status but has no fetch[] entry",
                        mm.role
                    ),
                    Some(&mm.role),
                );
            }
            _ => {}
        }
        if mm.status == MemberStatus::Present && !decoded.members.contains_key(&mm.address) {
            push(
                "member_absent",
                format!(
                    "member `{}` declared present but no bytes were supplied",
                    mm.role
                ),
                Some(&mm.role),
            );
        }
    }
    for f in &m.fetch {
        if !member_addrs.contains(f.address.as_str()) {
            push(
                "fetch_orphan",
                format!("fetch[] entry `{}` binds no member", f.address),
                None,
            );
        }
    }
    if let Some(Json::Arr(contains)) = m.composition.get("contains") {
        for c in contains {
            if let Some(id) = c.get("bundle_id").and_then(Json::as_str) {
                if !known_bundles.is_empty() && !known_bundles.contains(id) {
                    push(
                        "contains_unresolved",
                        format!("contained bundle `{id}` not in the supplied universe"),
                        None,
                    );
                }
            }
        }
    }
    findings.sort_by(|a, b| {
        a.get("code")
            .and_then(Json::as_str)
            .cmp(&b.get("code").and_then(Json::as_str))
            .then(
                a.get("member")
                    .and_then(Json::as_str)
                    .cmp(&b.get("member").and_then(Json::as_str)),
            )
    });
    let ok = findings.is_empty();
    AuditReport {
        bundle_id: m.version_id.clone(),
        findings,
        ok,
    }
}
