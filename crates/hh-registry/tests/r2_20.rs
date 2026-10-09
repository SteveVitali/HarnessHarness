//! R2.20 — extension-trust producers (DF-S1.23-2 residual; BL-28; R-2.8.5,
//! R-2.12.2; spec §5g.5). The halves the emitter slice (S4.14a) left open:
//!
//! - `DeclaredSource` scanners produce `ExtensionRef`s — the
//!   `discover(sources, policy)` enumeration leg: allowlist-gated under the
//!   live `TrustView`, contained under declared roots (`ScanDenied`),
//!   credential-free locators, `merge_policy` collisions.
//! - `LegCrossing`/L3 runtime checks in resolve/install — the manifest
//!   claim lift (`allowed-tools`, tool annotations, permission manifests →
//!   `declared_claims`), grant/pin-shaped claim values refuse `LegCrossing`,
//!   and a delegate-origin record carrying conferred grants refuses (L2).
//! - `TextHygieneReport` consumers — `scanner_policy` `advisory |
//!   quarantine | deny` acts on the minted report.
//! - The `security.extension.{resolved, quarantined, sealed, loaded}`
//!   producers (`ResolveReport.events`, `seal_extension`,
//!   `activate_extension`).

use std::collections::BTreeSet;

use hh_identity::idp;
use hh_identity::kinds::RecordKind;
use hh_identity::refs::VersionedRef;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::origin::HumanRole;
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::errors::RegistryError;
use hh_registry::extension::lifecycle::{
    self, ProposerContext, ScanEntry, ScannerPolicy, SurfaceDriftPolicy, TrustView,
};
use hh_registry::extension::{
    DeclaredSource, ExtensionKind, ExtensionRecord, FetchOutcome, MergePolicy, SourceLocator,
    TrustStatus,
};
use hh_wire::json::Json;

// ── fixtures ────────────────────────────────────────────────────────────────

fn view() -> TrustView {
    TrustView {
        allowed_sources: BTreeSet::new(),
        accepted_signers: BTreeSet::new(),
        required_predicates: BTreeSet::new(),
        require_signature_for: BTreeSet::new(),
        hash_only_ceiling: AuthorityClass::External,
        max_age: None,
        model_install: hh_registry::records::ModelInstallRule::AllowAttenuated,
        scanner_policy: ScannerPolicy::Advisory,
        live_policy_ids: vec![],
    }
}

fn human() -> Origin {
    Origin::human("alice", HumanRole::Principal)
}

fn human_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(human(), PersistenceScope::Run, 0)
}

fn dir_source() -> DeclaredSource {
    DeclaredSource::DirectoryScan {
        root: "vendor/skills".to_string(),
        scope: PersistenceScope::Run,
    }
}

fn entry(name: &str, kind: ExtensionKind, member: Json) -> ScanEntry {
    ScanEntry {
        name: name.to_string(),
        kind,
        member,
    }
}

fn candidate(name: &str) -> hh_registry::extension::Candidate {
    hh_registry::extension::Candidate {
        name: name.into(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "git".into(),
            credential_free_uri: "https://example.com/plugins/demo.git".into(),
            selector: Some("main".into()),
            resolved: None,
            fetched_at: None,
        },
        source: DeclaredSource::Git {
            url: "https://example.com/plugins/demo.git".into(),
            ref_: "main".into(),
        },
    }
}

fn outcome(payload: &[u8], at: u64) -> FetchOutcome {
    FetchOutcome {
        resolved: "git:demo@abc123".into(),
        content: idp::address(payload, "application/octet-stream"),
        fetched_at: at,
        code_identity: vec![],
        source_snapshot: None,
        surface_pin: None,
    }
}

fn str_items<'a>(e: &'a Json, key: &str) -> Vec<&'a str> {
    match e.get(key) {
        Some(Json::Arr(items)) => items.iter().filter_map(Json::as_str).collect(),
        _ => vec![],
    }
}

fn classes(report: &lifecycle::ResolveReport) -> Vec<String> {
    report
        .events
        .iter()
        .filter_map(|e| e.get("class").and_then(Json::as_str).map(str::to_string))
        .collect()
}

fn proposer() -> ProposerContext {
    ProposerContext {
        provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp:1"),
            PersistenceScope::Run,
            0,
        ),
        grants: BTreeSet::from(["fs.read:*".to_string()]),
    }
}

fn grant(vid: &str) -> VersionedRef {
    VersionedRef::pinned(RecordKind::PermissionPolicy, vid, human_prov())
}

// ── scanners produce ExtensionRefs ─────────────────────────────────────────

/// `scan_declared_sources` enumerates a directory listing into refs: the
/// locator joins the declared root, the scheme is the source kind (so the
/// assembly's declared-source coverage check sees it), and nothing is
/// pinned — resolve pins, the scanner only enumerates.
#[test]
fn scan_directory_source_produces_refs() {
    let scans = vec![(
        dir_source(),
        vec![
            entry(
                "lint",
                ExtensionKind::Skill,
                Json::obj([("path", Json::str("lint"))]),
            ),
            entry(
                "fmt",
                ExtensionKind::Hook,
                Json::obj([("path", Json::str("hooks/fmt.sh"))]),
            ),
        ],
    )];
    let refs = lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly).unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].name, "lint");
    assert_eq!(refs[0].locator.scheme, "directory_scan");
    assert_eq!(refs[0].locator.credential_free_uri, "vendor/skills/lint");
    assert_eq!(
        refs[1].locator.credential_free_uri,
        "vendor/skills/hooks/fmt.sh"
    );
    assert!(!refs[0].is_pinned(), "scan enumerates; resolve pins");
    assert_eq!(refs[0].extension_id, None);
}

/// The remote/endpoint source kinds produce their honest locator spellings:
/// git carries the declared `ref` as the selector, archive the declared
/// `sha`, registry/marketplace/mcp_endpoint derive the member URI.
#[test]
fn scan_remote_sources_carry_source_coordinates() {
    let scans = vec![
        (
            DeclaredSource::Git {
                url: "https://git.example/x.git".into(),
                ref_: "v1".into(),
            },
            vec![entry("g", ExtensionKind::Plugin, Json::obj([]))],
        ),
        (
            DeclaredSource::Archive {
                url: "https://cdn.example/a.tar".into(),
                sha: "sha256:aa".into(),
            },
            vec![entry(
                "a",
                ExtensionKind::Plugin,
                Json::obj([("path", Json::str("bin/x"))]),
            )],
        ),
        (
            DeclaredSource::Registry {
                name: "corp".into(),
            },
            vec![entry("r", ExtensionKind::Skill, Json::obj([]))],
        ),
        (
            DeclaredSource::Marketplace {
                catalog_ref: "mkt://catalog".into(),
            },
            vec![entry("m", ExtensionKind::Skill, Json::obj([]))],
        ),
        (
            DeclaredSource::McpEndpoint {
                uri: "https://mcp.example/svc".into(),
            },
            vec![entry("srv", ExtensionKind::McpServer, Json::obj([]))],
        ),
    ];
    let refs = lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly).unwrap();
    assert_eq!(refs.len(), 5);
    assert_eq!(
        refs[0].locator.credential_free_uri,
        "https://git.example/x.git"
    );
    assert_eq!(refs[0].locator.selector.as_deref(), Some("v1"));
    assert_eq!(
        refs[1].locator.credential_free_uri,
        "https://cdn.example/a.tar#bin/x"
    );
    assert_eq!(refs[1].locator.selector.as_deref(), Some("sha256:aa"));
    assert_eq!(refs[2].locator.credential_free_uri, "corp/r");
    assert_eq!(refs[3].locator.credential_free_uri, "mkt://catalog/m");
    assert_eq!(
        refs[4].locator.credential_free_uri,
        "https://mcp.example/svc/srv"
    );
}

/// The allowlist gates the *source*, not the ref — a source kind outside
/// the live `allowed_sources` union refuses `SourceNotAllowed`.
#[test]
fn scan_source_allowlist_gates() {
    let mut v = view();
    v.allowed_sources = BTreeSet::from(["marketplace".to_string()]);
    let scans = vec![(
        dir_source(),
        vec![entry(
            "x",
            ExtensionKind::Skill,
            Json::obj([("path", Json::str("x"))]),
        )],
    )];
    assert!(matches!(
        lifecycle::scan_declared_sources(&scans, &v, MergePolicy::ExactOnly),
        Err(RegistryError::SourceNotAllowed { .. })
    ));
    // An admitted kind passes.
    let scans = vec![(
        DeclaredSource::Marketplace {
            catalog_ref: "mkt://c".into(),
        },
        vec![entry("x", ExtensionKind::Skill, Json::obj([]))],
    )];
    assert_eq!(
        lifecycle::scan_declared_sources(&scans, &v, MergePolicy::ExactOnly)
            .unwrap()
            .len(),
        1
    );
}

/// Containment: a `..`/absolute path escaping the declared `directory_scan`
/// root is `ScanDenied`; an instruction file outside its declared `roots`
/// is `ScanDenied`. Nothing outside the declaration is ever reached.
#[test]
fn scan_containment_denies_escapes() {
    let escapes = [
        Json::obj([("path", Json::str("../escape"))]),
        Json::obj([("path", Json::str("/etc/passwd"))]),
        Json::obj([("path", Json::str("a/../../escape"))]),
    ];
    for m in escapes {
        let scans = vec![(
            dir_source(),
            vec![entry("e", ExtensionKind::Skill, m.clone())],
        )];
        assert!(
            matches!(
                lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly),
                Err(RegistryError::ScanDenied { .. })
            ),
            "escape member refused: {m:?}"
        );
    }
    // instruction_files — the member path must live under a declared root.
    let ifs = DeclaredSource::InstructionFiles {
        roots: vec!["project".into(), "home/.claude".into()],
    };
    let scans = vec![(
        ifs.clone(),
        vec![entry(
            "agents",
            ExtensionKind::InstructionFile,
            Json::obj([("path", Json::str("project/AGENTS.md"))]),
        )],
    )];
    assert_eq!(
        lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly).unwrap()[0]
            .locator
            .credential_free_uri,
        "project/AGENTS.md"
    );
    let scans = vec![(
        ifs,
        vec![entry(
            "stray",
            ExtensionKind::InstructionFile,
            Json::obj([("path", Json::str("other/NOTES.md"))]),
        )],
    )];
    assert!(matches!(
        lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly),
        Err(RegistryError::ScanDenied { .. })
    ));
}

/// A credential-carrying locator is refused (CC3) — `user:pw@host` in a
/// marketplace member URI never scans into a ref.
#[test]
fn scan_credential_locator_refused() {
    let scans = vec![(
        DeclaredSource::Marketplace {
            catalog_ref: "mkt://c".into(),
        },
        vec![entry(
            "leaky",
            ExtensionKind::Skill,
            Json::obj([("uri", Json::str("https://user:pw@mkt.example/x"))]),
        )],
    )];
    assert!(matches!(
        lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly),
        Err(RegistryError::SchemaViolation { .. })
    ));
}

/// `exact_only` refuses a `(name, kind)` collision across sources —
/// `NameCollision`, never silent last-wins; `disjoint` admits both.
#[test]
fn scan_merge_policy_collisions() {
    let scans = vec![
        (
            dir_source(),
            vec![entry(
                "dup",
                ExtensionKind::Skill,
                Json::obj([("path", Json::str("dup"))]),
            )],
        ),
        (
            DeclaredSource::Registry {
                name: "corp".into(),
            },
            vec![entry("dup", ExtensionKind::Skill, Json::obj([]))],
        ),
    ];
    assert!(matches!(
        lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::ExactOnly),
        Err(RegistryError::NameCollision { .. })
    ));
    let refs = lifecycle::scan_declared_sources(&scans, &view(), MergePolicy::Disjoint).unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].locator.scheme, "directory_scan");
    assert_eq!(refs[1].locator.scheme, "registry");
}

// ── L3 claim lift + LegCrossing at runtime ─────────────────────────────────

/// The manifest claim members lift into `trust.declared_claims` — the only
/// legal home — and `record.manifest` stores verbatim.
#[test]
fn manifest_claims_lift_to_declared_claims() {
    let c = candidate("demo");
    let o = outcome(b"pkg", 7);
    let manifest = Json::obj([
        ("allowed_tools", Json::Arr(vec![Json::str("fs.read:*")])),
        (
            "tool_annotations",
            Json::obj([("readOnlyHint", Json::Bool(true))]),
        ),
        ("permissions", Json::Arr(vec![Json::str("net.egress")])),
        ("unrelated", Json::str("stays in manifest")),
    ]);
    let rep =
        lifecycle::resolve_extension(&c, &o, &[], None, Some(&manifest), human(), &view(), 100)
            .unwrap();
    let kinds: Vec<&str> = rep
        .record
        .trust
        .declared_claims
        .iter()
        .map(|c| c.kind.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec!["allowed_tools", "tool_annotation", "permission_manifest"]
    );
    assert_eq!(
        rep.record.manifest.get("unrelated").and_then(Json::as_str),
        Some("stays in manifest"),
        "unrecognised members never lift"
    );
}

/// A claim value carrying a pin/grant shape is a `LegCrossing` at resolve —
/// the L3 runtime check (a claim pretending to confer is refused before the
/// record exists).
#[test]
fn grant_shaped_claim_is_leg_crossing_at_resolve() {
    let c = candidate("demo");
    let o = outcome(b"pkg", 7);
    for key in ["version_id", "grant", "pin"] {
        let manifest = Json::obj([("allowed_tools", Json::obj([(key, Json::str("sha256:x"))]))]);
        match lifecycle::resolve_extension(
            &c,
            &o,
            &[],
            None,
            Some(&manifest),
            human(),
            &view(),
            100,
        ) {
            Err(RegistryError::SchemaViolation { detail, .. }) => {
                assert!(detail.contains("LegCrossing"), "{detail}")
            }
            other => panic!("claim carrying `{key}` must refuse LegCrossing, got {other:?}"),
        }
    }
}

/// The same L3 check runs on the install path — a model-instructed install
/// whose manifest smuggles a grant-shaped claim refuses `LegCrossing`.
#[test]
fn grant_shaped_claim_is_leg_crossing_at_install() {
    let c = candidate("demo");
    let o = outcome(b"pkg", 7);
    let manifest = Json::obj([(
        "requests",
        Json::obj([("version_id", Json::str("perm:escape"))]),
    )]);
    match lifecycle::install_completed(
        &c,
        &o,
        &[],
        None,
        Some(&manifest),
        &proposer(),
        &view(),
        None,
        100,
    ) {
        Err(RegistryError::SchemaViolation { detail, .. }) => {
            assert!(detail.contains("LegCrossing"), "{detail}")
        }
        other => panic!("grant-shaped claim at install must refuse LegCrossing, got {other:?}"),
    }
}

/// L2 conferral: a record minted under a delegate origin (model install) or
/// installed by a delegate coordinate carries no conferred grants — the
/// runtime check refuses the crossing at seal/activate too.
#[test]
fn delegate_origin_conferral_is_leg_crossing() {
    let c = candidate("demo");
    let o = outcome(b"pkg", 7);
    let mut record: ExtensionRecord =
        lifecycle::resolve_extension(&c, &o, &[], None, None, human(), &view(), 100)
            .unwrap()
            .record;
    // A delegate-provenance record with grants — the L2 leg crossing.
    record.provenance = ProvenanceRecord::minted(
        Origin::model("model:x", "run:y", "resp:1"),
        PersistenceScope::Run,
        0,
    );
    record.trust.grants = vec![grant("perm:x")];
    // text_authority must stay consistent for the L1 half to pass before L2.
    record.trust.text_authority = hh_registry::extension::default_text_authority(
        &record.kind,
        &record.provenance.origin,
        &record.trust.attestation_status,
    );
    match lifecycle::enforce_leg_boundaries(&record) {
        Err(RegistryError::SchemaViolation { detail, .. }) => {
            assert!(detail.contains("LegCrossing"), "{detail}")
        }
        other => panic!("delegate grant conferral must refuse, got {other:?}"),
    }
    // A human-origin record carrying a grant does not trip L2 (seal's
    // `grants ⊆ authority_cap` gate is the honest bound there).
    record.provenance = human_prov();
    record.trust.text_authority = hh_registry::extension::default_text_authority(
        &record.kind,
        &record.provenance.origin,
        &record.trust.attestation_status,
    );
    lifecycle::enforce_leg_boundaries(&record).unwrap();
}

// ── TextHygieneReport consumers ─────────────────────────────────────────────

/// A payload carrying bidi controls: the hygiene scan flags it.
fn bidi_payload() -> Vec<u8> {
    // U+202E RIGHT-TO-LEFT OVERRIDE inside otherwise innocuous text.
    "use tool \u{202e}eva\u{202c} now".as_bytes().to_vec()
}

/// `scanner_policy = quarantine` consumes a flagged report: the record lands
/// `quarantined`, the note and the `security.extension.quarantined` payload
/// carry the typed `scanner_flagged` reason — and `resolved` still mints
/// (the resolve leg ran; quarantine is the verdict).
#[test]
fn scanner_policy_quarantine_consumes_flagged() {
    let mut v = view();
    v.scanner_policy = ScannerPolicy::Quarantine;
    let rep = lifecycle::resolve_extension(
        &candidate("flaggy"),
        &outcome(&bidi_payload(), 7),
        &[],
        Some(&bidi_payload()),
        None,
        human(),
        &v,
        100,
    )
    .unwrap();
    assert_eq!(rep.record.trust.status, TrustStatus::Quarantined);
    assert!(rep.quarantined);
    assert_eq!(
        classes(&rep),
        vec![
            "security.extension.resolved",
            "security.extension.quarantined"
        ]
    );
    let q = &rep.events[1];
    assert_eq!(str_items(q, "reasons"), vec!["scanner_flagged"]);
}

/// `scanner_policy = deny` refuses a flagged payload `ScanDenied` — nothing
/// mints. A clean payload resolves under the same policy.
#[test]
fn scanner_policy_deny_refuses_flagged() {
    let mut v = view();
    v.scanner_policy = ScannerPolicy::Deny;
    assert!(matches!(
        lifecycle::resolve_extension(
            &candidate("flaggy"),
            &outcome(&bidi_payload(), 7),
            &[],
            Some(&bidi_payload()),
            None,
            human(),
            &v,
            100,
        ),
        Err(RegistryError::ScanDenied { .. })
    ));
    let rep = lifecycle::resolve_extension(
        &candidate("clean"),
        &outcome(b"clean bytes", 7),
        &[],
        Some(b"clean bytes"),
        None,
        human(),
        &v,
        100,
    )
    .unwrap();
    assert_eq!(rep.record.trust.status, TrustStatus::Resolved);
}

/// `unavailable` is never `clean` — under `quarantine`/`deny` a payload-less
/// resolve is consumed; under `advisory` the report mints with the advisory
/// note and no gate.
#[test]
fn scanner_policy_consumes_unavailable() {
    for (policy, quarantined) in [
        (ScannerPolicy::Quarantine, true),
        (ScannerPolicy::Advisory, false),
    ] {
        let mut v = view();
        v.scanner_policy = policy;
        let rep = lifecycle::resolve_extension(
            &candidate("nobytes"),
            &outcome(b"x", 7),
            &[],
            None, // no payload → the report mints `unavailable`
            None,
            human(),
            &v,
            100,
        )
        .unwrap();
        assert_eq!(rep.quarantined, quarantined, "{policy:?}");
        if quarantined {
            let q = &rep.events[1];
            let reasons = str_items(q, "reasons");
            assert!(reasons.contains(&"scanner_unavailable"), "{reasons:?}");
        }
    }
    let mut v = view();
    v.scanner_policy = ScannerPolicy::Deny;
    assert!(matches!(
        lifecycle::resolve_extension(
            &candidate("nobytes"),
            &outcome(b"x", 7),
            &[],
            None,
            None,
            human(),
            &v,
            100,
        ),
        Err(RegistryError::ScanDenied { .. })
    ));
}

/// `strictest_scan` — `deny > quarantine > advisory`; an unrecognised
/// declared spelling parses fail-closed `deny`.
#[test]
fn scanner_policy_strictest_and_fail_closed_parse() {
    use lifecycle::strictest_scan;
    assert_eq!(
        strictest_scan(ScannerPolicy::Advisory, ScannerPolicy::Quarantine),
        ScannerPolicy::Quarantine
    );
    assert_eq!(
        strictest_scan(ScannerPolicy::Quarantine, ScannerPolicy::Deny),
        ScannerPolicy::Deny
    );
    assert_eq!(ScannerPolicy::parse("advisory"), ScannerPolicy::Advisory);
    assert_eq!(ScannerPolicy::parse("off"), ScannerPolicy::Advisory);
    assert_eq!(
        ScannerPolicy::parse("quarantine"),
        ScannerPolicy::Quarantine
    );
    assert_eq!(ScannerPolicy::parse("bogus"), ScannerPolicy::Deny);
}

/// The store's `trust_view` projects the strictest `scanner_policy` over the
/// live policy set — a permissive policy never loosens a stricter one.
#[test]
fn trust_view_projects_strictest_scanner_policy() {
    use hh_registry::corpus;
    use hh_registry::records::{RegistryRecord, TrustRootPolicy};
    use hh_registry::store::RegistryStore;
    use std::collections::BTreeMap;

    let d = std::env::temp_dir().join(format!("hh-r2-20-tv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    corpus::build(&d).unwrap();
    let mut store = RegistryStore::open(&d, &corpus::kernel_registrar()).unwrap();
    let policy = |scanner: &str| {
        RegistryRecord::TrustRootPolicy(TrustRootPolicy {
            allowed_sources: BTreeSet::new(),
            accepted_signers: BTreeSet::new(),
            required_predicates: BTreeSet::new(),
            require_signature_for: BTreeSet::new(),
            hash_only_ceiling: AuthorityClass::External,
            max_age: None,
            drift_policy: "quarantine".into(),
            scanner_policy: scanner.into(),
            model_install: hh_registry::records::ModelInstallRule::AllowAttenuated,
            archive_policy: "advisory".into(),
            ext: BTreeMap::new(),
        })
    };
    let reg = corpus::kernel_registrar();
    store.register(policy("advisory"), &reg, None).unwrap();
    assert_eq!(store.trust_view().scanner_policy, ScannerPolicy::Advisory);
    store.register(policy("deny"), &reg, None).unwrap();
    assert_eq!(store.trust_view().scanner_policy, ScannerPolicy::Deny);
    let _ = std::fs::remove_dir_all(&d);
}

// ── resolved / quarantined / sealed / loaded producers ─────────────────────

/// `resolve_extension` mints `security.extension.resolved` on the report —
/// `extension_id`-free (the caller registers; the vid lands at register),
/// content-pinned, carrying the minted authority + hygiene verdicts.
#[test]
fn resolved_event_mints_on_resolve() {
    let rep = lifecycle::resolve_extension(
        &candidate("demo"),
        &outcome(b"pkg", 7),
        &[],
        Some(b"pkg"),
        None,
        human(),
        &view(),
        100,
    )
    .unwrap();
    let e = &rep.events[0];
    assert_eq!(
        e.get("class").and_then(Json::as_str),
        Some("security.extension.resolved")
    );
    assert_eq!(
        e.get("resolved").and_then(Json::as_str),
        Some("git:demo@abc123")
    );
    assert_eq!(e.get("text_hygiene").and_then(Json::as_str), Some("clean"));
}

/// `seal_extension` — `status ≠ resolved` refuses; an unpinned locator
/// refuses (L4); `grants ⊄ authority_cap` is `AuthorityWidening`; on success
/// the record seals and `security.extension.sealed` mints.
#[test]
fn seal_extension_gates_and_mints() {
    let rep = lifecycle::resolve_extension(
        &candidate("demo"),
        &outcome(b"pkg", 7),
        &[],
        None,
        None,
        human(),
        &view(),
        100,
    )
    .unwrap();
    let rec = rep.record;
    let out =
        lifecycle::seal_extension(&rec, "ext:demo", "trust-snap:1", &BTreeSet::new()).unwrap();
    assert_eq!(out.record.trust.status, TrustStatus::Sealed);
    assert_eq!(
        out.sealed_event.get("class").and_then(Json::as_str),
        Some("security.extension.sealed")
    );
    assert_eq!(
        out.sealed_event.get("extension_id").and_then(Json::as_str),
        Some("ext:demo")
    );
    assert_eq!(
        out.sealed_event
            .get("trust_snapshot_ref")
            .and_then(Json::as_str),
        Some("trust-snap:1")
    );
    // A sealed record does not re-seal.
    assert!(lifecycle::seal_extension(&out.record, "ext:demo", "t", &BTreeSet::new()).is_err());
    // grants ⊄ cap → AuthorityWidening.
    let mut g = rec.clone();
    g.trust.grants = vec![grant("perm:x")];
    assert!(matches!(
        lifecycle::seal_extension(&g, "ext:demo", "t", &BTreeSet::new()),
        Err(RegistryError::AuthorityWidening { .. })
    ));
    assert!(lifecycle::seal_extension(
        &g,
        "ext:demo",
        "t",
        &BTreeSet::from(["perm:x".to_string()])
    )
    .is_ok());
    // unpinned locator → refuse (L4).
    let mut u = rec.clone();
    u.locator.resolved = None;
    assert!(lifecycle::seal_extension(&u, "ext:demo", "t", &BTreeSet::new()).is_err());
    // quarantined status → refuse.
    let mut q = rec.clone();
    q.trust.status = TrustStatus::Quarantined;
    assert!(lifecycle::seal_extension(&q, "ext:demo", "t", &BTreeSet::new()).is_err());
}

/// `activate_extension` — revoked/quarantined refuse typed, unpinned refuses
/// L4, a pinned resolved record mints `security.extension.loaded` with
/// `pin_check: pinned`; a drifted surface under `pause_surface` mints no
/// `loaded` (the drift row is the evidence).
#[test]
fn activate_extension_gates_and_mints() {
    let rep = lifecycle::resolve_extension(
        &candidate("demo"),
        &outcome(b"pkg", 7),
        &[],
        None,
        None,
        human(),
        &view(),
        100,
    )
    .unwrap();
    let rec = rep.record;
    let observer = human_prov();
    let act = lifecycle::activate_extension(
        &rec,
        "ext:demo",
        None,
        None,
        SurfaceDriftPolicy::Notify,
        &observer,
        Some("run:x"),
    )
    .unwrap();
    let e = act.loaded_event.unwrap();
    assert_eq!(
        e.get("class").and_then(Json::as_str),
        Some("security.extension.loaded")
    );
    assert_eq!(e.get("pin_check").and_then(Json::as_str), Some("pinned"));
    assert_eq!(
        e.get("isolation").and_then(Json::as_str),
        Some("in_process")
    );
    // revoked → RegistryError::Revoked.
    let mut r = rec.clone();
    r.trust.status = TrustStatus::Revoked;
    assert!(matches!(
        lifecycle::activate_extension(
            &r,
            "ext:demo",
            None,
            None,
            SurfaceDriftPolicy::Notify,
            &observer,
            None,
        ),
        Err(RegistryError::Revoked { .. })
    ));
    // quarantined → typed refusal (never loads).
    let mut q = rec.clone();
    q.trust.status = TrustStatus::Quarantined;
    assert!(lifecycle::activate_extension(
        &q,
        "ext:demo",
        None,
        None,
        SurfaceDriftPolicy::Notify,
        &observer,
        None,
    )
    .is_err());
    // unpinned → L4 refusal.
    let mut u = rec.clone();
    u.locator.selector = Some("latest".into());
    assert!(lifecycle::activate_extension(
        &u,
        "ext:demo",
        None,
        None,
        SurfaceDriftPolicy::Notify,
        &observer,
        None,
    )
    .is_err());
}

/// The surface re-check at activate: a pinned surface whose live listing
/// moved produces the drift decision — `notify` loads with the changed/added
/// tools suppressed (`pin_check = drifted`); `pause_surface` mints no
/// `loaded` row.
#[test]
fn activate_runs_surface_drift_check() {
    let listing = |tool_hash: &str| {
        Json::obj([
            ("schema", Json::str("hh-mcp-listing/1")),
            ("server_ref", Json::str("srv")),
            ("listing_hash", Json::str(tool_hash)),
            (
                "tools",
                Json::Arr(vec![Json::obj([
                    ("name", Json::str("t1")),
                    ("listing_hash", Json::str(tool_hash)),
                    (
                        "tool",
                        Json::obj([("name", Json::str("t1")), ("description", Json::str("d"))]),
                    ),
                ])]),
            ),
        ])
    };
    let pinned = listing("h1");
    let live = listing("h2");
    let mut rec = lifecycle::resolve_extension(
        &candidate("srv"),
        &outcome(b"pkg", 7),
        &[],
        None,
        None,
        human(),
        &view(),
        100,
    )
    .unwrap()
    .record;
    rec.kind = ExtensionKind::McpServer;
    rec.trust.text_authority = hh_registry::extension::default_text_authority(
        &rec.kind,
        &rec.provenance.origin,
        &rec.trust.attestation_status,
    );
    rec.trust.surface_pin = Some(idp::address(b"surface-doc", "application/hir"));
    let observer = human_prov();
    // notify → loads, changed tools suppressed, pin_check = drifted.
    let act = lifecycle::activate_extension(
        &rec,
        "ext:srv",
        Some(&pinned),
        Some(&live),
        SurfaceDriftPolicy::Notify,
        &observer,
        Some("run:1"),
    )
    .unwrap();
    assert_eq!(act.pin_check, "drifted");
    assert!(act.drift.is_some());
    assert!(act.loaded_event.is_some(), "notify still loads");
    assert!(act
        .decision
        .as_ref()
        .map(|d| d.suppress.contains(&"t1".to_string()))
        .unwrap_or(false));
    // pause_surface → no loaded row; the drift evidence rides out.
    let act = lifecycle::activate_extension(
        &rec,
        "ext:srv",
        Some(&pinned),
        Some(&live),
        SurfaceDriftPolicy::PauseSurface,
        &observer,
        Some("run:1"),
    )
    .unwrap();
    assert!(
        act.loaded_event.is_none(),
        "pause_surface mints no loaded row"
    );
    assert!(act
        .decision
        .as_ref()
        .map(|d| d.pause_surface)
        .unwrap_or(false));
}

/// The model-install path now carries the resolve events too —
/// `[resolved, install_completed]` on a clean install under
/// `allow_attenuated`.
#[test]
fn install_completed_carries_resolve_events() {
    let out = lifecycle::install_completed(
        &candidate("demo"),
        &outcome(b"pkg", 7),
        &[],
        Some(b"pkg"),
        None,
        &proposer(),
        &view(),
        None,
        100,
    )
    .unwrap();
    let classes: Vec<&str> = out
        .events
        .iter()
        .filter_map(|e| e.get("class").and_then(Json::as_str))
        .collect();
    assert_eq!(
        classes,
        vec![
            "security.extension.resolved",
            "security.extension.install_completed"
        ]
    );
    // The model-installed record confers no grants (L2 holds by
    // construction — `enforce_leg_boundaries` ran inside).
    assert!(out.record.trust.grants.is_empty());
}
