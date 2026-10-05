//! The **identity profile `idp/1`** and the one content-addressing scheme (spec §8.3; ADR-0036).
//!
//! CC1 (one scheme per concern) **anchors here**: every id in the persistent tree — content
//! addresses, version ids, configuration ids and the `hh-embed/1` `schema_hash` — is produced by
//! this one construction over the one canonicalizer. The construction is
//! `H(idp ∥ domain_tag ∥ bytes)` where `H` is SHA-256 (`hh-wire::sha256`, NIST known-answer
//! tested) and `bytes` is the single canonical form (`hh-wire::json`, sorted-key compact). No
//! second hash primitive and no second canonicalization scheme exists after S1.2.
//!
//! `idp/1` requires a 256-bit standardized collision-resistant hash with streaming computation
//! and ubiquitous independent implementations (SHA-256 satisfies this; naming a cryptographic
//! standard is not an ecosystem commitment — ADR-0036 D2 / RK-09 / CC4).

use hh_wire::sha256::sha256_hex;
use hh_wire::sha512::sha512_hex;

use crate::kinds::RecordKind;

/// The identity-profile record (`{idp_id, hash_algorithm, digest_length, canonical_form_version,
/// tree_rule_version, id_text_form}`; §8.3 #3). `idp/1` is *bootstrapped*: it is hashed under
/// itself, and its bytes are also published verbatim. Exactly one idp is mandatory-writable at a
/// time (idp/1 here); all prior are readable; there is no default a consumer may assume (a
/// consumer that does not know an id's idp may pass it through but never verifies it silently as
/// ok — N2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityProfile {
    /// The profile id, e.g. `"idp/1"`. Hashed into every id for cross-profile domain separation.
    pub idp_id: &'static str,
    /// The hash algorithm, rendered as the id prefix (`<algorithm>:<hex>`; OCI grammar).
    pub hash_algorithm: &'static str,
    /// The digest length in hex characters (SHA-256 → 64).
    pub digest_length: usize,
    /// The canonical-form version this profile pins (the `hh-wire::json` sorted-key compact form).
    pub canonical_form_version: &'static str,
    /// The tree-rule version (`address` over directories; §8.3 #2, N8).
    pub tree_rule_version: &'static str,
    /// The id text form (`<algorithm>:<lowercase hex>`).
    pub id_text_form: &'static str,
}

/// The one mandatory-writable identity profile at genesis: **`idp/1`** (spec §8.3 #9 C0/Stage 1).
pub const IDP_1: IdentityProfile = IdentityProfile {
    idp_id: "idp/1",
    hash_algorithm: "sha256",
    digest_length: 64,
    canonical_form_version: "hh-json/1",
    tree_rule_version: "tree/1",
    id_text_form: "<algorithm>:<lowercase hex>",
};

/// The `idp/2` companion profile (spec §5g.6 §2, R-2.8.6; ADR-0307) — the
/// sha-512 digest over the same `hh-json/1` canonical form and the same
/// `idp ∥ domain ∥ bytes` framing. Ids minted under `idp/2` render
/// `sha512:<128 lowercase hex>`; they parse only under `idp/2` (the
/// profile is a parameter of verification, never coerced — N2).
pub const IDP_2: IdentityProfile = IdentityProfile {
    idp_id: "idp/2",
    hash_algorithm: "sha512",
    digest_length: 128,
    canonical_form_version: "hh-json/1",
    tree_rule_version: "tree/1",
    id_text_form: "<algorithm>:<lowercase hex>",
};

/// The registered profile catalogue — the closed set `profile_for`/
/// `parse_id_any` recognize (§5g.6 §2: profiles are named by their idp
/// id; a name outside this set is `UnknownIdentityProfile`, never a
/// silent default — N2).
pub const PROFILES: &[IdentityProfile] = &[IDP_1, IDP_2];

/// The registered profile record for an idp id (`idp/1`, `idp/2`), or
/// `None` — callers turn the `None` into `UnknownIdentityProfile`.
pub fn profile_for(idp_id: &str) -> Option<&'static IdentityProfile> {
    PROFILES.iter().find(|p| p.idp_id == idp_id)
}

/// The registered profile whose `hash_algorithm` is `algo` — the
/// id-text → profile dispatch (`sha256:` ⇒ idp/1, `sha512:` ⇒ idp/2).
/// `None` when no registered profile uses that algorithm tag.
pub fn profile_for_algorithm(algo: &str) -> Option<&'static IdentityProfile> {
    PROFILES.iter().find(|p| p.hash_algorithm == algo)
}

/// The unit separator that frames `idp ∥ domain_tag ∥ bytes`. Because neither `idp_id` nor any
/// [`RecordKind::domain_tag`] contains it, the framing is unambiguous — `idp/1 ∥ blob ∥ X` can
/// never equal `idp/1 ∥ tree ∥ Y`, which is what makes domain separation (N3) sound.
const SEP: u8 = 0x1f;

/// Errors from parsing or verifying an id (§8.3 #2/#5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    /// The id has no `<algorithm>:` tag, or a `:` but an empty/unknown algorithm (untagged id —
    /// the "8-hex filename" / git SHA-1 transition failure mode).
    AlgorithmMismatch { got: String },
    /// The digest is shorter/longer than the profile's `digest_length` (a truncated id — display
    /// abbreviations must never be stored; N1).
    TruncatedId { got_len: usize, want_len: usize },
    /// The digest length is right but a character is not lowercase hex.
    DigestLengthInvalid { detail: String },
    /// The idp named by the id is unknown to this consumer.
    UnknownIdentityProfile { idp: String },
}

/// The recomputation verdict for [`verify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// The id matches the recomputed digest.
    Ok,
    /// The id parses but does not match the content.
    Mismatch { expected: String, computed: String },
}

/// A parsed, well-formed id (`<algorithm>:<hex>`), after the N1/N2 format checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedId {
    pub algorithm: String,
    pub digest_hex: String,
}

/// The five-field content address (`{idp, algorithm, digest, media_type, size}`; §8.3 #3,
/// ADR-0036 D6). Raw bytes and trees are addressed; the `id()` renders `<algorithm>:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAddress {
    pub idp: &'static str,
    pub algorithm: &'static str,
    pub digest: String,
    pub media_type: String,
    pub size: u64,
}

impl ContentAddress {
    /// The rendered id text (`<algorithm>:<lowercase hex>`; N1 full digest, N2 self-describing).
    pub fn id(&self) -> String {
        format!("{}:{}", self.algorithm, self.digest)
    }
}

/// The core `idp/1` construction: the lowercase-hex SHA-256 of `idp_id ∥ SEP ∥ domain_tag ∥ SEP
/// ∥ payload`. This is the *single* content-addressing primitive (CC1); `address`, `identify`,
/// the configuration ids and the `schema_hash` all funnel through it.
pub fn idp_digest(domain_tag: &str, payload: &[u8]) -> String {
    sha256_hex(&frame(&IDP_1, domain_tag, payload))
}

/// The rendered id (`<algorithm>:<hex>`) over `domain_tag ∥ payload` under `idp/1`.
pub fn idp_id(domain_tag: &str, payload: &[u8]) -> String {
    format!(
        "{}:{}",
        IDP_1.hash_algorithm,
        idp_digest(domain_tag, payload)
    )
}

/// The framed preimage bytes — `idp ∥ SEP ∥ domain_tag ∥ SEP ∥ payload`.
/// One construction shared by every profile (CC1): profiles differ only
/// in the hash applied to this framing, never in the framing itself.
fn frame(profile: &IdentityProfile, domain_tag: &str, payload: &[u8]) -> Vec<u8> {
    debug_assert!(!domain_tag.as_bytes().contains(&SEP));
    let mut framed =
        Vec::with_capacity(profile.idp_id.len() + domain_tag.len() + payload.len() + 2);
    framed.extend_from_slice(profile.idp_id.as_bytes());
    framed.push(SEP);
    framed.extend_from_slice(domain_tag.as_bytes());
    framed.push(SEP);
    framed.extend_from_slice(payload);
    framed
}

/// Apply a profile's `hash_algorithm` to the framed bytes. `None` names
/// an algorithm no registered profile provides — a registry-level
/// inconsistency, which callers surface as `UnknownIdentityProfile` (the
/// catalogue is closed; this arm is unreachable through `PROFILES`).
fn digest_for_algorithm(algo: &str, framed: &[u8]) -> Option<String> {
    match algo {
        "sha256" => Some(sha256_hex(framed)),
        "sha512" => Some(sha512_hex(framed)),
        _ => None,
    }
}

/// The raw (unframed) digest of `bytes` under a profile's
/// `hash_algorithm` — e.g. the audit tree's `H("")` constant. `None` when
/// the profile's algorithm is unregistered (a catalogue bug, not an input
/// error — see [`digest_for_algorithm`]).
pub fn raw_digest(profile: &IdentityProfile, bytes: &[u8]) -> String {
    digest_for_algorithm(profile.hash_algorithm, bytes).unwrap_or_else(|| {
        panic!(
            "identity profile {} registers unknown hash algorithm {}",
            profile.idp_id, profile.hash_algorithm
        )
    })
}

/// The profile-parameterized construction (spec §5g.6 §2): the lowercase
/// hex of `profile.hash_algorithm(idp_id ∥ SEP ∥ domain_tag ∥ SEP ∥
/// payload)`. Returns `None` only if `profile` is not a registered
/// profile — construct it via [`profile_for`].
pub fn idp_digest_in(profile: &IdentityProfile, domain_tag: &str, payload: &[u8]) -> String {
    let framed = frame(profile, domain_tag, payload);
    digest_for_algorithm(profile.hash_algorithm, &framed).unwrap_or_else(|| {
        panic!(
            "identity profile {} registers unknown hash algorithm {}",
            profile.idp_id, profile.hash_algorithm
        )
    })
}

/// Convenience over [`idp_digest_in`] by profile name; `None` when
/// `idp_name` is not a registered profile (`UnknownIdentityProfile` is
/// the caller's refusal — never a silent default, N2).
pub fn digest_named(idp_name: &str, domain_tag: &str, payload: &[u8]) -> Option<String> {
    profile_for(idp_name).map(|p| idp_digest_in(p, domain_tag, payload))
}

/// The rendered id under an explicit profile (`<profile.algorithm>:<hex>`).
pub fn idp_id_in(profile: &IdentityProfile, domain_tag: &str, payload: &[u8]) -> String {
    format!(
        "{}:{}",
        profile.hash_algorithm,
        idp_digest_in(profile, domain_tag, payload)
    )
}

/// `address(bytes, media_type)` under an explicit profile — the same
/// `blob` domain, the profile's digest.
pub fn address_in(
    profile: &'static IdentityProfile,
    bytes: &[u8],
    media_type: impl Into<String>,
) -> ContentAddress {
    ContentAddress {
        idp: profile.idp_id,
        algorithm: profile.hash_algorithm,
        digest: idp_digest_in(profile, "blob", bytes),
        media_type: media_type.into(),
        size: bytes.len() as u64,
    }
}

/// `address(bytes, media_type) → ContentAddress` (§8.3 #2). Domain tag `blob`; streaming-safe
/// (SHA-256); idempotent; no timestamps or ownership. Foreign digests are recorded as claims,
/// never as identity (N8) — that is the caller's concern, not this function's.
pub fn address(bytes: &[u8], media_type: impl Into<String>) -> ContentAddress {
    ContentAddress {
        idp: IDP_1.idp_id,
        algorithm: IDP_1.hash_algorithm,
        digest: idp_digest("blob", bytes),
        media_type: media_type.into(),
        size: bytes.len() as u64,
    }
}

/// The version id of a `Text` leaf's content, under the `hir.text` domain (distinct from a blob
/// with equal bytes — N3; exercised by AC-2).
pub fn identify_text(content: &[u8]) -> String {
    idp_id(RecordKind::HirTextLeaf.domain_tag(), content)
}

/// The version id of a node under its record-kind domain, over the given canonical bytes. Used by
/// [`crate::record::identify`] and by AC-2's node arm.
pub fn identify_bytes(kind: RecordKind, canonical_bytes: &[u8]) -> String {
    idp_id(kind.domain_tag(), canonical_bytes)
}

/// Parse an id into `(algorithm, hex)` with the N1/N2 format checks under the
/// idp/1 profile. A missing tag, unknown algorithm, wrong length or non-hex
/// digit is rejected here so a consumer never treats a truncated or untagged
/// id as valid. `idp/2` ids (`sha512:<128 hex>`) refuse `AlgorithmMismatch`
/// here — parse them via [`parse_id_in`]/[`parse_id_any`].
pub fn parse_id(id: &str) -> Result<ParsedId, IdError> {
    parse_id_in(&IDP_1, id)
}

/// Parse an id under an explicit profile — the tag, length and alphabet
/// checks are the profile's (`sha512:` refuses under idp/1 and parses under
/// idp/2; §5g.6 §2's "ids never verify across profiles" — N2).
pub fn parse_id_in(profile: &IdentityProfile, id: &str) -> Result<ParsedId, IdError> {
    let (algo, digest) = match id.split_once(':') {
        Some((a, d)) if !a.is_empty() => (a, d),
        _ => {
            return Err(IdError::AlgorithmMismatch {
                got: id.to_string(),
            })
        }
    };
    if algo != profile.hash_algorithm {
        return Err(IdError::AlgorithmMismatch {
            got: algo.to_string(),
        });
    }
    if digest.len() != profile.digest_length {
        return Err(IdError::TruncatedId {
            got_len: digest.len(),
            want_len: profile.digest_length,
        });
    }
    if !digest
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(IdError::DigestLengthInvalid {
            detail: format!("non-lowercase-hex digit in {digest:?}"),
        });
    }
    Ok(ParsedId {
        algorithm: algo.to_string(),
        digest_hex: digest.to_string(),
    })
}

/// Parse an id under whichever registered profile its algorithm tag names —
/// the multi-profile entry point (§5g.6 §2's readable-set). An unknown tag
/// is `UnknownIdentityProfile` (never silently re-tagged).
pub fn parse_id_any(id: &str) -> Result<(&'static IdentityProfile, ParsedId), IdError> {
    let algo = match id.split_once(':') {
        Some((a, d)) if !a.is_empty() && !d.is_empty() => a,
        _ => {
            return Err(IdError::AlgorithmMismatch {
                got: id.to_string(),
            })
        }
    };
    let profile = profile_for_algorithm(algo).ok_or_else(|| IdError::UnknownIdentityProfile {
        idp: algo.to_string(),
    })?;
    let parsed = parse_id_in(profile, id)?;
    Ok((profile, parsed))
}

/// `verify(id, blob)` for a content address: recompute the blob address and compare. Format
/// errors (truncated/untagged id) are returned as [`IdError`]; a well-formed id that does not
/// match the bytes is a [`VerifyOutcome::Mismatch`] (never coerced to ok — N2).
pub fn verify_blob(id: &str, bytes: &[u8]) -> Result<VerifyOutcome, IdError> {
    let parsed = parse_id(id)?;
    let computed = idp_digest("blob", bytes);
    Ok(if parsed.digest_hex == computed {
        VerifyOutcome::Ok
    } else {
        VerifyOutcome::Mismatch {
            expected: parsed.digest_hex,
            computed,
        }
    })
}

/// `verify(id, canonical_bytes)` for a typed record kind under its domain tag.
pub fn verify_record(
    id: &str,
    kind: RecordKind,
    canonical_bytes: &[u8],
) -> Result<VerifyOutcome, IdError> {
    let parsed = parse_id(id)?;
    let computed = idp_digest(kind.domain_tag(), canonical_bytes);
    Ok(if parsed.digest_hex == computed {
        VerifyOutcome::Ok
    } else {
        VerifyOutcome::Mismatch {
            expected: parsed.digest_hex,
            computed,
        }
    })
}

/// `verify` under an explicit profile (the id must parse under that
/// profile — `parse_id_in` refusals propagate).
pub fn verify_blob_in(
    profile: &IdentityProfile,
    id: &str,
    bytes: &[u8],
) -> Result<VerifyOutcome, IdError> {
    let parsed = parse_id_in(profile, id)?;
    let computed = idp_digest_in(profile, "blob", bytes);
    Ok(if parsed.digest_hex == computed {
        VerifyOutcome::Ok
    } else {
        VerifyOutcome::Mismatch {
            expected: parsed.digest_hex,
            computed,
        }
    })
}

/// `verify_record` under an explicit profile.
pub fn verify_record_in(
    profile: &IdentityProfile,
    id: &str,
    kind: RecordKind,
    canonical_bytes: &[u8],
) -> Result<VerifyOutcome, IdError> {
    let parsed = parse_id_in(profile, id)?;
    let computed = idp_digest_in(profile, kind.domain_tag(), canonical_bytes);
    Ok(if parsed.digest_hex == computed {
        VerifyOutcome::Ok
    } else {
        VerifyOutcome::Mismatch {
            expected: parsed.digest_hex,
            computed,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idp_1_is_the_bootstrapped_sha256_profile() {
        assert_eq!(IDP_1.idp_id, "idp/1");
        assert_eq!(IDP_1.hash_algorithm, "sha256");
        assert_eq!(IDP_1.digest_length, 64);
    }

    #[test]
    fn ids_render_as_algorithm_colon_lowercase_hex() {
        // N1 (full digest) + N2 (self-describing) + AC-1 (parse).
        let ca = address(b"hello", "text/plain");
        let id = ca.id();
        assert!(id.starts_with("sha256:"));
        let parsed = parse_id(&id).unwrap();
        assert_eq!(parsed.algorithm, "sha256");
        assert_eq!(parsed.digest_hex.len(), 64);
        assert_eq!(ca.size, 5);
    }

    #[test]
    fn domain_separation_blob_text_node_over_equal_bytes() {
        // AC-2 / N3: a blob, a Text leaf and a node with identical bytes yield three distinct ids.
        let bytes = b"identical canonical bytes";
        let blob = address(bytes, "application/octet-stream").id();
        let text = identify_text(bytes);
        let node = identify_bytes(RecordKind::HirNode, bytes);
        assert_ne!(blob, text);
        assert_ne!(text, node);
        assert_ne!(blob, node);
        // …and each is a full, self-describing id.
        for id in [&blob, &text, &node] {
            assert!(parse_id(id).is_ok(), "not a valid id: {id}");
        }
    }

    #[test]
    fn verify_rejects_untagged_id_with_algorithm_mismatch() {
        // AC-1: an untagged id (no `<algorithm>:`) is rejected, never treated as ok.
        let untagged = "deadbeef".repeat(8); // 64 hex chars but no tag
        assert!(matches!(
            verify_blob(&untagged, b"x"),
            Err(IdError::AlgorithmMismatch { .. })
        ));
        // The git-SHA-1 transition failure mode: an unknown algorithm tag.
        assert!(matches!(
            verify_blob(&format!("sha1:{}", "a".repeat(64)), b"x"),
            Err(IdError::AlgorithmMismatch { .. })
        ));
    }

    #[test]
    fn verify_rejects_truncated_id() {
        // AC-1: the "8-hex filename" failure mode.
        let ca = address(b"payload", "text/plain");
        let full = ca.id();
        let truncated = &full[..full.len() - 40]; // chop the digest
        assert!(matches!(
            verify_blob(truncated, b"payload"),
            Err(IdError::TruncatedId { .. })
        ));
    }

    #[test]
    fn verify_ok_and_mismatch_are_distinguished() {
        let ca = address(b"payload", "text/plain");
        assert_eq!(
            verify_blob(&ca.id(), b"payload").unwrap(),
            VerifyOutcome::Ok
        );
        match verify_blob(&ca.id(), b"tampered").unwrap() {
            VerifyOutcome::Mismatch { .. } => {}
            VerifyOutcome::Ok => panic!("verify must not coerce a mismatch to ok (N2)"),
        }
    }

    #[test]
    fn idp_digest_is_deterministic_and_domain_framed() {
        assert_eq!(idp_digest("blob", b"x"), idp_digest("blob", b"x"));
        assert_ne!(idp_digest("blob", b"x"), idp_digest("tree", b"x"));
        // The framing is not naive concatenation: idp/1 ∥ "a" ∥ "bc" ≠ idp/1 ∥ "ab" ∥ "c".
        assert_ne!(idp_digest("a", b"bc"), idp_digest("ab", b"c"));
    }

    #[test]
    fn idp_2_is_registered_sha512_and_ids_are_profile_separated() {
        // §5g.6 §2: the profile is a parameter of the digest — equal bytes
        // under idp/1 and idp/2 produce different ids, and neither parses
        // under the other.
        assert_eq!(IDP_2.idp_id, "idp/2");
        assert_eq!(IDP_2.hash_algorithm, "sha512");
        assert_eq!(IDP_2.digest_length, 128);
        assert_eq!(profile_for("idp/2").map(|p| p.idp_id), Some("idp/2"));
        assert!(profile_for("idp/9").is_none());

        let ca1 = address(b"hello", "text/plain");
        let ca2 = address_in(&IDP_2, b"hello", "text/plain");
        assert!(ca2.id().starts_with("sha512:"));
        assert_eq!(ca2.id().len(), "sha512:".len() + 128);
        assert_ne!(ca1.id(), ca2.id());
    }

    #[test]
    fn parse_dispatches_by_profile_and_never_across() {
        let id1 = address(b"x", "text/plain").id();
        let id2 = address_in(&IDP_2, b"x", "text/plain").id();
        // Each parses under its own profile…
        assert!(parse_id_in(&IDP_1, &id1).is_ok());
        assert!(parse_id_in(&IDP_2, &id2).is_ok());
        // …and refuses under the other (the algorithm tag is checked
        // first — a sha256 id under idp/2 is an AlgorithmMismatch before
        // any length check).
        assert!(matches!(
            parse_id_in(&IDP_1, &id2),
            Err(IdError::AlgorithmMismatch { .. })
        ));
        assert!(matches!(
            parse_id_in(&IDP_2, &id1),
            Err(IdError::AlgorithmMismatch { .. })
        ));
        // `parse_id_any` dispatches on the algorithm tag.
        assert_eq!(parse_id_any(&id1).unwrap().0.idp_id, "idp/1");
        assert_eq!(parse_id_any(&id2).unwrap().0.idp_id, "idp/2");
        assert!(matches!(
            parse_id_any(&format!("sha1:{}", "a".repeat(40))),
            Err(IdError::UnknownIdentityProfile { .. })
        ));
    }

    #[test]
    fn verify_in_profile_ok_and_mismatch() {
        let bytes = b"payload";
        let id2 = address_in(&IDP_2, bytes, "text/plain").id();
        assert_eq!(
            verify_blob_in(&IDP_2, &id2, bytes).unwrap(),
            VerifyOutcome::Ok
        );
        assert!(matches!(
            verify_blob_in(&IDP_2, &id2, b"other").unwrap(),
            VerifyOutcome::Mismatch { .. }
        ));
        // Never verifies across profiles (N2).
        assert!(verify_blob_in(&IDP_1, &id2, bytes).is_err());
    }
}
