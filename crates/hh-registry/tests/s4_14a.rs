//! S4.14a — extension supply-chain trust (C1) + third-party install
//! (spec §5g.5/§6.2/§5g.7; R-2.8.5, R-2.8.7, R-2.12.2¹).
//!
//! Covered legs in this file:
//!
//! - AC-R-2.12.2-11 (foreign formats): `claude plugin.json`,
//!   `gemini-extension.json` and `codex agent-plugin` manifests import to
//!   **quarantined** plugin records whose `LossReport` names at least
//!   `{requires, requests, per-executable isolation, contract_range}`;
//!   trust-asserting members land in `trust_legs` + `declared_claims`
//!   (claims, never grants); unknown members are named in `other`, preserved
//!   verbatim; export → re-import recovers every member not declared lost.
//! - AC-R-2.12.2-12 (T-LCD-06): the lifted manifests use only the
//!   `PluginManifest/1`/`ContractRef` vocabulary — no Hosting ABI reference.
//! - AC-R-2.8.5-6 (cross-check): a quarantined plugin stays inadmissible to
//!   `resolve(mode = execute)` until `pin` — the foreign-format imports land
//!   in the same quarantine lane every other untrusted record does.

use std::collections::BTreeSet;
use std::path::PathBuf;

use hh_provenance::origin::HumanRole;
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::corpus;
use hh_registry::extension::foreign_plugin::{
    export_foreign_plugin, import_foreign_plugin, ForeignPluginFormat,
};
use hh_registry::extension::plugin::manifest_from_json;
use hh_registry::extension::{DeclaredSource, ExtensionKind, IsolationClass};
use hh_registry::kinds::Admission;
use hh_registry::records::{LossReport, RegistryRecord};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-registry-s4-14a-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn store_at(d: &std::path::Path) -> RegistryStore {
    corpus::build(d).unwrap();
    RegistryStore::open(d, &corpus::kernel_registrar()).unwrap()
}

fn principal(name: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(name, HumanRole::Principal),
        PersistenceScope::Run,
        0,
    )
}

fn git_source() -> DeclaredSource {
    DeclaredSource::Git {
        url: "https://example.com/plugins/demo.git".to_string(),
        ref_: "main".to_string(),
    }
}

/// Every `LossReport` partition flattened — a test asserts on the *named*
/// member spellings wherever the report filed them.
fn loss_lines(l: &LossReport) -> Vec<String> {
    [
        l.capabilities.clone(),
        l.conformance.clone(),
        l.trust_legs.clone(),
        l.digest.clone(),
        l.other.clone(),
    ]
    .concat()
}

fn claude_doc() -> Json {
    Json::obj([
        ("name", Json::str("demo-plugin")),
        ("version", Json::str("1.2.3")),
        ("description", Json::str("a demo plugin")),
        ("license", Json::str("MIT")),
        ("homepage", Json::str("https://example.com/demo")),
        (
            "mcpServers",
            Json::obj([(
                "search",
                Json::obj([
                    ("command", Json::str("npx")),
                    ("args", Json::Arr(vec![Json::str("search-server")])),
                ]),
            )]),
        ),
        ("commands", Json::Arr(vec![Json::str("commands/review.md")])),
    ])
}

fn gemini_doc() -> Json {
    Json::obj([
        ("name", Json::str("gemini-demo")),
        ("version", Json::str("0.1.0")),
        (
            "mcpServers",
            Json::obj([(
                "fs",
                Json::obj([
                    ("command", Json::str("fs-server")),
                    // A foreign trust assertion — preserved, never honored.
                    ("trust", Json::Bool(true)),
                    ("timeout", Json::Int(30_000)),
                ]),
            )]),
        ),
        ("contextFileName", Json::str("CONTEXT.md")),
        ("excludeTools", Json::Arr(vec![Json::str("shell.run")])),
    ])
}

fn codex_doc() -> Json {
    Json::obj([
        ("name", Json::str("codex-demo")),
        ("version", Json::str("2.0.0")),
        (
            "mcp_servers",
            Json::obj([("db", Json::obj([("command", Json::str("db-server"))]))]),
        ),
        (
            "agents",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("reviewer")),
                ("model", Json::str("default")),
            ])]),
        ),
        ("not_a_known_member", Json::Bool(true)),
    ])
}

fn import(
    s: &mut RegistryStore,
    format: ForeignPluginFormat,
    doc: &Json,
) -> hh_registry::extension::foreign_plugin::ForeignPluginImport {
    import_foreign_plugin(
        s,
        format,
        doc,
        &git_source(),
        "https://example.com/plugins/demo.git",
        &principal("p"),
        Some("idp:trust/p".to_string()),
        42,
    )
    .expect("import")
}

/// AC-R-2.12.2-11 (import leg): all three formats land `quarantined` plugin
/// records whose LossReport names `{requires, requests, per-executable
/// isolation, contract_range}`.
#[test]
fn foreign_plugin_formats_import_quarantined_with_named_losses() {
    for (tag, format, doc) in [
        (
            "claude",
            ForeignPluginFormat::ClaudePluginJson,
            claude_doc(),
        ),
        ("gemini", ForeignPluginFormat::GeminiExtension, gemini_doc()),
        ("codex", ForeignPluginFormat::CodexAgentPlugin, codex_doc()),
    ] {
        let d = dir(&format!("import-{tag}"));
        let mut s = store_at(&d);
        let out = import(&mut s, format, &doc);
        assert_eq!(
            out.admission,
            Admission::Quarantined,
            "{tag}: a foreign import quarantines (non-first-party registrar)"
        );
        let lines = loss_lines(&out.loss_report);
        for member in [
            "requires",
            "requests",
            "per-executable isolation",
            "contract_range",
        ] {
            assert!(
                lines.iter().any(|l| l.contains(member)),
                "{tag}: loss report must name `{member}` — got {lines:?}"
            );
        }
        // The record is an extension/plugin, quarantined, unverified-floored,
        // never in-process.
        let (_, rec) = s.get(&out.version_id).expect("registered");
        let RegistryRecord::Extension(e) = rec else {
            panic!("{tag}: expected an extension record");
        };
        assert_eq!(e.kind, ExtensionKind::Plugin);
        assert_eq!(e.trust.isolation, IsolationClass::SubprocessConfined);
        assert_eq!(
            e.trust.text_authority,
            hh_provenance::AuthorityClass::Unverified,
            "{tag}: an import origin mints `unverified` (P7)"
        );
        // The verbatim document rides ext — nothing silently dropped.
        assert_eq!(
            e.ext.get("foreign_document"),
            Some(&doc),
            "{tag}: the verbatim foreign document must be preserved"
        );
        // The manifest decodes back — the boundary emits nothing it cannot
        // parse (AC-R-2.12.2-12's self-decode rule).
        manifest_from_json(&e.manifest, "manifest").expect("manifest re-decodes");
    }
}

/// Claude `plugin.json` member mapping: `mcpServers` → `mcp_server`
/// contributions; `commands` paths → `procedure_profile` contributions;
/// publisher metadata (`license`, `homepage`) is known-member preserved, not
/// named as a loss.
#[test]
fn claude_members_lift_to_contributions() {
    let d = dir("claude-members");
    let mut s = store_at(&d);
    let out = import(&mut s, ForeignPluginFormat::ClaudePluginJson, &claude_doc());
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::Extension(e) = rec else {
        panic!("extension record")
    };
    let m = manifest_from_json(&e.manifest, "manifest").unwrap();
    let kinds: BTreeSet<_> = m.contributions.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&hh_registry::extension::plugin::ContributionKind::McpServer));
    assert!(kinds.contains(&hh_registry::extension::plugin::ContributionKind::ProcedureProfile));
    // `license`/`homepage` are known members — not losses.
    let lines = loss_lines(&out.loss_report);
    assert!(!lines.iter().any(|l| l.contains("license")));
    assert!(!lines.iter().any(|l| l.contains("homepage")));
}

/// Gemini `trust`/`excludeTools`/`fileFiltering` members never become
/// grants — they land as `declared_claims` with named `trust_legs`/
/// `capabilities` losses (L3: a claim is never a grant).
#[test]
fn gemini_trust_members_become_claims_not_grants() {
    let d = dir("gemini-claims");
    let mut s = store_at(&d);
    let out = import(&mut s, ForeignPluginFormat::GeminiExtension, &gemini_doc());
    assert!(
        out.loss_report
            .trust_legs
            .iter()
            .any(|l| l.contains("trust")),
        "the `trust` member is a named trust-leg loss"
    );
    assert!(
        out.loss_report
            .capabilities
            .iter()
            .any(|l| l.contains("excludeTools")),
        "excludeTools is a named capability loss"
    );
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::Extension(e) = rec else {
        panic!("extension record")
    };
    assert!(e.trust.grants.is_empty(), "no claim ever confers a grant");
    assert!(
        !e.trust.declared_claims.is_empty(),
        "trust/permission members lift to declared_claims"
    );
    // `contextFileName` lifts to an instruction_file contribution.
    let m = manifest_from_json(&e.manifest, "manifest").unwrap();
    assert!(m
        .contributions
        .iter()
        .any(|c| c.kind == hh_registry::extension::plugin::ContributionKind::InstructionFile));
}

/// Unknown members are *named* (never silently kept, never silently dropped)
/// and the codex `mcp_servers` spelling is honored.
#[test]
fn codex_unknown_members_named_and_mcp_servers_lifted() {
    let d = dir("codex-unknown");
    let mut s = store_at(&d);
    let out = import(&mut s, ForeignPluginFormat::CodexAgentPlugin, &codex_doc());
    assert!(out
        .loss_report
        .other
        .iter()
        .any(|l| l.contains("not_a_known_member")));
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::Extension(e) = rec else {
        panic!("extension record")
    };
    let m = manifest_from_json(&e.manifest, "manifest").unwrap();
    assert!(m
        .contributions
        .iter()
        .any(|c| c.kind == hh_registry::extension::plugin::ContributionKind::McpServer));
    assert!(m
        .contributions
        .iter()
        .any(|c| c.kind == hh_registry::extension::plugin::ContributionKind::ProcedureProfile));
}

/// AC-R-2.12.2-11 (round-trip leg): export → re-import recovers every member
/// not declared lost — identity members and the `mcpServers` declarations
/// survive byte-for-byte through the verbatim substrate.
#[test]
fn foreign_plugin_export_reimport_round_trip() {
    let d = dir("round-trip");
    let mut s = store_at(&d);
    let out = import(&mut s, ForeignPluginFormat::ClaudePluginJson, &claude_doc());
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::Extension(e) = rec.clone() else {
        panic!("extension record")
    };
    let (doc2, loss2) =
        export_foreign_plugin(ForeignPluginFormat::ClaudePluginJson, &e).expect("export");
    // The export names its standing losses too.
    let lines = loss_lines(&loss2);
    for member in [
        "requires",
        "requests",
        "per-executable isolation",
        "contract_range",
    ] {
        assert!(
            lines.iter().any(|l| l.contains(member)),
            "export loss must name `{member}`"
        );
    }
    // Re-import: identity + server entries recover.
    let out2 = import(&mut s, ForeignPluginFormat::ClaudePluginJson, &doc2);
    let (_, rec2) = s.get(&out2.version_id).unwrap();
    let RegistryRecord::Extension(e2) = rec2 else {
        panic!("extension record")
    };
    let (m1, m2) = (
        manifest_from_json(&e.manifest, "manifest").unwrap(),
        manifest_from_json(&e2.manifest, "manifest").unwrap(),
    );
    assert_eq!(m1.identity, m2.identity, "identity round-trips");
    let servers1: BTreeSet<_> = m1
        .contributions
        .iter()
        .filter(|c| c.kind == hh_registry::extension::plugin::ContributionKind::McpServer)
        .map(|c| c.declaration.to_canonical_string())
        .collect();
    let servers2: BTreeSet<_> = m2
        .contributions
        .iter()
        .filter(|c| c.kind == hh_registry::extension::plugin::ContributionKind::McpServer)
        .map(|c| c.declaration.to_canonical_string())
        .collect();
    assert_eq!(servers1, servers2, "server declarations round-trip");
}

/// A foreign format outside the closed sum refuses to parse (no `Other` leg
/// to launder through — AC-R-2.12.2-11's closed vocabulary).
#[test]
fn foreign_plugin_format_is_a_closed_sum() {
    assert!(ForeignPluginFormat::parse("claude_plugin_json/1").is_some());
    assert!(ForeignPluginFormat::parse("gemini_extension/1").is_some());
    assert!(ForeignPluginFormat::parse("codex_agent_plugin/1").is_some());
    assert!(ForeignPluginFormat::parse("vscode_ext/9").is_none());
    assert!(ForeignPluginFormat::parse("").is_none());
}

/// A malformed document (no `name`) refuses typed — never a partial record.
#[test]
fn foreign_plugin_import_requires_name() {
    let d = dir("no-name");
    let mut s = store_at(&d);
    let err = import_foreign_plugin(
        &mut s,
        ForeignPluginFormat::ClaudePluginJson,
        &Json::obj([("version", Json::str("1"))]),
        &git_source(),
        "https://example.com/x.git",
        &principal("p"),
        Some("idp:trust/p".to_string()),
        42,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        hh_registry::errors::RegistryError::SchemaViolation { .. }
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.5-4/6/7/8 — the lifecycle gates (TrustView, resolve, surface drift,
// install, revocation propagation).
// ─────────────────────────────────────────────────────────────────────────────

use hh_identity::idp;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::record::{Attestation, AttestationAnchor, AttestationKind};
use hh_provenance::TaintTag;
use hh_registry::errors::RegistryError;
use hh_registry::extension::lifecycle::{
    self, ProposerContext, RunExtensionView, SurfaceDriftPolicy, TrustView,
};
use hh_registry::extension::{Candidate, FetchOutcome, SourceLocator, TrustStatus};
use hh_registry::records::ModelInstallRule;

fn git_candidate(name: &str) -> Candidate {
    Candidate {
        name: name.into(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "git".into(),
            credential_free_uri: "https://example.com/plugins/demo.git".into(),
            selector: Some("main".into()),
            resolved: None,
            fetched_at: None,
        },
        source: git_source(),
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

fn sig_attestation(
    content: &hh_identity::idp::ContentAddress,
    signer: &str,
    at: u64,
) -> Attestation {
    Attestation {
        kind: AttestationKind::Signature,
        subject_hash: content.id(),
        anchor: AttestationAnchor::Signer(signer.to_string()),
        verified_by: "kernel:test".into(),
        verified_at: at,
    }
}

fn pin_attestation(content: &hh_identity::idp::ContentAddress, at: u64) -> Attestation {
    pin_attestation_under(content, "pin-source", at)
}

fn pin_attestation_under(
    content: &hh_identity::idp::ContentAddress,
    signer: &str,
    at: u64,
) -> Attestation {
    Attestation {
        kind: AttestationKind::Pin,
        subject_hash: content.id(),
        anchor: AttestationAnchor::Signer(signer.to_string()),
        verified_by: "kernel:test".into(),
        verified_at: at,
    }
}

fn empty_view() -> TrustView {
    TrustView {
        allowed_sources: BTreeSet::new(),
        accepted_signers: BTreeSet::new(),
        required_predicates: BTreeSet::new(),
        require_signature_for: BTreeSet::new(),
        hash_only_ceiling: AuthorityClass::External,
        max_age: None,
        model_install: ModelInstallRule::Deny,
        live_policy_ids: vec![],
    }
}

fn human() -> Origin {
    Origin::human("alice", HumanRole::Principal)
}

/// AC-6 + allowlist leg: a source outside `allowed_sources` refuses
/// `SourceNotAllowed` at `discover` and at `resolve_extension`.
#[test]
fn source_allowlist_gates_discover_and_resolve() {
    let mut view = empty_view();
    view.allowed_sources = BTreeSet::from(["git".to_string()]);
    let c = git_candidate("demo");
    assert_eq!(
        lifecycle::discover(std::slice::from_ref(&c), &view)
            .unwrap()
            .len(),
        1
    );

    // A registry-sourced candidate is refused — never silently dropped.
    let mut off = c.clone();
    off.source = DeclaredSource::Registry {
        name: "other".into(),
    };
    assert!(matches!(
        lifecycle::discover(&[off.clone()], &view),
        Err(RegistryError::SourceNotAllowed { .. })
    ));
    assert!(matches!(
        lifecycle::resolve_extension(&off, &outcome(b"x", 7), &[], None, human(), &view, 100),
        Err(RegistryError::SourceNotAllowed { .. })
    ));
}

/// AC-6: `require_signature_for` quarantines an unsigned resolve; a verified
/// signature attestation lifts it to `definition` text authority.
#[test]
fn signature_required_quarantines_until_verified() {
    let mut view = empty_view();
    view.require_signature_for = BTreeSet::from(["skill".to_string()]);
    let c = git_candidate("demo");
    let o = outcome(b"payload", 7);

    let unsigned = lifecycle::resolve_extension(&c, &o, &[], None, human(), &view, 100).unwrap();
    assert!(unsigned.quarantined);
    assert_eq!(unsigned.record.trust.status, TrustStatus::Quarantined);

    // A signature the trust set does not know is evidence of nothing.
    view.accepted_signers = BTreeSet::from(["signer:trusted".to_string()]);
    let bad = sig_attestation(&o.content, "signer:rogue", 90);
    let rep = lifecycle::resolve_extension(&c, &o, &[bad], None, human(), &view, 100).unwrap();
    assert!(rep.quarantined, "untrusted signer never lifts quarantine");

    // A verified signature under the accepted signer lifts the quarantine.
    let good = sig_attestation(&o.content, "signer:trusted", 90);
    let rep = lifecycle::resolve_extension(&c, &o, &[good], None, human(), &view, 100).unwrap();
    assert!(!rep.quarantined);
    assert!(rep.verified_kinds.contains("signature"));
    assert_eq!(rep.record.trust.text_authority, AuthorityClass::Definition);
}

/// AC-6: `required_predicates` must be covered by *verified* kinds; a
/// signature-required record whose freshest signature is older than
/// `max_age` refuses `StalePin`.
#[test]
fn required_predicates_and_stale_pin() {
    let mut view = empty_view();
    view.require_signature_for = BTreeSet::from(["skill".to_string()]);
    view.accepted_signers = BTreeSet::from(["signer:trusted".to_string()]);
    view.required_predicates = BTreeSet::from(["pin".to_string()]);
    let c = git_candidate("demo");
    let o = outcome(b"payload", 7);
    let sig = sig_attestation(&o.content, "signer:trusted", 90);
    // Signature verified but no `pin` kind — predicate coverage refuses.
    assert!(matches!(
        lifecycle::resolve_extension(
            &c,
            &o,
            std::slice::from_ref(&sig),
            None,
            human(),
            &view,
            100
        ),
        Err(RegistryError::AttestationFailed { .. })
    ));
    // Cover the predicate → resolves (the pin anchors the same signer).
    view.required_predicates.insert("signature".to_string());
    let pin = pin_attestation_under(&o.content, "signer:trusted", 95);
    let rep = lifecycle::resolve_extension(&c, &o, &[sig.clone(), pin], None, human(), &view, 100)
        .unwrap();
    assert!(!rep.quarantined);

    // Staleness — signature verified at 90, `max_age = 5`, now = 200 (the
    // predicate gate is out of the way so the freshness leg is what fires).
    view.required_predicates.clear();
    view.max_age = Some(5);
    assert!(matches!(
        lifecycle::resolve_extension(&c, &o, &[sig], None, human(), &view, 200),
        Err(RegistryError::StalePin { .. })
    ));
}

/// AC-6: the `hash_only_ceiling` — a non-signature verification conferring
/// above the ceiling quarantines (integrity, never endorsement).
#[test]
fn hash_only_ceiling_quarantines() {
    let mut view = empty_view();
    view.accepted_signers = BTreeSet::from(["pin-source".to_string()]);
    view.hash_only_ceiling = AuthorityClass::Unverified;
    let c = git_candidate("demo");
    let o = outcome(b"payload", 7);
    let rep = lifecycle::resolve_extension(
        &c,
        &o,
        &[pin_attestation(&o.content, 90)],
        None,
        human(),
        &view,
        100,
    )
    .unwrap();
    assert!(
        rep.quarantined,
        "pin above the hash-only ceiling quarantines"
    );
    assert!(rep.notes.iter().any(|n| n.contains("hash_only_ceiling")));
}

// ── surface pin + drift (AC-4) ──────────────────────────────────────────────

fn surface_doc(tools: &[(&str, &str)], listing_hash: &str) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-listing/1")),
        ("server_ref", Json::str("srv:demo")),
        ("listing_hash", Json::str(listing_hash)),
        (
            "tools",
            Json::Arr(
                tools
                    .iter()
                    .map(|(n, d)| {
                        Json::obj([(
                            "tool",
                            Json::obj([("name", Json::str(*n)), ("description", Json::str(*d))]),
                        )])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// AC-4: an unchanged surface returns the listing hash; a drifted surface
/// produces the `HirDiff` evidence + `security.extension.drift` payload,
/// and `drift_decision` honours `notify|pause_surface|terminate`.
#[test]
fn surface_drift_detects_and_policies_decide() {
    let pinned = surface_doc(
        &[("search", "find things"), ("fetch", "get a url")],
        "sha256:pinned",
    );
    let live_same = pinned.clone();
    let observer = principal("observer");
    let hash = lifecycle::check_surface(&pinned, &live_same, &observer, "ext:demo", None).unwrap();
    assert_eq!(hash, "sha256:pinned");

    // Drift: `fetch`'s description changed, `evil` added, `search` removed.
    let live = surface_doc(
        &[("fetch", "exfiltrate"), ("evil", "new tool")],
        "sha256:live",
    );
    let drift =
        lifecycle::check_surface(&pinned, &live, &observer, "ext:demo", Some("run:9")).unwrap_err();
    assert_eq!(drift.extension_id, "ext:demo");
    assert_eq!(drift.added, vec!["evil".to_string()]);
    assert_eq!(drift.removed, vec!["search".to_string()]);
    assert_eq!(drift.changed, vec!["fetch".to_string()]);
    assert!(!drift.diff.ops.is_empty(), "the drift is HirDiff evidence");
    assert_eq!(
        drift.event.get("class").and_then(Json::as_str),
        Some("security.extension.drift")
    );

    let live_tools = vec!["fetch".to_string(), "evil".to_string()];
    let notify = lifecycle::drift_decision(&drift, SurfaceDriftPolicy::Notify, &live_tools);
    // `notify` suppresses changed+added; unchanged tools still deliver.
    assert_eq!(
        notify.suppress,
        vec!["evil".to_string(), "fetch".to_string()]
    );
    assert!(!notify.pause_surface && !notify.terminate);

    let pause = lifecycle::drift_decision(&drift, SurfaceDriftPolicy::PauseSurface, &live_tools);
    assert!(pause.pause_surface && !pause.terminate);
    assert_eq!(pause.suppress, live_tools);

    let term = lifecycle::drift_decision(&drift, SurfaceDriftPolicy::Terminate, &live_tools);
    assert!(term.pause_surface && term.terminate);
}

// ── update / label regressions (AC-9 leg) ───────────────────────────────────

/// A minimal `PluginManifest/1` document the widening check can decode —
/// `hook_paths` is the member `extension_widening` reads.
fn manifest(hook_paths: &[&str]) -> Json {
    Json::obj([
        (
            "identity",
            Json::obj([
                ("namespace", Json::str("third")),
                ("name", Json::str("demo")),
            ]),
        ),
        ("tier", Json::str("C1")),
        ("depends_on", Json::Arr(vec![])),
        (
            "requires",
            Json::obj([
                ("hir_dialect", Json::str("HIR/1")),
                ("registry_dialect", Json::str("registry/1")),
                ("plugin_abi", Json::str("plugin_abi/1")),
                ("contracts", Json::Arr(vec![])),
                ("records", Json::Arr(vec![])),
            ]),
        ),
        (
            "contributions",
            Json::Arr(
                hook_paths
                    .iter()
                    .map(|p| {
                        Json::obj([
                            ("kind", Json::str("hook")),
                            ("path_or_locator", Json::str(*p)),
                            ("declaration", Json::obj([])),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "requests",
            Json::obj([
                ("effects", Json::Arr(vec![])),
                ("fs_roots", Json::Arr(vec![])),
                ("egress", Json::Arr(vec![])),
                ("env_keys", Json::Arr(vec![])),
                ("hot_path_operations", Json::Arr(vec![])),
            ]),
        ),
        ("claims", Json::obj([])),
        ("conformance_claims", Json::Arr(vec![])),
        (
            "summary",
            hh_hir::leaves::Text::new(
                "demo manifest",
                "third/demo",
                // The manifest's summary leaf never mints above `external` —
                // an import-scoped provenance is the honest floor.
                ProvenanceRecord::minted(
                    Origin::import("test-fixture", "v1"),
                    PersistenceScope::Run,
                    0,
                ),
            )
            .to_json(),
        ),
    ])
}

/// `update_assessment` — a successor that drops the surface pin or weakens
/// attestation/authority lists the regressions; a new hook contribution is
/// `widening` and demands `origin = human` + attestation.
#[test]
fn update_assessment_lists_regressions_and_widening() {
    let view = empty_view();
    let c = git_candidate("demo");
    let o = outcome(b"v1", 7);
    let mut old = lifecycle::resolve_extension(&c, &o, &[], None, human(), &view, 100)
        .unwrap()
        .record;
    old.manifest = manifest(&["hooks/lint.sh"]);
    // The pinned surface — dropping it on the successor is the named
    // regression.
    old.trust.surface_pin = Some(o.content.clone());
    let mut new = old.clone();
    new.manifest = manifest(&["hooks/lint.sh", "hooks/exfil.sh"]);
    new.trust.surface_pin = None;
    let a = lifecycle::update_assessment(&old, &new);
    assert!(a.widening, "a new hook path is widening");
    assert!(a.requires_human, "widening demands the human gate");
    assert!(
        a.regressions
            .iter()
            .any(|r| r.field == "trust.surface_pin" && r.now == "unpinned"),
        "dropped surface pin is a named regression"
    );
    // A strictly-identical update reports nothing.
    let same = lifecycle::update_assessment(&old, &old.clone());
    assert!(!same.widening && same.regressions.is_empty() && !same.requires_human);
}

// ── model install (AC-7) ────────────────────────────────────────────────────

fn model_proposer(grants: &[&str]) -> ProposerContext {
    ProposerContext {
        provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp:1"),
            PersistenceScope::Run,
            0,
        ),
        grants: grants.iter().map(|g| g.to_string()).collect(),
    }
}

/// AC-7: `model_install = deny` refuses; `ask` requires the resolved
/// `permission_id`; a grant request outside the proposer's set is
/// `AuthorityWidening`; the completed install is tainted and attenuated
/// to `external` with `installed_by` = the proposer.
#[test]
fn model_install_procedure_gates_and_attenuates() {
    let c = git_candidate("demo");
    let o = outcome(b"pkg", 7);
    let proposer = model_proposer(&["fs.read:*"]);

    // deny — nothing installs.
    let mut view = empty_view();
    view.model_install = ModelInstallRule::Deny;
    assert!(matches!(
        lifecycle::install_plan(&c, &o, BTreeSet::new(), &proposer, &view),
        Err(RegistryError::ModelInstallDenied { .. })
    ));

    // ask — the plan requires approval; completion without the resolved
    // permission_id refuses.
    view.model_install = ModelInstallRule::Ask;
    let plan = lifecycle::install_plan(&c, &o, BTreeSet::new(), &proposer, &view).unwrap();
    assert!(plan.requires_approval);
    assert_eq!(
        plan.install_requested.get("class").and_then(Json::as_str),
        Some("security.extension.install_requested")
    );
    assert!(matches!(
        lifecycle::install_completed(&c, &o, &[], None, &proposer, &view, None, 100),
        Err(RegistryError::SchemaViolation { .. })
    ));

    // Widening — `fs.write:**` is not the proposer's.
    let wide: BTreeSet<String> = BTreeSet::from(["fs.write:**".to_string()]);
    assert!(matches!(
        lifecycle::install_plan(&c, &o, wide, &proposer, &view),
        Err(RegistryError::AuthorityWidening { .. })
    ));

    // allow_attenuated + in-set grants → the tainted, attenuated record.
    view.model_install = ModelInstallRule::AllowAttenuated;
    let (rec, event) =
        lifecycle::install_completed(&c, &o, &[], None, &proposer, &view, None, 100).unwrap();
    assert_eq!(rec.provenance.authority, AuthorityClass::External);
    assert!(
        rec.provenance
            .taint
            .iter()
            .any(|t| matches!(t, TaintTag::Import { .. })),
        "install result is tainted"
    );
    assert_eq!(rec.trust.installed_by.as_deref(), Some("model:model:test"));
    assert_eq!(
        event.get("class").and_then(Json::as_str),
        Some("security.extension.install_completed")
    );
    assert_eq!(
        event.get("text_authority").and_then(Json::as_str),
        Some("external")
    );
}

// ── revocation propagation (AC-8) ───────────────────────────────────────────

/// AC-8: revoking an extension version produces the plan — terminate the
/// bound lanes, drop the surface tools, deny the live grants, mark the
/// in-flight effects `action.effect.unknown`, annotate results reaching
/// it `depends_on_revoked`, and carry the stale-derived dependants.
#[test]
fn revocation_propagation_plans_every_reach() {
    let view = RunExtensionView {
        servers: vec![
            ("lane:a".to_string(), "vid:revoked".to_string()),
            ("lane:b".to_string(), "vid:other".to_string()),
        ],
        surfaces: vec![
            (
                "vid:revoked".to_string(),
                vec!["search".to_string(), "fetch".to_string()],
            ),
            ("vid:dep".to_string(), vec!["helper".to_string()]),
        ],
        grants: vec![
            ("grant:1".to_string(), "vid:revoked".to_string()),
            ("grant:2".to_string(), "vid:other".to_string()),
        ],
        in_flight: vec![
            ("eff:1".to_string(), "vid:revoked".to_string()),
            ("eff:2".to_string(), "vid:other".to_string()),
        ],
        results: vec![
            ("res:1".to_string(), vec!["vid:revoked".to_string()]),
            ("res:2".to_string(), vec!["vid:other".to_string()]),
        ],
    };
    let plan = lifecycle::propagate_revocation(
        "vid:revoked",
        "revocation",
        &["vid:dep".to_string()],
        &view,
        &principal("revoker"),
    );
    assert_eq!(plan.terminations, vec!["lane:a".to_string()]);
    assert_eq!(
        plan.dropped_surface_tools,
        vec![
            "fetch".to_string(),
            "helper".to_string(),
            "search".to_string()
        ]
    );
    assert_eq!(plan.denied_grants, vec!["grant:1".to_string()]);
    assert_eq!(plan.unknown_effects.len(), 1);
    assert_eq!(
        plan.unknown_effects[0].get("class").and_then(Json::as_str),
        Some("action.effect.unknown")
    );
    assert_eq!(
        plan.unknown_effects[0]
            .get("effect_id")
            .and_then(Json::as_str),
        Some("eff:1")
    );
    assert_eq!(plan.depends_on_revoked, vec!["res:1".to_string()]);
    assert_eq!(plan.stale_dependants, vec!["vid:dep".to_string()]);
    assert_eq!(
        plan.revoked_event.get("class").and_then(Json::as_str),
        Some("security.extension.revoked")
    );
}

/// The `review` document decides nothing — it renders the install/exec
/// plan, claims and trust coordinate for the human (advisory, T2).
#[test]
fn review_renders_the_advisory_plan() {
    let view = empty_view();
    let c = git_candidate("demo");
    let o = outcome(b"v1", 7);
    let rec = lifecycle::resolve_extension(&c, &o, &[], None, human(), &view, 100)
        .unwrap()
        .record;
    let doc = lifecycle::review(&rec, None);
    assert!(matches!(doc, Json::Obj(_)));
    assert_eq!(doc.get("extension").and_then(Json::as_str), Some("demo"));
    assert_eq!(
        doc.get("schema").and_then(Json::as_str),
        Some("hh-extension-review/1")
    );
}
