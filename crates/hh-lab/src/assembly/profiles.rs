//! The registry `ProfileView` the plan-time compiled-surface check
//! (`C-PROF-2`, R-2.1.4¹ᵇ) and `lab.assembly.compile` read through (S4.12;
//! ADR-0304 D3). CF-046's snapshot-confined read model — the compiler never
//! reaches a live registry; the view reads the *records* the one
//! `RegistryStore` holds. Home moved from hh-embed at S5.1 so
//! `AssemblyService::plan`'s stage-6b pass-through shares the one resolver
//! (CC1/CC7).

use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

/// Resolve a model/profile coordinate through the registry — the
/// spellings §3.2.2 admits (`content_hash`/`version_id`,
/// `profile_id@version`, `namespace/name[@label]`). `None` = the
/// coordinate names nothing *admissible* — `UnknownCoordinate` at the
/// boundary, never a silent substitute.
pub fn resolve_profile_coordinate(
    store: &RegistryStore,
    coordinate: &str,
) -> Option<(String, hh_compiler::profile::ModelProfile)> {
    use hh_registry::kinds::Admission;
    use hh_registry::records::RegistryRecord;
    // The stored body is the registry envelope `{kind:"model_profile",
    // profile:<ModelProfile/1 view>}` — the compiler decodes the payload
    // member, never the envelope (opaque layering, CC3).
    let decode = |body: &Json| {
        body.get("profile")
            .and_then(|v| hh_compiler::schema::profile_from_json(v, "profile").ok())
    };
    let from_record = |rec: &RegistryRecord, revoked: bool, vid: &str| {
        if revoked {
            return None;
        }
        if let RegistryRecord::ModelProfile(body) = rec {
            return decode(body).map(|p| (vid.to_string(), p));
        }
        None
    };
    // `version_id`/`content_hash` spelling — resolve derives the
    // effective admission (`revoked` coordinates nothing live).
    if let Ok(r) = store.resolve(
        &hh_registry::store::ResolveInput::Version(coordinate.to_string()),
        hh_identity::names::ResolveMode::Execute,
        &hh_registry::store::ResolveRequest::default(),
    ) {
        if !matches!(r.admission, Admission::Revoked) {
            if let Some(hit) = from_record(&r.record, false, &r.envelope.version_id) {
                return Some(hit);
            }
        }
    }
    // `namespace/name[@label]` — the name-resolution path (revocation
    // and yank tombstones are the resolve machinery's answers).
    if let Some((ns, rest)) = coordinate.split_once('/') {
        let (name, label) = match rest.split_once('@') {
            Some((n, l)) => (n, Some(l.to_string())),
            None => (rest, None),
        };
        let input = hh_registry::store::ResolveInput::Selector {
            namespace: ns.to_string(),
            name: name.to_string(),
            label,
            snapshot_id: None,
        };
        if let Ok(r) = store.resolve(
            &input,
            hh_identity::names::ResolveMode::Execute,
            &hh_registry::store::ResolveRequest::default(),
        ) {
            if !matches!(r.admission, Admission::Revoked) {
                if let Some(hit) = from_record(&r.record, false, &r.envelope.version_id) {
                    return Some(hit);
                }
            }
        }
    }
    // `profile_id@version` — the canonical coordinate spelling; scan
    // the kind's catalog (revocation is the derived `revoked` flag —
    // a revoked profile coordinates nothing live).
    let pred = hh_registry::store::QueryPredicate {
        clauses: vec![hh_registry::store::QueryClause {
            field: "kind".to_string(),
            op: hh_registry::store::QueryOp::Eq,
            value: "model_profile".to_string(),
        }],
        snapshot_id: None,
    };
    for entry in store.catalog(Some(&pred)).unwrap_or_default() {
        if entry.revoked {
            continue;
        }
        if let Some((vid, p)) = from_record(&entry.record, false, &entry.envelope.version_id) {
            if hh_compiler::profile::profile_coordinate(&p) == coordinate
                || p.content_hash == coordinate
            {
                return Some((vid, p));
            }
        }
    }
    None
}

/// The profile test report the registry holds beside a coordinate
/// (ADR-0125 d.1): `profile_ref`/`profile_hash` match — `None` is the
/// link gate's `profile_untested`, never coerced.
pub fn resolve_test_report(
    store: &RegistryStore,
    coordinate: &str,
) -> Option<hh_compiler::profile_test::ProfileTestReport> {
    use hh_registry::records::RegistryRecord;
    let pred = hh_registry::store::QueryPredicate {
        clauses: vec![hh_registry::store::QueryClause {
            field: "kind".to_string(),
            op: hh_registry::store::QueryOp::Eq,
            value: "profile_test_report".to_string(),
        }],
        snapshot_id: None,
    };
    for entry in store.catalog(Some(&pred)).unwrap_or_default() {
        if entry.revoked {
            continue;
        }
        if let RegistryRecord::ProfileTestReport(body) = &entry.record {
            // `{kind:"profile_test_report", target:{…}, report:<report>}`
            // — the payload member, never the envelope.
            let payload = body.get("report").cloned().unwrap_or(Json::Null);
            if let Some(r) = hh_compiler::profile_test::test_report_from_json(&payload) {
                if r.profile_ref == coordinate || r.profile_hash == coordinate {
                    return Some(r);
                }
            }
        }
    }
    None
}

/// `ProfileView` over the one `RegistryStore` (S4.12; ADR-0304 D3).
/// Bound coordinates, `extends` ancestors and test reports all read
/// through the same registry — the compile is as deterministic as the
/// records are.
pub struct RegistryProfiles<'a>(pub &'a RegistryStore);

impl hh_compiler::profile::ProfileView for RegistryProfiles<'_> {
    fn profile(&self, coordinate: &str) -> Option<hh_compiler::profile::ModelProfile> {
        resolve_profile_coordinate(self.0, coordinate).map(|(_, p)| p)
    }
    fn test_report(
        &self,
        coordinate: &str,
    ) -> Option<hh_compiler::profile_test::ProfileTestReport> {
        resolve_test_report(self.0, coordinate)
    }
}
