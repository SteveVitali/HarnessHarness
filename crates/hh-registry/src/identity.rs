//! `identify`/`verify` over the one `idp/1` construction (§8.3 #1–#3, N3/N5/N6;
//! ADR-0239). The two coordinates answer different questions:
//!
//! - `version_id = idp(<kind domain tag>, H(full record body))` — the *exact
//!   bytes* coordinate. **Every** declared field is covered — `variant_id`,
//!   `version_label`, `summary`, `declared_costs` included — so a rename or a
//!   summary edit mints a **new** `version_id` (AC-7).
//! - `semantic_id = idp("semantic.registry.<kind>", H(core − variant_id))` — the
//!   *comparison* coordinate (N6), declared only for kinds with a semantic
//!   projection (variant, class). The surface fields and the tag stay out, so the
//!   coordinate **survives** a rename/summary edit (AC-7).
//!
//! All ids are minted/verified through `hh_identity::idp` over `hh_wire` canonical
//! JSON — no second scheme (CC1/CC7).

use hh_identity::idp::{address, idp_digest, idp_id as mint_id, parse_id, IDP_1};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::records::{RegistryRecord, RegistrySnapshot};
use crate::schema;

/// The `version_id` of a record — `idp(<kind domain tag>, H(full body))`.
/// Covers every declared field: a rename or a summary edit is a new version.
pub fn version_id(r: &RegistryRecord) -> String {
    let body = schema::body_json(r, false);
    let tag = r.kind().domain_tag();
    let digest = body.to_canonical_string();
    mint_id(&tag, digest.as_bytes())
}

/// The `semantic_id` of a record with a declared projection — the comparison
/// coordinate that survives a rename (N6). `None` for version-only kinds.
pub fn semantic_id(r: &RegistryRecord) -> Option<String> {
    let proj = schema::semantic_projection_json(r)?;
    let tag = format!("semantic.{}", r.kind().domain_tag());
    let digest = proj.to_canonical_string();
    Some(mint_id(&tag, digest.as_bytes()))
}

/// Verify a record body against its claimed `version_id` — the `verify()` half of
/// the store's snapshot/content integrity check. The `registry_snapshot` kind
/// mints over its own projection ([`snapshot_id`]) — a record's id never covers
/// the id member itself.
pub fn verify_body(r: &RegistryRecord, claimed_version_id: &str) -> Result<(), RegistryError> {
    if let RegistryRecord::Snapshot(s) = r {
        let recomputed = snapshot_id_of(s);
        return if recomputed == claimed_version_id {
            Ok(())
        } else {
            Err(RegistryError::SchemaViolation {
                path: "version_id".to_string(),
                detail: format!("snapshot id mismatch: {recomputed} != {claimed_version_id}"),
            })
        };
    }
    let body = schema::body_json(r, false);
    let bytes = body.to_canonical_string().into_bytes();
    let parsed = parse_id(claimed_version_id).map_err(|e| RegistryError::SchemaViolation {
        path: "version_id".to_string(),
        detail: format!("unparsable id: {e:?}"),
    })?;
    let computed = idp_digest(&r.kind().domain_tag(), &bytes);
    let got = format!("{}:{}", parsed.algorithm, parsed.digest_hex);
    if parsed.digest_hex == computed {
        Ok(())
    } else {
        Err(RegistryError::SchemaViolation {
            path: "version_id".to_string(),
            detail: format!("idp mismatch: expected {computed}, got {got}"),
        })
    }
}

/// The canonical name-binding spellings (`"<ns>/<name>=<version_id>"`) — shared
/// by the mint and the verify projections (CC7).
pub fn binding_spellings(s: &RegistrySnapshot) -> Vec<String> {
    s.name_bindings
        .iter()
        .map(|((ns, n), v)| format!("{ns}/{n}={v}"))
        .collect()
}

/// The `registry_snapshot_id` — `idp("registry.snapshot", H(members, bindings,
/// policy digest, created_seq))`. Deterministic over the same store state
/// (AC-1/AC-8).
pub fn snapshot_id(
    members: &[String],
    name_bindings: &[String],
    policy_digest: &str,
    created_seq: u64,
) -> String {
    let mut m = std::collections::BTreeMap::new();
    m.insert("created_seq".to_string(), Json::Int(created_seq as i64));
    m.insert(
        "members".to_string(),
        Json::Arr(members.iter().map(|s| Json::str(s.clone())).collect()),
    );
    m.insert(
        "name_bindings".to_string(),
        Json::Arr(name_bindings.iter().map(|s| Json::str(s.clone())).collect()),
    );
    m.insert(
        "policy_digest".to_string(),
        Json::str(policy_digest.to_string()),
    );
    let digest = Json::Obj(m).to_canonical_string();
    mint_id("registry.snapshot", digest.as_bytes())
}

/// The snapshot record's id — recomputed for `verify`.
pub fn snapshot_id_of(s: &RegistrySnapshot) -> String {
    let member_strs: Vec<String> = s.members.iter().cloned().collect();
    snapshot_id(
        &member_strs,
        &binding_spellings(s),
        &s.policy_digest,
        s.created_seq,
    )
}

/// The blob content-address of a canonical payload (the one `idp("blob", …)` —
/// reused for the store's canonical-log persistence).
pub fn payload_address(bytes: &[u8]) -> String {
    address(bytes, "application/json").id()
}

/// The `idp/1` profile id this store writes on every reference.
pub fn idp_profile() -> &'static str {
    IDP_1.idp_id
}
