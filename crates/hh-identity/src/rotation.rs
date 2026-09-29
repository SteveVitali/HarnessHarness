//! Identity rotation (spec §5g.6 §2, R-2.8.6; ADR-0307) — the pure record
//! shapes and migration helpers. The store-driving `rotate` op (the
//! `security.audit.checkpoint{kind = rotation}` row, the `rehash` claim,
//! the `bridge_record_ref`) lives in `hh-ledger::rotation` because it
//! writes a run ledger; this module is ledger-free (CC5 — identity never
//! reaches into the ledger).
//!
//! Semantics:
//!
//! - A `RotationPlan` declares `from_idp → to_idp`, both registered
//!   profiles. The `to_idp` named by a *minting* step must be the
//!   currently writable profile (`IdpNotWritable` otherwise — spec §5g.6
//!   §2's refusal row).
//! - `migrate_id` maps a stored id to its new-profile spelling by the
//!   declared `MigrationMethod`: `rotate-full-rehash`/`copy-store`
//!   recompute over the object's canonical bytes; `alias-id` carries the
//!   id verbatim (no rehash — the alias is the bridge).
//! - Stored ids are **never rewritten** — verification across profiles is
//!   by recomputation, not replacement (spec §5g.6 §2 "old ids retained;
//!   verification across profiles"; N1).

use hh_wire::json::Json;

use crate::idp::{idp_id_in, parse_id_in, profile_for, IdError, IdentityProfile};

/// `id_migration.method ∈ {rotate-full-rehash, alias-id, copy-store}`
/// (spec §5g.6 §2's migration record member).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationMethod {
    /// Recompute every migrated id under the new profile over the same
    /// canonical bytes (`H_{idp'}(preimage)` — the heavyweight path).
    RotateFullRehash,
    /// The id text is carried verbatim under the new profile's namespace
    /// (no rehash — the bridge resolves the old spelling).
    AliasId,
    /// The object bytes are copied into a new store under new-profile ids
    /// (the migration record carries the new id; the bytes are unchanged).
    CopyStore,
}

impl MigrationMethod {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MigrationMethod::RotateFullRehash => "rotate-full-rehash",
            MigrationMethod::AliasId => "alias-id",
            MigrationMethod::CopyStore => "copy-store",
        }
    }
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<MigrationMethod> {
        Some(match s {
            "rotate-full-rehash" => MigrationMethod::RotateFullRehash,
            "alias-id" => MigrationMethod::AliasId,
            "copy-store" => MigrationMethod::CopyStore,
            _ => return None,
        })
    }
}

/// The rotation/refusal sum (spec §5g.6 §2's `refused` column +
/// the honest additions ADR-0307 records):
/// `UnknownIdentityProfile` (a plan or id names an unregistered idp),
/// `IdpNotWritable` (a mint under a profile that is not the currently
/// writable one), `BridgeMissing` (a cross-profile resolution attempted
/// without a `BridgeRecord`), `ContentMissing` (a rehash method without
/// the object's bytes), `MalformedId` (the stored id does not parse under
/// its declared profile).
#[derive(Debug, Clone, PartialEq)]
pub enum RotationError {
    /// The named idp is not in [`crate::idp::PROFILES`].
    UnknownIdentityProfile {
        /// The unregistered idp name.
        idp: String,
    },
    /// The target profile is not currently writable.
    IdpNotWritable {
        /// The refused profile name.
        idp: String,
    },
    /// A cross-profile resolution needs a `BridgeRecord` and none binds.
    BridgeMissing {
        /// The id or ref that could not resolve.
        object_ref: String,
    },
    /// A rehash method was asked for without the object's canonical bytes.
    ContentMissing {
        /// The object the bytes were needed for.
        object_ref: String,
    },
    /// The stored id does not parse under its declared profile.
    MalformedId {
        /// The offending id text.
        id: String,
        /// The underlying parse error.
        error: IdError,
    },
}

impl std::fmt::Display for RotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RotationError::UnknownIdentityProfile { idp } => {
                write!(f, "unknown_idp: `{idp}` is not a registered profile")
            }
            RotationError::IdpNotWritable { idp } => {
                write!(f, "idp_not_writable: `{idp}` is not the writable profile")
            }
            RotationError::BridgeMissing { object_ref } => {
                write!(f, "bridge_missing: `{object_ref}` has no bridge record")
            }
            RotationError::ContentMissing { object_ref } => {
                write!(
                    f,
                    "content_missing: rehash of `{object_ref}` needs the object bytes"
                )
            }
            RotationError::MalformedId { id, error } => {
                write!(f, "malformed_id: `{id}` does not parse ({error:?})")
            }
        }
    }
}

impl std::error::Error for RotationError {}

/// `RotationPlan{from_idp, to_idp, declared_at, reason, bridge,
/// attestation_ref, effective_from_seq, rehash}` — the declared rotation
/// (spec §5g.6 §2). `bridge = true` requests the `BridgeRecord` a
/// cross-profile resolution consults; `rehash = true` requests the
/// checkpoint's `rehash{idp', chain_hash', tree_head'}` claim member.
#[derive(Debug, Clone, PartialEq)]
pub struct RotationPlan {
    /// The profile the covered records were minted under (`idp/1`).
    pub from_idp: String,
    /// The profile being rotated to (`idp/2`) — must be a registered
    /// profile; minting under it additionally requires it be writable.
    pub to_idp: String,
    /// The run the rotation is declared on.
    pub declared_at: String,
    /// The human/operator reason (never empty — CC3).
    pub reason: String,
    /// Whether a `BridgeRecord` is minted alongside the checkpoint.
    pub bridge: bool,
    /// The attestation binding the rotation (an attestation ref, e.g. a
    /// `security.attestation.*` record id).
    pub attestation_ref: Option<String>,
    /// The seq the rotation takes effect at (`None` = at emit).
    pub effective_from_seq: Option<u64>,
    /// Whether the checkpoint claims carry the `rehash` member.
    pub rehash: bool,
}

impl RotationPlan {
    /// Validate the plan: both profiles registered and distinct.
    pub fn validate(
        &self,
    ) -> Result<(&'static IdentityProfile, &'static IdentityProfile), RotationError> {
        let from =
            profile_for(&self.from_idp).ok_or_else(|| RotationError::UnknownIdentityProfile {
                idp: self.from_idp.clone(),
            })?;
        let to =
            profile_for(&self.to_idp).ok_or_else(|| RotationError::UnknownIdentityProfile {
                idp: self.to_idp.clone(),
            })?;
        if from.idp_id == to.idp_id {
            return Err(RotationError::IdpNotWritable {
                idp: self.to_idp.clone(),
            });
        }
        Ok((from, to))
    }
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("from_idp", Json::str(self.from_idp.clone())),
            ("to_idp", Json::str(self.to_idp.clone())),
            ("declared_at", Json::str(self.declared_at.clone())),
            ("reason", Json::str(self.reason.clone())),
            ("bridge", Json::Bool(self.bridge)),
            ("rehash", Json::Bool(self.rehash)),
        ];
        if let Some(a) = &self.attestation_ref {
            m.push(("attestation_ref", Json::str(a.clone())));
        }
        if let Some(s) = self.effective_from_seq {
            m.push((
                "effective_from_seq",
                Json::Int(s.min(i64::MAX as u64) as i64),
            ));
        }
        Json::obj(m)
    }
    /// Decode (lenient — `validate` is the authority).
    pub fn from_json(j: &Json) -> Option<RotationPlan> {
        Some(RotationPlan {
            from_idp: j.get("from_idp")?.as_str()?.to_string(),
            to_idp: j.get("to_idp")?.as_str()?.to_string(),
            declared_at: j.get("declared_at")?.as_str()?.to_string(),
            reason: j
                .get("reason")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            bridge: matches!(j.get("bridge"), Some(Json::Bool(true))),
            attestation_ref: j
                .get("attestation_ref")
                .and_then(Json::as_str)
                .map(String::from),
            effective_from_seq: j
                .get("effective_from_seq")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            rehash: matches!(j.get("rehash"), Some(Json::Bool(true))),
        })
    }
}

/// `IdMigration{old_id, new_id, object_ref, method}` — one migrated id
/// (spec §5g.6 §2).
#[derive(Debug, Clone, PartialEq)]
pub struct IdMigration {
    /// The stored id under the source profile.
    pub old_id: String,
    /// The id under the target profile (verbatim under `alias-id`).
    pub new_id: String,
    /// The object the migration binds.
    pub object_ref: String,
    /// The method used.
    pub method: MigrationMethod,
}

impl IdMigration {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("old_id", Json::str(self.old_id.clone())),
            ("new_id", Json::str(self.new_id.clone())),
            ("object_ref", Json::str(self.object_ref.clone())),
            ("method", Json::str(self.method.as_str())),
        ])
    }
    /// Decode.
    pub fn from_json(j: &Json) -> Option<IdMigration> {
        Some(IdMigration {
            old_id: j.get("old_id")?.as_str()?.to_string(),
            new_id: j.get("new_id")?.as_str()?.to_string(),
            object_ref: j.get("object_ref")?.as_str()?.to_string(),
            method: MigrationMethod::parse(j.get("method")?.as_str()?)?,
        })
    }
}

/// `migrate_id` — map one stored id to its target-profile spelling
/// (spec §5g.6 §2). `canonical` carries the object's canonical bytes for
/// the rehash methods; `writable` is the currently writable profile the
/// minted `new_id` must name (`IdpNotWritable` otherwise — a migration
/// cannot mint under a retired profile).
#[allow(clippy::too_many_arguments)] // the record's members are its shape.
pub fn migrate_id(
    id: &str,
    from: &IdentityProfile,
    to: &'static IdentityProfile,
    writable: &IdentityProfile,
    domain_tag: &str,
    canonical: Option<&[u8]>,
    object_ref: &str,
    method: MigrationMethod,
) -> Result<IdMigration, RotationError> {
    if to.idp_id != writable.idp_id {
        return Err(RotationError::IdpNotWritable {
            idp: to.idp_id.to_string(),
        });
    }
    parse_id_in(from, id).map_err(|e| RotationError::MalformedId {
        id: id.to_string(),
        error: e,
    })?;
    let new_id = match method {
        MigrationMethod::AliasId => id.to_string(),
        MigrationMethod::RotateFullRehash | MigrationMethod::CopyStore => {
            let bytes = canonical.ok_or_else(|| RotationError::ContentMissing {
                object_ref: object_ref.to_string(),
            })?;
            idp_id_in(to, domain_tag, bytes)
        }
    };
    Ok(IdMigration {
        old_id: id.to_string(),
        new_id,
        object_ref: object_ref.to_string(),
        method,
    })
}

/// `BridgeRecord{bridge_id, run_id, profile:{from,to},
/// rotations:[IdMigration], links_run_ids, prev_bridge_ref, at_ms}` — the
/// cross-profile resolution record (spec §5g.6 §2). Content-addressed by
/// `bridge_id` (`idp/1` over the record sans `bridge_id` — the address of
/// a bridge is itself an idp/1 id: the registry profile, not the rotated
/// one, names transition records; ADR-0307).
#[derive(Debug, Clone, PartialEq)]
pub struct BridgeRecord {
    /// The record's own id (`sha256:<hex>` over `unsigned_json`).
    pub bridge_id: String,
    /// The run the bridge binds.
    pub run_id: String,
    /// The source profile.
    pub from_idp: String,
    /// The target profile.
    pub to_idp: String,
    /// The id migrations the bridge records.
    pub rotations: Vec<IdMigration>,
    /// The runs linked across the rotation boundary.
    pub links_run_ids: Vec<String>,
    /// The prior bridge in the chain (`None` = the first).
    pub prev_bridge_ref: Option<String>,
    /// The record's mint stamp.
    pub at_ms: i64,
}

impl BridgeRecord {
    /// The unsigned body — the `bridge_id` preimage.
    fn unsigned(&self) -> Json {
        let mut m = vec![
            ("schema", Json::str("hh.bridge-record/1")),
            ("run_id", Json::str(self.run_id.clone())),
            ("from_idp", Json::str(self.from_idp.clone())),
            ("to_idp", Json::str(self.to_idp.clone())),
            (
                "rotations",
                Json::Arr(self.rotations.iter().map(IdMigration::to_json).collect()),
            ),
            (
                "links_run_ids",
                Json::Arr(
                    self.links_run_ids
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ),
            ("at_ms", Json::Int(self.at_ms)),
        ];
        if let Some(p) = &self.prev_bridge_ref {
            m.push(("prev_bridge_ref", Json::str(p.clone())));
        }
        Json::obj(m)
    }
    /// Compute/stamp `bridge_id` (`idp/1` over the unsigned body).
    pub fn seal(&mut self) {
        self.bridge_id = crate::idp::idp_id(
            "hh.bridge-record",
            self.unsigned().to_canonical_string().as_bytes(),
        );
    }
    /// Canonical JSON (`bridge_id` present once sealed).
    pub fn to_json(&self) -> Json {
        let mut m = self.unsigned();
        if let Json::Obj(ref mut mm) = m {
            mm.insert("bridge_id".into(), Json::str(self.bridge_id.clone()));
        }
        m
    }
    /// Decode.
    pub fn from_json(j: &Json) -> Option<BridgeRecord> {
        Some(BridgeRecord {
            bridge_id: j
                .get("bridge_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            run_id: j.get("run_id")?.as_str()?.to_string(),
            from_idp: j.get("from_idp")?.as_str()?.to_string(),
            to_idp: j.get("to_idp")?.as_str()?.to_string(),
            rotations: match j.get("rotations") {
                Some(Json::Arr(a)) => a.iter().filter_map(IdMigration::from_json).collect(),
                _ => Vec::new(),
            },
            links_run_ids: match j.get("links_run_ids") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            prev_bridge_ref: j
                .get("prev_bridge_ref")
                .and_then(Json::as_str)
                .map(String::from),
            at_ms: j.get("at_ms").and_then(Json::as_int).unwrap_or(0),
        })
    }
    /// Resolve `id` through the bridge: the migration row binding it, or
    /// `BridgeMissing` (spec §5g.6 §2 — cross-profile resolution without a
    /// bridge refuses, never guesses).
    pub fn resolve(&self, id: &str) -> Result<&IdMigration, RotationError> {
        self.rotations
            .iter()
            .find(|m| m.old_id == id || m.new_id == id)
            .ok_or_else(|| RotationError::BridgeMissing {
                object_ref: id.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idp::{address, address_in, IDP_1, IDP_2};

    fn plan() -> RotationPlan {
        RotationPlan {
            from_idp: "idp/1".into(),
            to_idp: "idp/2".into(),
            declared_at: "run-1".into(),
            reason: "post-incident rotation".into(),
            bridge: true,
            attestation_ref: Some("att:xyz".into()),
            effective_from_seq: Some(10),
            rehash: true,
        }
    }

    #[test]
    fn plan_validates_registered_distinct_profiles() {
        let (from, to) = plan().validate().unwrap();
        assert_eq!(from.idp_id, "idp/1");
        assert_eq!(to.idp_id, "idp/2");
        let mut bad = plan();
        bad.to_idp = "idp/9".into();
        assert!(matches!(
            bad.validate(),
            Err(RotationError::UnknownIdentityProfile { .. })
        ));
        let mut same = plan();
        same.to_idp = "idp/1".into();
        assert!(matches!(
            same.validate(),
            Err(RotationError::IdpNotWritable { .. })
        ));
    }

    #[test]
    fn migrate_id_rehash_recomputes_alias_carries() {
        let bytes = b"object bytes";
        let old = address(bytes, "text/plain").id();
        // rotate-full-rehash: the new id is idp/2 over the same bytes.
        let m = migrate_id(
            &old,
            &IDP_1,
            &IDP_2,
            &IDP_2,
            "blob",
            Some(bytes),
            "obj:1",
            MigrationMethod::RotateFullRehash,
        )
        .unwrap();
        assert_eq!(m.new_id, address_in(&IDP_2, bytes, "text/plain").id());
        // alias-id: the old spelling is carried verbatim.
        let m = migrate_id(
            &old,
            &IDP_1,
            &IDP_2,
            &IDP_2,
            "blob",
            None,
            "obj:1",
            MigrationMethod::AliasId,
        )
        .unwrap();
        assert_eq!(m.new_id, old);
        // A rehash without bytes refuses (nothing silently coerced).
        assert!(matches!(
            migrate_id(
                &old,
                &IDP_1,
                &IDP_2,
                &IDP_2,
                "blob",
                None,
                "obj:1",
                MigrationMethod::RotateFullRehash,
            ),
            Err(RotationError::ContentMissing { .. })
        ));
        // Minting under a non-writable profile refuses.
        assert!(matches!(
            migrate_id(
                &old,
                &IDP_1,
                &IDP_2,
                &IDP_1,
                "blob",
                Some(bytes),
                "obj:1",
                MigrationMethod::RotateFullRehash,
            ),
            Err(RotationError::IdpNotWritable { .. })
        ));
        // A malformed source id refuses.
        assert!(matches!(
            migrate_id(
                "not-an-id",
                &IDP_1,
                &IDP_2,
                &IDP_2,
                "blob",
                Some(bytes),
                "obj:1",
                MigrationMethod::AliasId,
            ),
            Err(RotationError::MalformedId { .. })
        ));
    }

    #[test]
    fn bridge_record_seals_resolves_and_refuses_unbridged() {
        let mut b = BridgeRecord {
            bridge_id: String::new(),
            run_id: "run-1".into(),
            from_idp: "idp/1".into(),
            to_idp: "idp/2".into(),
            rotations: vec![IdMigration {
                old_id: "sha256:abc".into(),
                new_id: "sha512:def".into(),
                object_ref: "obj:1".into(),
                method: MigrationMethod::AliasId,
            }],
            links_run_ids: vec!["run-1".into()],
            prev_bridge_ref: None,
            at_ms: 5,
        };
        b.seal();
        assert!(b.bridge_id.starts_with("sha256:"));
        // The codec round-trips.
        let decoded = BridgeRecord::from_json(&b.to_json()).unwrap();
        assert_eq!(decoded.bridge_id, b.bridge_id);
        assert_eq!(decoded.rotations.len(), 1);
        // Both directions resolve; an unlisted id is BridgeMissing.
        assert!(b.resolve("sha256:abc").is_ok());
        assert!(b.resolve("sha512:def").is_ok());
        assert!(matches!(
            b.resolve("sha256:unlisted"),
            Err(RotationError::BridgeMissing { .. })
        ));
    }
}
