//! Foreign *plugin manifest* formats (R-2.12.2¹; AC-R-2.12.2-10/11/12):
//! `claude_plugin_json/1` (a Claude `plugin.json`), `gemini_extension/1`
//! (`gemini-extension.json`), and `codex_agent_plugin/1` (a Codex agent-plugin
//! manifest). These are **format adapters**, not source systems — the
//! `ForeignSystem` vocabulary (`foreign.rs`) covers *where* a document came
//! from; this module covers *what shape it is in*. Source gating is the
//! caller's (`TrustRootPolicy.allowed_sources` + the standing `import_foreign`
//! `allowed_foreign_systems` gate); this module only refuses shapes it cannot
//! lift honestly.
//!
//! Import lifts the foreign document into a `PluginManifest/1` +
//! `ExtensionRecord{kind: plugin}`:
//!
//! - identity members (`name`, `version`/`version_label`, `description`,
//!   publisher metadata) lift into the manifest where a member exists and are
//!   otherwise preserved verbatim under `manifest.ext.foreign_document` (the
//!   re-emission source for `export_foreign_plugin`);
//! - executable-bearing members (`mcpServers`/`mcp_servers`, `commands`,
//!   `agents`, `skills`, `hooks`, `contextFileName`) become `contributions[]`
//!   with **pinned** locators — the entry body pins under `idp/1`, the
//!   per-executable `isolation` defaults to `subprocess_confined` (the
//!   third-party floor — a foreign manifest can never confer `in_process`);
//! - permission-ish members (`trust`, `excludeTools`, `allowedTools`,
//!   `permissions`) become `declared_claims` — **claims, never grants** (L3);
//! - everything the format cannot express typed is named in the `LossReport`
//!   *and* preserved verbatim — nothing is silently dropped.
//!
//! The record registers through the standing `register` path: a
//! non-first-party registrar admits it `quarantined` until `pin`, and the
//! minted `text_authority` (`default_text_authority` over an `import` origin —
//! `unverified`) is what `validate_extension_record` checks (L1). On export the
//! verbatim `foreign_document` is re-emitted with the canonical identity
//! members overlaid; the members *our* manifest carries that the foreign shape
//! cannot express — `requires`, `requests`, per-executable `isolation`,
//! `contract_range` — are the export leg's declared losses (AC-11's named
//! list).

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_identity::idp::{self, ContentAddress};
use hh_provenance::authority::PersistenceScope;
use hh_provenance::{Origin, ProvenanceRecord};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::kinds::Admission;
use crate::records::{LossReport, RegistryRecord};
use crate::store::RegistryStore;

use super::plugin::{
    manifest_to_json, Contribution, ContributionKind, Executable, PathOrLocator, PluginIdentity,
    PluginManifest, Requests, Requires,
};
use super::{
    default_text_authority, AttestationStatus, DeclaredClaim, DeclaredClaimKind, DeclaredSource,
    ExtensionKind, ExtensionRecord, ExtensionTrustRecord, HygieneStatus, IsolationClass,
    SourceLocator, TextHygieneReport, TrustStatus,
};

/// The closed foreign-plugin-format sum (AC-R-2.12.2-10's three named shapes —
/// `parse` rejects anything else; there is no `Other` leg to launder through).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForeignPluginFormat {
    /// A Claude `plugin.json` (`{name, version?, description?, author?,
    /// homepage?, repository?, license?, keywords?, commands?, agents?,
    /// hooks?, mcpServers?}`).
    ClaudePluginJson,
    /// A Gemini `gemini-extension.json` (`{name, version, description?,
    /// mcpServers?, contextFileName?, excludeTools?, fileFiltering?,
    /// settings?}`).
    GeminiExtension,
    /// A Codex agent-plugin manifest (`{name, version?, description?,
    /// agents?, skills?, mcpServers?|mcp_servers?}`).
    CodexAgentPlugin,
}

impl ForeignPluginFormat {
    /// The canonical spelling (also the mapping-version tag the import
    /// provenance carries).
    pub fn as_str(self) -> &'static str {
        match self {
            ForeignPluginFormat::ClaudePluginJson => "claude_plugin_json/1",
            ForeignPluginFormat::GeminiExtension => "gemini_extension/1",
            ForeignPluginFormat::CodexAgentPlugin => "codex_agent_plugin/1",
        }
    }

    /// The closed-sum parse (`None` for any other spelling).
    pub fn parse(s: &str) -> Option<ForeignPluginFormat> {
        Some(match s {
            "claude_plugin_json/1" | "claude_plugin_json" => ForeignPluginFormat::ClaudePluginJson,
            "gemini_extension/1" | "gemini_extension" => ForeignPluginFormat::GeminiExtension,
            "codex_agent_plugin/1" | "codex_agent_plugin" => ForeignPluginFormat::CodexAgentPlugin,
            _ => return None,
        })
    }

    /// The top-level members this format knows (the "known" test for the
    /// `other` loss partition — a member outside this set is named, never
    /// silently kept).
    fn known_members(self) -> &'static [&'static str] {
        match self {
            ForeignPluginFormat::ClaudePluginJson => &[
                "name",
                "version",
                "description",
                "author",
                "homepage",
                "repository",
                "license",
                "keywords",
                "commands",
                "agents",
                "hooks",
                "mcpServers",
                "outputStyles",
            ],
            ForeignPluginFormat::GeminiExtension => &[
                "name",
                "version",
                "description",
                "mcpServers",
                "contextFileName",
                "excludeTools",
                "fileFiltering",
                "settings",
            ],
            ForeignPluginFormat::CodexAgentPlugin => &[
                "name",
                "version",
                "description",
                "agents",
                "skills",
                "mcpServers",
                "mcp_servers",
            ],
        }
    }

    /// The member(s) carrying MCP-server maps for this format.
    fn mcp_server_members(self) -> &'static [&'static str] {
        match self {
            ForeignPluginFormat::CodexAgentPlugin => &["mcpServers", "mcp_servers"],
            _ => &["mcpServers"],
        }
    }

    /// The `(member, contribution-kind)` pairs for list-shaped contribution
    /// members (`commands`/`agents`/`skills` lift as procedure profiles,
    /// `hooks` as hooks — the §8.4 §3 closed contribution sum).
    fn list_contribution_members(self) -> &'static [(&'static str, ContributionKind)] {
        match self {
            ForeignPluginFormat::ClaudePluginJson => &[
                ("commands", ContributionKind::ProcedureProfile),
                ("agents", ContributionKind::ProcedureProfile),
                ("skills", ContributionKind::ProcedureProfile),
                ("hooks", ContributionKind::Hook),
            ],
            ForeignPluginFormat::GeminiExtension => &[],
            ForeignPluginFormat::CodexAgentPlugin => &[
                ("agents", ContributionKind::ProcedureProfile),
                ("skills", ContributionKind::ProcedureProfile),
            ],
        }
    }
}

/// The `import_foreign_plugin` result — the registered record plus its loss
/// report (the same shape `import_foreign` returns).
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignPluginImport {
    /// The registered `extension` record's `version_id`.
    pub version_id: String,
    /// The admission the register path assigned (`quarantined` for a
    /// non-first-party registrar — the AC-11 quarantined-import rule).
    pub admission: Admission,
    /// The import-leg losses (verbatim preservation rides
    /// `manifest.ext.foreign_document` + `record.ext.foreign_document`).
    pub loss_report: LossReport,
}

/// A `ContentAddress` over a canonical JSON value (the import pin — ours,
/// never a publisher's digest claim).
fn json_addr(domain: &str, j: &Json) -> ContentAddress {
    let bytes = j.to_canonical_string();
    ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest: idp::idp_id(domain, bytes.as_bytes())
            .strip_prefix("sha256:")
            .unwrap_or_default()
            .to_string(),
        media_type: "application/json".to_string(),
        size: bytes.len() as u64,
    }
}

/// The pinned locator a lifted contribution carries — the entry body *is* the
/// payload (never an inline body: the declaration is opaque, the locator pins
/// it).
fn pinned_entry_locator(
    format: ForeignPluginFormat,
    source_uri: &str,
    entry_id: String,
    now: u64,
) -> PathOrLocator {
    PathOrLocator::PinnedLocator(SourceLocator {
        scheme: format.as_str().to_string(),
        credential_free_uri: source_uri.to_string(),
        selector: None,
        resolved: Some(entry_id),
        fetched_at: Some(now),
    })
}

/// The `idp/1` content id for an entry (the locator's `resolved` coordinate).
fn entry_id(domain: &str, j: &Json) -> String {
    idp::idp_id(domain, j.to_canonical_string().as_bytes())
}

/// Members a foreign manifest may carry that assert trust we cannot verify
/// (CC2 — named in `trust_legs`, preserved verbatim, never honored: the
/// third-party floor applies regardless).
const FOREIGN_TRUST_MEMBERS: &[&str] = &[
    "trust",
    "trusted",
    "signature",
    "signatures",
    "signing",
    "verified",
    "verification",
];

/// Members that look like permission/capability policy the foreign shape
/// expresses in its own vocabulary — they lift to `declared_claims` and are
/// named in `capabilities` (a claim is never a grant; the monitor never sees
/// these as conferred).
const FOREIGN_CAPABILITY_MEMBERS: &[&str] = &[
    "excludeTools",
    "allowedTools",
    "allowed-tools",
    "permissions",
    "settings",
    "fileFiltering",
];

/// `import_foreign_plugin(store, format, doc, source, registrar,
/// trust_record_ref, now)` — lift a foreign plugin manifest to a quarantined
/// `ExtensionRecord{kind: plugin}` (AC-R-2.12.2-10/11).
///
/// - `doc` is the verbatim foreign document (a JSON object).
/// - `source` is the `DeclaredSource` the document was fetched under — its
///   kind spelling becomes the record locator's `scheme` (the *source* gate
///   already ran; this adapter never re-admits a source).
/// - `source_uri` is the credential-free URI the locator carries (refused at
///   `register` if it carries credentials — CC3).
/// - `now` is the fetch stamp (logical time — the store carries no wall
///   clock).
///
/// The record registers `quarantined` under a non-first-party registrar; a
/// `pin` endorsement lifts it per the standing C1 rule. The returned
/// `LossReport` names every member that did not map onto a typed manifest
/// field — the verbatim document rides `ext.foreign_document` regardless, so
/// re-export loses nothing that was ever represented.
#[allow(clippy::too_many_arguments)] // the import request's members are the record's shape.
pub fn import_foreign_plugin(
    store: &mut RegistryStore,
    format: ForeignPluginFormat,
    doc: &Json,
    source: &DeclaredSource,
    source_uri: &str,
    registrar: &ProvenanceRecord,
    trust_record_ref: Option<String>,
    now: u64,
) -> Result<ForeignPluginImport, RegistryError> {
    let obj = match doc {
        Json::Obj(m) => m,
        _ => {
            return Err(RegistryError::SchemaViolation {
                path: "foreign_plugin".to_string(),
                detail: format!("a {} document must be a JSON object", format.as_str()),
            })
        }
    };
    let name = obj
        .get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: "foreign_plugin.name".to_string(),
            detail: format!("a {} document requires `name`", format.as_str()),
        })?
        .to_string();
    let version_label = obj
        .get("version")
        .and_then(Json::as_str)
        .map(str::to_string);
    let description = obj
        .get("description")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();

    let mut loss = LossReport::default();
    let mut declared_claims: Vec<DeclaredClaim> = Vec::new();
    let mut contributions: Vec<Contribution> = Vec::new();

    // The import pin is always ours — a foreign manifest carries no `idp/1`
    // digest leg (a `digest`/`sha256` member it *does* carry is a claim, named
    // under `digest`, never trusted).
    loss.digest.push(format!(
        "{}: no publisher content digest — the body is pinned under idp/1 at import (our pin, not the publisher's)",
        format.as_str()
    ));
    if obj.contains_key("digest") || obj.contains_key("sha256") {
        loss.digest.push(format!(
            "{}: `digest`/`sha256` member is a foreign algorithm claim — preserved verbatim, never used as the pin",
            format.as_str()
        ));
    }
    // The standing import losses (AC-R-2.12.2-11's named list): the members
    // *our* manifest carries that no foreign plugin shape can express — the
    // imported record takes the defaults, and the defaulting is the declared
    // loss (the verbatim document preserves anything the format did carry).
    loss.capabilities.push(format!(
        "requests: {} carries no requests{{effects,fs_roots,egress,env_keys}} member — the import records `requests = ∅` (deny-by-default claims)",
        format.as_str()
    ));
    loss.other.push(format!(
        "requires: {} carries no requires{{hir_dialect,registry_dialect,plugin_abi,contracts,records}} member — the import binds the current dialect triple",
        format.as_str()
    ));
    loss.other.push(format!(
        "per-executable isolation: {} carries no isolation member — executables import at `subprocess_confined` (the third-party floor)",
        format.as_str()
    ));
    loss.other.push(format!(
        "contract_range: {} carries no contract_range member — contributions import unbound (compatibility re-derives at admit)",
        format.as_str()
    ));

    // ── MCP-server members → `mcp_server` contributions ─────────────────────
    for member in format.mcp_server_members() {
        let Some(servers) = obj.get(*member) else {
            continue;
        };
        let servers_obj = match servers {
            Json::Obj(m) => m,
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("foreign_plugin.{member}"),
                    detail: format!("{member} must be an object of server entries"),
                })
            }
        };
        for (srv_name, cfg) in servers_obj {
            let eid = entry_id("registry.foreign_plugin.mcp", cfg);
            contributions.push(Contribution {
                kind: ContributionKind::McpServer,
                path_or_locator: pinned_entry_locator(
                    format,
                    source_uri,
                    format!("{member}/{srv_name}#{eid}"),
                    now,
                ),
                declaration: cfg.clone(),
                executable: Some(Executable {
                    code_pointer: json_addr("registry.foreign_plugin.mcp", cfg),
                    // The third-party floor — a foreign manifest can never
                    // confer `in_process` (AC-R-2.12.2-6/7).
                    isolation: IsolationClass::SubprocessConfined,
                    placement_preference: None,
                    one_shot: None,
                    host_requirements: Json::obj([]),
                }),
                contract_range: None,
            });
            // Per-entry trust/permission members → claims + named losses.
            if let Json::Obj(cfg_obj) = cfg {
                for key in cfg_obj.keys() {
                    if FOREIGN_TRUST_MEMBERS.contains(&key.as_str()) {
                        loss.trust_legs.push(format!(
                            "{member}.{srv_name}.{key}: foreign trust assertion — no TrustRootPolicy anchor verifies it; the third-party floor applies"
                        ));
                        declared_claims.push(DeclaredClaim {
                            kind: DeclaredClaimKind::PermissionManifest,
                            value: cfg_obj.get(key).cloned().unwrap_or(Json::Null),
                        });
                    }
                    if FOREIGN_CAPABILITY_MEMBERS.contains(&key.as_str()) {
                        loss.capabilities.push(format!(
                            "{member}.{srv_name}.{key}: foreign capability member — lifted to declared_claims (a claim, never a grant)"
                        ));
                        declared_claims.push(DeclaredClaim {
                            kind: DeclaredClaimKind::AllowedTools,
                            value: cfg_obj.get(key).cloned().unwrap_or(Json::Null),
                        });
                    }
                }
            }
        }
    }

    // ── List-shaped contribution members ────────────────────────────────────
    for (member, kind) in format.list_contribution_members() {
        let Some(list) = obj.get(*member) else {
            continue;
        };
        match list {
            Json::Arr(items) => {
                for (i, item) in items.iter().enumerate() {
                    let (locator, declaration) = match item {
                        // A bare path string: the foreign layout is
                        // package-relative — `PackagePath` carries the
                        // spelling; the body lives at the path.
                        Json::Str(p) => (
                            PathOrLocator::PackagePath(p.clone()),
                            Json::obj([("member", Json::str(*member))]),
                        ),
                        other => (
                            pinned_entry_locator(
                                format,
                                source_uri,
                                format!(
                                    "{member}/{i}#{}",
                                    entry_id("registry.foreign_plugin.member", other)
                                ),
                                now,
                            ),
                            other.clone(),
                        ),
                    };
                    contributions.push(Contribution {
                        kind: *kind,
                        path_or_locator: locator,
                        declaration,
                        executable: None,
                        contract_range: None,
                    });
                }
            }
            // `hooks` (claude) is commonly an object keyed by event — the
            // same lift, one contribution per entry.
            Json::Obj(map) => {
                for (key, item) in map {
                    contributions.push(Contribution {
                        kind: *kind,
                        path_or_locator: pinned_entry_locator(
                            format,
                            source_uri,
                            format!(
                                "{member}/{key}#{}",
                                entry_id("registry.foreign_plugin.member", item)
                            ),
                            now,
                        ),
                        declaration: item.clone(),
                        executable: None,
                        contract_range: None,
                    });
                }
            }
            _ => {
                loss.other.push(format!(
                    "{member}: expected an array or object — preserved verbatim, not lifted"
                ));
            }
        }
    }

    // ── `contextFileName` (gemini) → `instruction_file` contribution ────────
    if let Some(Json::Str(ctx)) = obj.get("contextFileName") {
        contributions.push(Contribution {
            kind: ContributionKind::InstructionFile,
            path_or_locator: PathOrLocator::PackagePath(ctx.clone()),
            declaration: Json::obj([("contextFileName", Json::str(ctx.clone()))]),
            executable: None,
            contract_range: None,
        });
    }

    // ── Top-level capability/trust members → claims + named losses ─────────
    for (key, value) in obj {
        if FOREIGN_TRUST_MEMBERS.contains(&key.as_str()) {
            loss.trust_legs.push(format!(
                "{key}: foreign trust assertion — preserved verbatim, never honored"
            ));
            declared_claims.push(DeclaredClaim {
                kind: DeclaredClaimKind::PermissionManifest,
                value: value.clone(),
            });
        } else if FOREIGN_CAPABILITY_MEMBERS.contains(&key.as_str()) {
            loss.capabilities.push(format!(
                "{key}: foreign capability member — lifted to declared_claims (advisory only)"
            ));
            declared_claims.push(DeclaredClaim {
                kind: DeclaredClaimKind::AllowedTools,
                value: value.clone(),
            });
        } else if !format.known_members().contains(&key.as_str()) {
            // Unknown top-level member: named under `other`, preserved
            // verbatim — never silently kept, never silently dropped.
            loss.other.push(format!(
                "{key}: unknown {} member — preserved verbatim under ext.foreign_document",
                format.as_str()
            ));
        }
    }

    // ── Synthesize the PluginManifest ────────────────────────────────────────
    let origin = Origin::import(format.as_str(), "hh-foreign-plugin/1");
    let provenance = ProvenanceRecord::minted(origin.clone(), PersistenceScope::Run, now);
    let summary = Text::new(description, "foreign_plugin", provenance.clone());
    let mut ext = BTreeMap::new();
    ext.insert("foreign_format".to_string(), Json::str(format.as_str()));
    ext.insert("foreign_document".to_string(), doc.clone());
    let manifest = PluginManifest {
        identity: PluginIdentity {
            // Foreign imports are never `hh/`-namespaced (the `in_process`
            // admissibility rule keys on `hh/` + first-party registrar).
            namespace: "local".to_string(),
            name: name.clone(),
            version_label,
        },
        tier: "C0".to_string(),
        depends_on: Vec::new(),
        requires: Requires {
            hir_dialect: "1".to_string(),
            registry_dialect: "1".to_string(),
            plugin_abi: "1".to_string(),
            contracts: Vec::new(),
            records: Vec::new(),
        },
        contributions,
        requests: Requests::default(),
        claims: Json::obj([]),
        conformance_claims: Vec::new(),
        parameters: None,
        summary: summary.to_json(),
        ext,
    };

    let content = json_addr("registry.foreign_plugin", doc);
    let record = ExtensionRecord {
        kind: ExtensionKind::Plugin,
        name,
        content: content.clone(),
        manifest: manifest_to_json(&manifest),
        contributes: Vec::new(),
        locator: SourceLocator {
            scheme: source_kind_spelling(source).to_string(),
            credential_free_uri: source_uri.to_string(),
            selector: None,
            resolved: Some(content.id()),
            fetched_at: Some(now),
        },
        trust: ExtensionTrustRecord {
            text_authority: default_text_authority(
                &ExtensionKind::Plugin,
                &origin,
                &AttestationStatus::Missing,
            ),
            code_identity: Vec::new(),
            isolation: IsolationClass::SubprocessConfined,
            grants: Vec::new(),
            declared_claims,
            attestations: Vec::new(),
            attestation_status: AttestationStatus::Missing,
            scans: Vec::new(),
            text_hygiene: TextHygieneReport {
                status: HygieneStatus::Unavailable,
                findings: Vec::new(),
            },
            surface_pin: None,
            installed_by: None,
            installed_at: None,
            scope: PersistenceScope::Run,
            status: TrustStatus::Quarantined,
            review_ref: None,
        },
        provenance,
        ext: BTreeMap::from([
            ("foreign_format".to_string(), Json::str(format.as_str())),
            ("foreign_document".to_string(), doc.clone()),
        ]),
    };
    let vref = store.register(
        RegistryRecord::Extension(record),
        registrar,
        trust_record_ref,
    )?;
    let admission = store
        .get(&vref.version_id)
        .map(|(env, _)| env.admission)
        .unwrap_or(Admission::Quarantined);
    Ok(ForeignPluginImport {
        version_id: vref.version_id,
        admission,
        loss_report: loss,
    })
}

/// The `DeclaredSource` kind spelling the record locator's `scheme` carries
/// (the *source* fact — the format lives in `ext.foreign_format`).
fn source_kind_spelling(source: &DeclaredSource) -> &'static str {
    match source {
        DeclaredSource::Registry { .. } => "registry",
        DeclaredSource::Marketplace { .. } => "marketplace",
        DeclaredSource::Git { .. } => "git",
        DeclaredSource::Archive { .. } => "archive",
        DeclaredSource::McpEndpoint { .. } => "mcp_endpoint",
        DeclaredSource::InstructionFiles { .. } => "instruction_files",
        DeclaredSource::DirectoryScan { .. } => "directory_scan",
    }
}

/// The members *our* manifest carries that no foreign plugin shape can
/// express — the export leg's standing declared losses (AC-11's named list).
/// Emitted unconditionally: the report must name the loss class even when the
/// particular manifest happened to leave the member empty (the *vocabulary*
/// is lost, not just the value).
const EXPORT_LOST_MEMBERS: &[&str] = &[
    "requires",
    "requests",
    "per-executable isolation",
    "contract_range",
];

/// The `description` a foreign document carries — the `summary` `Text` leaf is
/// hash-addressed (`content_hash`, never raw content), so prose recovery
/// reads the import-time verbatim `ext.foreign_document` (or a caller-passed
/// description member); a synthesized export for a native manifest emits no
/// `description` rather than fabricating one.
fn doc_description(record: &ExtensionRecord) -> Option<String> {
    record
        .ext
        .get("foreign_document")
        .and_then(|d| d.get("description"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

/// `mcpServers` entries recovered from the manifest contributions: each
/// `mcp_server` contribution's `declaration` is the verbatim server entry
/// (import) or the record declaration (native) — keyed by the locator's
/// `resolved` member name when it carries one, else the declaration's `name`.
fn mcp_server_entries(manifest: &PluginManifest) -> BTreeMap<String, Json> {
    let mut out = BTreeMap::new();
    for (i, c) in manifest.contributions.iter().enumerate() {
        if c.kind != ContributionKind::McpServer {
            continue;
        }
        let name = match &c.path_or_locator {
            PathOrLocator::PinnedLocator(l) => l
                .resolved
                .as_deref()
                .and_then(|r| r.split('/').nth(1))
                .and_then(|s| s.split('#').next())
                .map(str::to_string),
            PathOrLocator::PackagePath(_) => None,
        }
        .or_else(|| {
            c.declaration
                .get("name")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("server_{i}"));
        out.insert(name, c.declaration.clone());
    }
    out
}

/// `export_foreign_plugin(format, record)` — emit the foreign document for a
/// registered plugin record (AC-R-2.12.2-11's export leg): for a
/// foreign-imported record the verbatim `ext.foreign_document` is re-emitted
/// with the canonical identity members overlaid (the manifest is the record
/// of truth); for a *native* manifest the foreign shape is **synthesized**
/// (`{name, version?, description?, mcpServers?, agents?/skills?}` from the
/// contributions) so the export → re-import round-trip recovers every field
/// not declared lost. `UnsupportedExportKind` for a non-plugin record.
pub fn export_foreign_plugin(
    format: ForeignPluginFormat,
    record: &ExtensionRecord,
) -> Result<(Json, LossReport), RegistryError> {
    if record.kind != ExtensionKind::Plugin {
        return Err(RegistryError::UnsupportedExportKind {
            kind: record.kind.as_str().to_string(),
            target: format.as_str().to_string(),
        });
    }
    let manifest = manifest_from_json_lossy(&record.manifest);
    let mut loss = LossReport::default();
    for member in EXPORT_LOST_MEMBERS {
        loss.other.push(format!(
            "{member}: {} carries no member for it — dropped on export (the record retains the typed values)",
            format.as_str()
        ));
    }
    let base = record
        .ext
        .get("foreign_document")
        .cloned()
        .unwrap_or_else(|| Json::obj([]));
    let mut out = match base {
        Json::Obj(m) => m,
        other => {
            let _ = other;
            BTreeMap::new()
        }
    };
    // Canonical identity overlay (the manifest is the record of truth — the
    // verbatim document is the substrate; `description` and every other
    // member ride the verbatim doc unchanged — a `Text` leaf is hash-only, so
    // the manifest cannot re-supply prose).
    let name = manifest
        .as_ref()
        .map(|m| m.identity.name.clone())
        .unwrap_or_else(|| record.name.clone());
    out.insert("name".to_string(), Json::str(name));
    match manifest
        .as_ref()
        .and_then(|m| m.identity.version_label.as_ref())
    {
        Some(v) => {
            out.insert("version".to_string(), Json::str(v.clone()));
        }
        None => {
            out.remove("version");
        }
    }
    if !out.contains_key("description") {
        if let Some(d) = doc_description(record) {
            out.insert("description".to_string(), Json::str(d));
        }
    }
    if manifest.is_none() {
        loss.other.push(
            "manifest: could not re-decode PluginManifest — identity exported from the record"
                .to_string(),
        );
    }
    // Contribution reconstruction (the export → re-import round-trip leg):
    // `mcp_server` contributions emit their verbatim declarations under
    // `mcpServers`; profile-shaped contributions emit under the format's
    // list member (`commands` for claude, `agents` for codex).
    if let Some(m) = manifest {
        let servers = mcp_server_entries(&m);
        if !servers.is_empty() && !out.contains_key("mcpServers") {
            out.insert(
                "mcpServers".to_string(),
                Json::Obj(servers.into_iter().collect()),
            );
        }
        let profiles: Vec<Json> = m
            .contributions
            .iter()
            .filter(|c| c.kind == ContributionKind::ProcedureProfile)
            .map(|c| c.declaration.clone())
            .collect();
        if !profiles.is_empty() {
            match format {
                ForeignPluginFormat::ClaudePluginJson => {
                    if !out.contains_key("commands") {
                        out.insert("commands".to_string(), Json::Arr(profiles));
                    }
                }
                ForeignPluginFormat::CodexAgentPlugin => {
                    if !out.contains_key("agents") {
                        out.insert("agents".to_string(), Json::Arr(profiles));
                    }
                }
                // `gemini-extension.json` has no profile-shaped member —
                // procedure-profile contributions are a named export loss
                // (the record keeps them; the format cannot).
                ForeignPluginFormat::GeminiExtension => {
                    loss.other.push(format!(
                        "procedure_profile contributions ({}): gemini_extension/1 carries no member for them",
                        profiles.len()
                    ));
                }
            }
        }
    }
    Ok((Json::Obj(out), loss))
}

/// Lenient manifest re-decode for export (`None` falls back to record-level
/// identity — export never fails on a manifest it cannot read).
fn manifest_from_json_lossy(j: &Json) -> Option<PluginManifest> {
    super::plugin::manifest_from_json(j, "manifest").ok()
}
