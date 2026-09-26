//! `attest(bundle, attestation)` — the signed statement surface (§5h.3
//! §2; ADR-0141 D2; S4.2). An attestation binds `{bundle_id, statement,
//! instrument, issued_ms}`; when it carries `sig` the signature is the
//! C0 `hmac-sha256` construction over the canonical unsigned statement
//! — the same seam `security.audit.checkpoint` uses (`key_id` resolved
//! through [`hh_ledger::audit::AuditKeyResolver`]; an unresolvable or
//! mismatched signature refuses `ProvenanceInvalid`, never passes —
//! CC2/CC4).

use hh_ledger::audit::{parse_sig, render_sig, AuditKeyResolver, CHECKPOINT_ALG};
use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256;

use crate::codec::Decoded;
use crate::error::BundleError;

/// The attestation document shape (`hh-bundle-attestation/1`).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleAttestation {
    /// The bundle the statement binds (`manifest.version_id`).
    pub bundle_id: String,
    /// The attested statement (free-form predicate document).
    pub statement: Json,
    /// The signing instrument (`participant_id` of the signer).
    pub instrument: String,
    /// The `key_id` the signature resolves under.
    pub key_id: Option<String>,
    /// The `hmac-sha256:<hex>` signature (absent = unsigned statement —
    /// admitted as a *statement*, never as a verified claim).
    pub sig: Option<String>,
    /// The issue stamp.
    pub issued_ms: i64,
}

impl BundleAttestation {
    /// The canonical unsigned statement document — the signature
    /// preimage (CC1: signer and verifier compute identical bytes).
    pub fn unsigned(&self) -> Json {
        Json::obj([
            ("bundle_id", Json::str(self.bundle_id.clone())),
            ("statement", self.statement.clone()),
            ("instrument", Json::str(self.instrument.clone())),
            ("issued_ms", Json::Int(self.issued_ms)),
        ])
    }

    /// The preimage bytes `sig` covers.
    pub fn preimage(&self) -> Vec<u8> {
        self.unsigned().to_canonical_string().into_bytes()
    }

    /// Canonical document (`sig`/`key_id` present only when signed).
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("schema".into(), Json::str("hh-bundle-attestation/1"));
        m.insert("bundle_id".into(), Json::str(self.bundle_id.clone()));
        m.insert("statement".into(), self.statement.clone());
        m.insert("instrument".into(), Json::str(self.instrument.clone()));
        m.insert("issued_ms".into(), Json::Int(self.issued_ms));
        if let Some(k) = &self.key_id {
            m.insert("key_id".into(), Json::str(k.clone()));
        }
        if let Some(s) = &self.sig {
            m.insert("alg".into(), Json::str(CHECKPOINT_ALG));
            m.insert("sig".into(), Json::str(s.clone()));
        }
        Json::Obj(m)
    }

    /// Decode.
    pub fn from_json(j: &Json) -> Result<BundleAttestation, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("attestation: {d}"),
        };
        Ok(BundleAttestation {
            bundle_id: j
                .get("bundle_id")
                .and_then(Json::as_str)
                .ok_or(bad("bundle_id"))?
                .to_string(),
            statement: j.get("statement").cloned().ok_or(bad("statement"))?,
            instrument: j
                .get("instrument")
                .and_then(Json::as_str)
                .ok_or(bad("instrument"))?
                .to_string(),
            key_id: j.get("key_id").and_then(Json::as_str).map(String::from),
            sig: j.get("sig").and_then(Json::as_str).map(String::from),
            issued_ms: j.get("issued_ms").and_then(Json::as_int).unwrap_or(0),
        })
    }
}

/// `attest(decoded, attestation, keys)` — bind-check and (when signed)
/// signature-check the statement against the bundle under the caller's
/// key table; the returned document is what the caller persists (the
/// results catalogue's attestation index / a `bundle.attested` kernel
/// event). An unsigned attestation is admitted as a *statement*; a
/// signed one must verify — an unheld `key_id` is `ProvenanceInvalid`,
/// never a pass (CC4).
pub fn attest(
    decoded: &Decoded,
    attestation: &BundleAttestation,
    keys: &dyn AuditKeyResolver,
) -> Result<Json, BundleError> {
    if attestation.bundle_id != decoded.manifest.version_id {
        return Err(BundleError::ProvenanceInvalid {
            detail: format!(
                "attestation binds `{}` but the bundle is `{}`",
                attestation.bundle_id, decoded.manifest.version_id
            ),
        });
    }
    if attestation.instrument.is_empty() {
        return Err(BundleError::ProvenanceInvalid {
            detail: "attestation has no instrument".into(),
        });
    }
    if let Some(sig) = &attestation.sig {
        let key_id = attestation
            .key_id
            .clone()
            .ok_or(BundleError::ProvenanceInvalid {
                detail: "signed attestation has no key_id".into(),
            })?;
        let sig_bytes = parse_sig(sig).ok_or(BundleError::ProvenanceInvalid {
            detail: format!("sig is not {CHECKPOINT_ALG}:<hex>"),
        })?;
        let key = keys
            .verify_key(&key_id)
            .ok_or(BundleError::ProvenanceInvalid {
                detail: format!("signer key `{key_id}` is not held"),
            })?;
        if hmac_sha256(&key, &attestation.preimage()).to_vec() != sig_bytes {
            return Err(BundleError::ProvenanceInvalid {
                detail: format!("signature does not verify under key `{key_id}`"),
            });
        }
    }
    Ok(attestation.to_json())
}

/// Mint a signed attestation (producer-side helper — the same binding
/// the verifier checks).
pub fn sign_attestation(
    bundle_id: &str,
    statement: Json,
    instrument: &str,
    key_id: &str,
    key: &[u8],
    issued_ms: i64,
) -> BundleAttestation {
    let mut a = BundleAttestation {
        bundle_id: bundle_id.to_string(),
        statement,
        instrument: instrument.to_string(),
        key_id: Some(key_id.to_string()),
        sig: None,
        issued_ms,
    };
    a.sig = Some(render_sig(&hmac_sha256(key, &a.preimage())));
    a
}
