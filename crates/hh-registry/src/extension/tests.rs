//! §5g.5 C0 unit tests — every gating behavior gets a removal-sensitive test
//! (AC-R-2.8.5-1/-2/-11 land here; L4's sealed-form walk lands in hh-assembly).

use super::*;
use hh_provenance::origin::{HumanRole, Origin};

fn human() -> Origin {
    Origin::Human {
        author_ref: "principal-1".to_string(),
        role: HumanRole::Principal,
    }
}

fn model() -> Origin {
    Origin::Model {
        model_ref: "model-m".to_string(),
        run_ref: "run-1".to_string(),
        response_id: "resp-1".to_string(),
    }
}

fn base_record(text_authority: AuthorityClass, status: AttestationStatus) -> ExtensionRecord {
    ExtensionRecord {
        kind: ExtensionKind::Skill,
        name: "demo-skill".to_string(),
        content: idp::address(b"payload", "application/octet-stream"),
        manifest: Json::obj([]),
        contributes: Vec::new(),
        locator: SourceLocator {
            scheme: "git".to_string(),
            credential_free_uri: "https://example.com/repo.git".to_string(),
            selector: None,
            resolved: Some("abc123".to_string()),
            fetched_at: Some(7),
        },
        trust: ExtensionTrustRecord {
            text_authority,
            code_identity: Vec::new(),
            isolation: IsolationClass::InProcess,
            grants: Vec::new(),
            declared_claims: Vec::new(),
            attestations: Vec::new(),
            attestation_status: status,
            scans: Vec::new(),
            text_hygiene: TextHygieneReport {
                status: HygieneStatus::Clean,
                findings: Vec::new(),
            },
            surface_pin: None,
            installed_by: None,
            installed_at: None,
            scope: PersistenceScope::Run,
            status: TrustStatus::Resolved,
            review_ref: None,
        },
        provenance: ProvenanceRecord::minted(human(), PersistenceScope::Run, 7),
        ext: BTreeMap::new(),
    }
}

// ── AC-R-2.8.5-2: default_text_authority location neutrality ─────────────────

/// The minting table is a pure function of (kind, origin, attestation_status).
/// Locator/scope/directory/marketplace/"trusted folder" facts are *not inputs* —
/// this property is the L1 spec: no location fact can raise text authority.
#[test]
fn ac_2_8_5_2_location_neutrality() {
    for kind in [
        ExtensionKind::Skill,
        ExtensionKind::Hook,
        ExtensionKind::Plugin,
        ExtensionKind::McpServer,
        ExtensionKind::InstructionFile,
        ExtensionKind::FetchedInstruction,
        ExtensionKind::ComponentVariant,
    ] {
        for origin in [human(), model()] {
            for status in [
                AttestationStatus::Missing,
                AttestationStatus::Failed,
                AttestationStatus::Verified(AttestationKind::Pin),
                AttestationStatus::Verified(AttestationKind::HashChain),
                AttestationStatus::Verified(AttestationKind::Seal),
                AttestationStatus::Verified(AttestationKind::Signature),
            ] {
                // The function is total: it must return for every triple.
                let a = default_text_authority(&kind, &origin, &status);
                // And it never exceeds the provenance minting table's ceiling
                // for the same origin (external text never becomes
                // environment/delegate/kernel).
                assert!(
                    matches!(
                        a,
                        AuthorityClass::Unverified
                            | AuthorityClass::External
                            | AuthorityClass::Definition
                            | AuthorityClass::Principal
                    ),
                    "kind {:?} origin {:?} status {:?} minted {:?}",
                    kind,
                    origin,
                    status,
                    a
                );
            }
        }
    }
}

/// Skill text from a project directory stays ≤ external while delivered —
/// the "trusted folder" carve-out is forbidden (L1).
#[test]
fn ac_2_8_5_2_project_directory_skill_stays_external() {
    // A skill discovered in a `project`-scope directory_scan, authored by a
    // human, with no attestation → external (the floor), never definition.
    let a = default_text_authority(&ExtensionKind::Skill, &human(), &AttestationStatus::Missing);
    assert_eq!(a, AuthorityClass::External);
    // Same with a model origin → still external floor ∩ delegate R_text.
    let a2 = default_text_authority(&ExtensionKind::Skill, &model(), &AttestationStatus::Missing);
    assert_eq!(a2, AuthorityClass::External);
}

/// MCP server text: missing attestation → unverified (server outside the
/// sealed definition); sealed → external (open-world carve-out).
#[test]
fn mcp_server_text_floor() {
    assert_eq!(
        default_text_authority(
            &ExtensionKind::McpServer,
            &human(),
            &AttestationStatus::Missing
        ),
        AuthorityClass::Unverified
    );
    assert_eq!(
        default_text_authority(
            &ExtensionKind::McpServer,
            &human(),
            &AttestationStatus::Verified(AttestationKind::Seal),
        ),
        AuthorityClass::External
    );
    // A verified *signature* is the only route to definition.
    assert_eq!(
        default_text_authority(
            &ExtensionKind::McpServer,
            &human(),
            &AttestationStatus::Verified(AttestationKind::Signature),
        ),
        AuthorityClass::Definition
    );
}

/// A failed verification lowers, never launders.
#[test]
fn failed_attestation_means_unverified() {
    for kind in [ExtensionKind::Skill, ExtensionKind::McpServer] {
        assert_eq!(
            default_text_authority(&kind, &human(), &AttestationStatus::Failed),
            AuthorityClass::Unverified
        );
    }
}

/// Hash pins cap at external — integrity, never authority.
#[test]
fn hash_pin_ceiling() {
    for k in [AttestationKind::Pin, AttestationKind::HashChain] {
        assert!(
            default_text_authority(
                &ExtensionKind::Skill,
                &human(),
                &AttestationStatus::Verified(k)
            ) <= AuthorityClass::External
        );
    }
}

// ── AC-R-2.8.5-11: TextHygieneReport advisory, bytes preserved ────────────────

#[test]
fn ac_2_8_5_11_bidi_and_invisible_flagged() {
    // RLO + ZWSP + a tag char embedded in otherwise normal text.
    let bytes = "normal text \u{202E}reversed\u{202C} and\u{200B}zero\u{E0041}tag".as_bytes();
    let report = scan_text_hygiene(bytes);
    assert_eq!(report.status, HygieneStatus::Flagged);
    assert!(!report.findings.is_empty());
    let kinds: Vec<HygieneFindingKind> = report.findings.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&HygieneFindingKind::BidirectionalControl));
    assert!(kinds.contains(&HygieneFindingKind::TagCodepoint));
    // Advisory: the report points at offsets; the input is untouched (bytes
    // preserved — the function takes &[u8] and returns findings only).
    assert_eq!(
        String::from_utf8_lossy(bytes),
        "normal text \u{202E}reversed\u{202C} and\u{200B}zero\u{E0041}tag"
    );
}

#[test]
fn clean_text_reports_clean() {
    let report = scan_text_hygiene("plain readable text, no tricks".as_bytes());
    assert_eq!(report.status, HygieneStatus::Clean);
    assert!(report.findings.is_empty());
}

#[test]
fn non_utf8_reports_unavailable_not_clean() {
    let report = scan_text_hygiene(&[0xFF, 0xFE, 0x00, 0x80]);
    assert_eq!(report.status, HygieneStatus::Unavailable);
}

// ── L1: LocationElevation at validate ────────────────────────────────────────

#[test]
fn l1_rejects_text_authority_above_minted() {
    // Skill, human origin, missing attestation → minted external; a record
    // carrying `definition` is a location elevation.
    let mut r = base_record(AuthorityClass::Definition, AttestationStatus::Missing);
    r.trust.text_authority = AuthorityClass::Definition;
    assert!(matches!(
        validate_extension_record(&r),
        Err(RegistryError::SchemaViolation { path, .. }) if path.contains("text_authority")
    ));
}

#[test]
fn l1_rejects_quiet_under_minting() {
    // Under-minting is also refused (determinism — two resolvers must mint
    // the same record).
    let r = base_record(AuthorityClass::Unverified, AttestationStatus::Missing);
    assert!(validate_extension_record(&r).is_err());
}

#[test]
fn l1_accepts_minted_value() {
    let r = base_record(AuthorityClass::External, AttestationStatus::Missing);
    assert!(validate_extension_record(&r).is_ok());
}

// ── L4: pinned-form test ─────────────────────────────────────────────────────

#[test]
fn l4_extension_ref_pinned_gate() {
    let pinned = ExtensionRef {
        name: "s".to_string(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "git".to_string(),
            credential_free_uri: "u".to_string(),
            selector: None,
            resolved: Some("sha".to_string()),
            fetched_at: Some(1),
        },
        content: Some(idp::address(b"x", "application/octet-stream")),
        extension_id: Some("v".to_string()),
    };
    assert!(pinned.is_pinned());
    // A surviving selector → unpinned.
    let mut unpinned = pinned.clone();
    unpinned.locator.selector = Some("latest".to_string());
    assert!(!unpinned.is_pinned());
    // Missing content → unpinned.
    let mut no_content = pinned.clone();
    no_content.content = None;
    assert!(!no_content.is_pinned());
    // Missing resolved → unpinned.
    let mut no_resolved = pinned;
    no_resolved.locator.resolved = None;
    assert!(!no_resolved.is_pinned());
}

// ── Credential-free locator ──────────────────────────────────────────────────

#[test]
fn credential_in_uri_refused() {
    let mut r = base_record(AuthorityClass::External, AttestationStatus::Missing);
    r.locator.credential_free_uri = "https://user:token@example.com/repo".to_string();
    assert!(matches!(
        validate_extension_record(&r),
        Err(RegistryError::SchemaViolation { path, .. }) if path.contains("credential_free_uri")
    ));
    r.locator.credential_free_uri = "https://example.com/repo?token=abc".to_string();
    assert!(validate_extension_record(&r).is_err());
}

// ── Verified status requires a matching attestation member ───────────────────

#[test]
fn verified_status_requires_attestation_member() {
    let mut r = base_record(
        AuthorityClass::Definition,
        AttestationStatus::Verified(AttestationKind::Signature),
    );
    // text_authority minted to definition matches, but attestations[] is
    // empty → the status is a parallel claim, refused.
    assert!(matches!(
        validate_extension_record(&r),
        Err(RegistryError::SchemaViolation { path, .. }) if path.contains("attestation_status")
    ));
    r.trust.attestations.push(Attestation {
        kind: AttestationKind::Signature,
        subject_hash: "h".to_string(),
        anchor: AttestationAnchor::Signer("root-1".to_string()),
        verified_by: "kernel:registry".to_string(),
        verified_at: 9,
    });
    assert!(validate_extension_record(&r).is_ok());
}

// ── L3: a claim carrying a pin shape pretends to be a grant ──────────────────

#[test]
fn l3_claim_with_grant_shape_refused() {
    let mut r = base_record(AuthorityClass::External, AttestationStatus::Missing);
    r.trust.declared_claims.push(DeclaredClaim {
        kind: DeclaredClaimKind::PermissionManifest,
        value: Json::obj([("version_id", Json::str("v-fake"))]),
    });
    assert!(matches!(
        validate_extension_record(&r),
        Err(RegistryError::SchemaViolation { detail, .. }) if detail.contains("LegCrossing")
    ));
}

// ── Codec round-trip ─────────────────────────────────────────────────────────

#[test]
fn extension_record_json_roundtrip() {
    let r = base_record(AuthorityClass::External, AttestationStatus::Missing);
    let j = extension_body_json(&r);
    let back = extension_from_json(&j, "extension").expect("decode");
    assert_eq!(back.kind.as_str(), r.kind.as_str());
    assert_eq!(back.name, r.name);
    assert_eq!(back.content.digest, r.content.digest);
    assert_eq!(back.trust.text_authority, r.trust.text_authority);
    assert_eq!(back.trust.status, r.trust.status);
}

#[test]
fn declared_source_roundtrip() {
    for s in [
        DeclaredSource::DirectoryScan {
            root: "~/.hh/skills".to_string(),
            scope: PersistenceScope::User,
        },
        DeclaredSource::Marketplace {
            catalog_ref: "cat-1".to_string(),
        },
        DeclaredSource::Registry {
            name: "local".to_string(),
        },
        DeclaredSource::Git {
            url: "https://example.com/r.git".to_string(),
            ref_: "main".to_string(),
        },
        DeclaredSource::Archive {
            url: "https://example.com/a.tgz".to_string(),
            sha: "abc".to_string(),
        },
        DeclaredSource::McpEndpoint {
            uri: "https://mcp.example.com".to_string(),
        },
        DeclaredSource::InstructionFiles {
            roots: vec!["./CLAUDE.md".to_string()],
        },
    ] {
        let j = declared_source_json(&s);
        let back = declared_source_from_json(&j, "source").expect("decode");
        assert_eq!(back.kind_str(), s.kind_str());
    }
}

// ── resolve_candidate ─────────────────────────────────────────────────────────

#[test]
fn resolve_candidate_fills_pins_and_mints_authority() {
    let candidate = Candidate {
        name: "skill-x".to_string(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "git".to_string(),
            credential_free_uri: "https://example.com/r.git".to_string(),
            selector: Some("v1.*".to_string()),
            resolved: None,
            fetched_at: None,
        },
        source: DeclaredSource::Git {
            url: "https://example.com/r.git".to_string(),
            ref_: "main".to_string(),
        },
    };
    let outcome = FetchOutcome {
        resolved: "deadbeef".to_string(),
        content: idp::address(b"payload", "application/octet-stream"),
        fetched_at: 42,
        code_identity: vec!["sha256:deadbeef".to_string()],
        source_snapshot: Some("snap-1".to_string()),
        surface_pin: None,
    };
    let rec = resolve_candidate(&candidate, &outcome, human(), Some(b"clean text"), 42);
    // Pins filled, selector consumed.
    assert!(rec.locator.is_pinned());
    assert!(rec.locator.selector.is_none());
    assert_eq!(rec.locator.resolved.as_deref(), Some("deadbeef"));
    // Minted authority (skill + human + missing → external).
    assert_eq!(rec.trust.text_authority, AuthorityClass::External);
    assert_eq!(rec.trust.status, TrustStatus::Resolved);
    assert_eq!(rec.trust.text_hygiene.status, HygieneStatus::Clean);
    // The record validates against its own L1–L3 gate.
    assert!(validate_extension_record(&rec).is_ok());
}
