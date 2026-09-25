//! `import`/`export` — the general foreign-registry operations (§6.2 R-2.10.2;
//! ADR-0153 D4; AC-R-2.10.2-11).
//!
//! `import(ForeignRef, document)` lifts a closed set of foreign registry
//! documents — `ForeignSystem` = `mcp_registry | acp_registry | marketplace |
//! git | archive` — into `foreign_import` records. The lift is honest
//! bookkeeping, never a laundering channel:
//!
//! - the `foreign_import` record registers **quarantined** (the admission the
//!   policy decides — only a pin endorsement / explicit admission lifts it);
//! - the `lifted_record` is a *candidate descriptor* (`lifted_kind`
//!   `participant_candidate | mcp_server_candidate | foreign_component`), never
//!   a `VariantRecord` — foreign content enters no live record kind;
//! - every foreign member that survives is preserved verbatim under
//!   `lifted_record.declared_claims` / `ext`; every member that could not be
//!   represented lands in a structured [`LossReport`] partition — silent
//!   dropping is prohibited by construction;
//! - nothing the foreign document claims — signatures, keys, verdicts — is
//!   verified (a self-declared claim is never evidence, CC2); unverifiable
//!   trust legs are *named* in the loss report.
//!
//! `export(refs[], target)` projects live records into the one stage-4 foreign
//! target, `plugin_manifest/1` (`PluginManifest` canonical form —
//! `registry.extension.plugin`). The lowering is honest about the protocol
//! asymmetry the design note calls out: a plugin manifest carries an
//! `executable` + `isolation` surface and a `summary: Text{authority ≤
//! external}` ceiling, so a registry-side provenance summary above `external`
//! cannot occupy that slot. The verbatim `Text` rides
//! `claims.exported_summary` (full-fidelity round-trip) and the loss report
//! names the downgrade. `export` self-checks: every emitted document must
//! re-decode through `manifest_from_json` before it leaves — an export that
//! cannot re-parse its own output is a bug, refused not shipped.
//!
//! `lift_variant_from_manifest` is the re-import lift: it decodes a
//! `plugin_manifest/1` document and reconstitutes the embedded `variant`
//! contribution back into a `VariantRecord` — the AC-11 round-trip whose
//! byte-identical body mints the same `version_id`.

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_identity::names::ResolveMode;
use hh_plugin::ContractRef;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::extension::plugin::{
    manifest_from_json, manifest_to_json, Contribution, ContributionKind, Executable,
    PathOrLocator, PinnedRecordRef, PluginIdentity, PluginManifest, Requests, Requires,
};
use crate::extension::{IsolationClass, SourceLocator};
use crate::kinds::{Admission, Placement, ProducedBy};
use crate::records::{ForeignImport, LossReport, RegistryRecord, VariantRecord};
use crate::schema;
use crate::store::{RegistryStore, ResolveInput, ResolveRequest, ResolvedRecord};

// ── ForeignSystem ────────────────────────────────────────────────────────────

/// The closed foreign-system vocabulary `import` may read (§6.2 R-2.10.2 —
/// "a closed set of foreign registry systems"). Spellings are the canonical
/// values stored in `ForeignImport.source_system`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForeignSystem {
    /// An ACP `agent.json` manifest — lifts to a `participant_candidate`.
    AcpRegistry,
    /// An MCP `server.json` registry listing — lifts to an
    /// `mcp_server_candidate` extension descriptor.
    McpRegistry,
    /// A marketplace plugin/component listing.
    Marketplace,
    /// A git-source component listing.
    Git,
    /// An archive (bundle) listing.
    Archive,
}

impl ForeignSystem {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            ForeignSystem::AcpRegistry => "acp_registry",
            ForeignSystem::McpRegistry => "mcp_registry",
            ForeignSystem::Marketplace => "marketplace",
            ForeignSystem::Git => "git",
            ForeignSystem::Archive => "archive",
        }
    }

    /// Parse a canonical spelling (`parse("…")` — string literals only, never
    /// another system's enum).
    pub fn parse(s: &str) -> Result<ForeignSystem, RegistryError> {
        Ok(match s {
            "acp_registry" => ForeignSystem::AcpRegistry,
            "mcp_registry" => ForeignSystem::McpRegistry,
            "marketplace" => ForeignSystem::Marketplace,
            "git" => ForeignSystem::Git,
            "archive" => ForeignSystem::Archive,
            other => {
                return Err(RegistryError::ForeignSystemRefused {
                    system: other.to_string(),
                })
            }
        })
    }
}

// ── ForeignRef ───────────────────────────────────────────────────────────────

/// `ForeignRef{system, locator, digest?, label?}` — the import head argument.
/// `locator` is credential-free by construction (CC3); `digest` is the
/// foreign-side content pin when the document itself doesn't carry one.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignRef {
    /// The foreign system this document belongs to.
    pub system: ForeignSystem,
    /// The credential-free locator the document was read from.
    pub locator: String,
    /// The foreign-side content digest (`sha256:<hex>` or a foreign-native
    /// pin spelling), when known.
    pub digest: Option<String>,
    /// The foreign-side version/release label, when the locator carries one.
    pub label: Option<String>,
}

impl ForeignRef {
    /// Canonical member spelling (the `foreign_ref` member of `foreign_import`).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(d) = &self.digest {
            m.insert("digest".to_string(), Json::str(d.clone()));
        }
        if let Some(l) = &self.label {
            m.insert("label".to_string(), Json::str(l.clone()));
        }
        m.insert("locator".to_string(), Json::str(self.locator.clone()));
        m.insert(
            "system".to_string(),
            Json::str(self.system.as_str().to_string()),
        );
        Json::Obj(m)
    }

    /// Decode a `foreign_ref` member.
    pub fn from_json(j: &Json) -> Result<ForeignRef, RegistryError> {
        let missing = |k: &str| RegistryError::SchemaViolation {
            path: format!("foreign_ref.{k}"),
            detail: "missing member".to_string(),
        };
        let str_at = |k: &str| -> Result<String, RegistryError> {
            match j.get(k) {
                Some(Json::Str(s)) => Ok(s.clone()),
                _ => Err(missing(k)),
            }
        };
        let opt = |k: &str| -> Option<String> {
            match j.get(k) {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        Ok(ForeignRef {
            system: ForeignSystem::parse(&str_at("system")?)?,
            locator: str_at("locator")?,
            digest: opt("digest"),
            label: opt("label"),
        })
    }
}

// ── import ───────────────────────────────────────────────────────────────────

/// The `import` result — the registered `foreign_import` row's `version_id`,
/// its admission (always `quarantined` at import — the record carries no
/// anchors; only a pin endorsement or explicit admission lifts it), and the
/// lift's structured loss report.
#[derive(Debug, Clone)]
pub struct ForeignImportOutcome {
    /// The registered `foreign_import` record's `version_id`.
    pub version_id: String,
    /// The admission the record carries (`quarantined` by policy).
    pub admission: Admission,
    /// The structured loss report (partitions: `capabilities | conformance |
    /// trust_legs | digest | other`).
    pub loss_report: LossReport,
}

/// `import(ForeignRef, document, registrar, trust_record_ref?)` — the governed
/// foreign-system head. The `system` must be in `policy.allowed_foreign_systems`
/// (fail-closed: an empty set admits nothing — the operator narrows the closed
/// vocabulary by listing what this deployment reads). The document is lifted
/// per-system and registered as a `foreign_import` record — the register path
/// records the `imported` lifecycle event itself. A non-first-party registrar
/// pins its `trust_record_ref` per the standing `register` rule (ADR-0153).
pub fn import_foreign(
    store: &mut RegistryStore,
    fref: &ForeignRef,
    document: &Json,
    registrar: &ProvenanceRecord,
    trust_record_ref: Option<String>,
) -> Result<ForeignImportOutcome, RegistryError> {
    // Narrowing check — the policy lists the foreign systems this deployment
    // may read. Fail-closed: an empty set reads nothing.
    if !store
        .policy()
        .allowed_foreign_systems
        .contains(fref.system.as_str())
    {
        return Err(RegistryError::ForeignSystemRefused {
            system: fref.system.as_str().to_string(),
        });
    }
    let digest = fref
        .digest
        .clone()
        .or_else(|| doc_str(document, "digest"))
        .or_else(|| doc_str(document, "sha256"));
    let (lifted, loss_report) = lift_foreign(fref.system, document, digest.as_deref());
    let rec = ForeignImport {
        system: fref.system.as_str().to_string(),
        locator: fref.locator.clone(),
        digest: fref.digest.clone(),
        label: fref.label.clone(),
        lifted_record: lifted,
        loss_report: loss_report.clone(),
    };
    let vref = store.register(
        RegistryRecord::ForeignImport(rec),
        registrar,
        trust_record_ref,
    )?;
    let admission = store
        .get(&vref.version_id)
        .map(|(env, _)| env.admission)
        .unwrap_or(Admission::Quarantined);
    Ok(ForeignImportOutcome {
        version_id: vref.version_id,
        admission,
        loss_report,
    })
}

fn doc_str(doc: &Json, key: &str) -> Option<String> {
    match doc.get(key) {
        Some(Json::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Members that claim trust legs the registry cannot verify against an anchor
/// (CC2 — never trusted, always *named*).
const TRUST_MEMBERS: &[&str] = &[
    "signature",
    "signatures",
    "signing",
    "attestation",
    "key",
    "keys",
];
/// Members that claim capabilities which are not registry declarations.
const CAPABILITY_MEMBERS: &[&str] = &["capabilities", "skills", "tools", "permissions"];
/// Members that claim conformance which is not an admissible registry report.
const CONFORMANCE_MEMBERS: &[&str] = &[
    "conformance",
    "verification",
    "verified",
    "health",
    "rating",
];

/// The per-system lift → `(lifted_record, LossReport)`. Every doc member is
/// either consumed into a named lifted field, preserved verbatim under
/// `declared_claims`, or named in a loss partition — nothing is silently
/// dropped.
fn lift_foreign(system: ForeignSystem, doc: &Json, digest: Option<&str>) -> (Json, LossReport) {
    let mut loss = LossReport::default();
    let lifted_kind = match system {
        ForeignSystem::AcpRegistry => "participant_candidate",
        ForeignSystem::McpRegistry => "mcp_server_candidate",
        _ => "foreign_component",
    };
    // Identity members become named lifted fields; everything else is verbatim
    // `declared_claims` (MCP's source claims and `ext` metadata survive here).
    let mut lifted: BTreeMap<String, Json> = BTreeMap::new();
    lifted.insert("lifted_kind".to_string(), Json::str(lifted_kind));
    lifted.insert(
        "source_system".to_string(),
        Json::str(system.as_str().to_string()),
    );
    let mut declared: BTreeMap<String, Json> = BTreeMap::new();
    if let Json::Obj(m) = doc {
        for (k, v) in m {
            match k.as_str() {
                "name" | "display_name" | "version" | "description" | "vendor" | "namespace" => {
                    lifted.insert(k.clone(), v.clone());
                }
                "package" if matches!(system, ForeignSystem::McpRegistry) => {
                    lifted.insert(k.clone(), v.clone());
                }
                _ => {
                    declared.insert(k.clone(), v.clone());
                }
            }
        }
    }
    // Loss partitions — named from the members actually present.
    for k in declared.keys() {
        if CAPABILITY_MEMBERS.contains(&k.as_str()) {
            loss.capabilities.push(format!(
                "declared `{k}` — carried as a claim, not a registry declaration"
            ));
        }
        if TRUST_MEMBERS.contains(&k.as_str()) {
            loss.trust_legs.push(format!(
                "declared `{k}` — unverified (no TrustRootPolicy anchor); never evidence"
            ));
        }
        if CONFORMANCE_MEMBERS.contains(&k.as_str()) {
            loss.conformance.push(format!(
                "declared `{k}` — publisher claim only; never counts toward `probed`"
            ));
        }
    }
    match digest {
        Some(d) => {
            lifted.insert("digest".to_string(), Json::str(d));
        }
        None => {
            loss.digest
                .push("no content digest — re-pin under idp/1 on adopt".to_string());
        }
    }
    lifted.insert("declared_claims".to_string(), Json::Obj(declared));
    lifted.insert("ext".to_string(), Json::obj(Vec::new()));
    (Json::Obj(lifted), loss)
}

// ── export ───────────────────────────────────────────────────────────────────

/// The one stage-4 export target — `plugin_manifest/1` (the
/// `registry.extension.plugin` inventory document).
pub const EXPORT_TARGET_PLUGIN_MANIFEST: &str = "plugin_manifest/1";

/// One exported document — the `plugin_manifest/1` canonical Json plus the
/// per-record loss report.
#[derive(Debug, Clone)]
pub struct ExportedDocument {
    /// The exported record's `version_id`.
    pub version_id: String,
    /// The canonical `plugin_manifest/1` document.
    pub document: Json,
    /// The export loss report for this record.
    pub loss_report: LossReport,
}

/// `export(refs[], target)` result — `{documents[], target}`.
#[derive(Debug, Clone)]
pub struct ExportOutcome {
    /// The export target spelling.
    pub target: String,
    /// One `plugin_manifest/1` document per input ref, in order.
    pub documents: Vec<ExportedDocument>,
}

/// `export(refs[], target)` — `target` is closed (`plugin_manifest/1` only at
/// Stage 4; `UnknownExportTarget` otherwise). Refs are `version_id`s or
/// `ns/name[@label]` selectors. Only `variant` records lower —
/// `UnsupportedExportKind` for anything else; a revoked record refuses
/// (`Revoked`). Each document is re-decoded through `manifest_from_json`
/// before it is emitted — an export that cannot re-parse itself is refused,
/// never shipped.
pub fn export(
    store: &RegistryStore,
    refs: &[String],
    target: &str,
) -> Result<ExportOutcome, RegistryError> {
    if target != EXPORT_TARGET_PLUGIN_MANIFEST {
        return Err(RegistryError::UnknownExportTarget {
            target: target.to_string(),
        });
    }
    let mut documents = Vec::with_capacity(refs.len());
    for r in refs {
        let input = match r.split_once('/') {
            Some((ns, rest)) => {
                let (name, label) = match rest.split_once('@') {
                    Some((n, l)) => (n.to_string(), Some(l.to_string())),
                    None => (rest.to_string(), None),
                };
                ResolveInput::Selector {
                    namespace: ns.to_string(),
                    name,
                    label,
                    snapshot_id: None,
                }
            }
            None => ResolveInput::Version(r.clone()),
        };
        // `Audit` mode — export is a read for re-publication, not an
        // execution-resolution; revoked rows still refuse below.
        let resolved = store.resolve(&input, ResolveMode::Audit, &ResolveRequest::default())?;
        documents.push(export_variant(store, &resolved)?);
    }
    Ok(ExportOutcome {
        target: target.to_string(),
        documents,
    })
}

fn export_variant(
    store: &RegistryStore,
    resolved: &ResolvedRecord,
) -> Result<ExportedDocument, RegistryError> {
    let v = match &resolved.record {
        RegistryRecord::Variant(v) => v,
        other => {
            return Err(RegistryError::UnsupportedExportKind {
                kind: other.kind().as_str().to_string(),
                target: EXPORT_TARGET_PLUGIN_MANIFEST.to_string(),
            })
        }
    };
    let version_id = resolved.versioned_ref.version_id.clone();
    if resolved.admission == Admission::Revoked {
        let reason = store
            .lineage(&version_id)
            .ok()
            .and_then(|v| {
                v.revocations
                    .first()
                    .map(|r| crate::store::reason_str(r.reason).to_string())
            })
            .unwrap_or_default();
        return Err(RegistryError::Revoked {
            version_id: version_id.clone(),
            reason,
        });
    }
    let mut loss = LossReport::default();

    // identity — the published name-history entry when the record carries one;
    // `local/<variant_id>` otherwise.
    let (ns, name) = match &resolved.name_entry {
        Some(e) => (e.namespace.as_str(), e.name.clone()),
        None => ("local".to_string(), v.variant_id.clone()),
    };
    let identity = PluginIdentity {
        namespace: ns,
        name,
        version_label: v.version_label.clone(),
    };

    // tier — the class's when resolvable; `C0` (inert data) when not. A class
    // outside the store exports with a loss entry rather than a fabricated
    // tier.
    let class = store.get(&v.class_ref).and_then(|(_, rec)| match rec {
        RegistryRecord::Class(c) => Some(c.clone()),
        _ => None,
    });
    let tier = match &class {
        Some(c) => c.tier.clone(),
        None => {
            loss.other.push(format!(
                "class {} not resolvable — tier lowered to `C0`",
                v.class_ref
            ));
            "C0".to_string()
        }
    };
    let depends_on = match &class {
        Some(c) => vec![ContractRef::class_contract(
            v.class_ref.clone(),
            c.contract_version.clone(),
        )],
        None => vec![],
    };

    // summary — `Text{authority ≤ external}` ceiling. A higher-authority
    // registry summary cannot occupy the slot: the verbatim `Text` rides
    // `claims.exported_summary` and the loss report names the downgrade.
    let (summary, exported_summary) = if v.summary.authority <= AuthorityClass::External {
        (v.summary.to_json(), None)
    } else {
        loss.trust_legs.push(format!(
            "summary: authority `{}` exceeds plugin_manifest/1's `external` ceiling — verbatim copy in claims.exported_summary",
            v.summary.authority.as_str()
        ));
        let prov = ProvenanceRecord::minted(
            Origin::import("registry:export", "plugin_manifest/1"),
            PersistenceScope::Run,
            0,
        );
        let t = Text::new(
            format!("component variant {} ({})", v.variant_id, version_id),
            "registry:export",
            prov,
        );
        (t.to_json(), Some(v.summary.to_json()))
    };

    // The variant body — full canonical members under `declaration.variant`
    // (the manifest's `body|content|closure|code` ban covers the
    // contribution's top-level members; a nested `variant` object is data).
    let declaration = Json::obj([
        ("class_id", Json::str(v.class_ref.clone())),
        ("variant", schema::variant_body_json(v, false)),
    ]);

    let executable = Executable {
        code_pointer: v.implementation.content.clone(),
        isolation: match v.implementation.placement {
            Placement::SubprocessConfined => IsolationClass::SubprocessConfined,
            Placement::Container => IsolationClass::Container,
            Placement::Remote => IsolationClass::Remote,
            Placement::InProcess => IsolationClass::InProcess,
            Placement::ComponentModel => {
                return Err(RegistryError::UnsupportedExportKind {
                    kind: "variant/component_model".to_string(),
                    target: EXPORT_TARGET_PLUGIN_MANIFEST.to_string(),
                })
            }
        },
        placement_preference: Some(v.implementation.placement),
        one_shot: None,
        host_requirements: v.implementation.host_requirements.clone(),
    };

    let contribution = Contribution {
        kind: ContributionKind::Variant,
        path_or_locator: PathOrLocator::PinnedLocator(SourceLocator {
            scheme: "registry".to_string(),
            credential_free_uri: format!("registry:{version_id}"),
            selector: None,
            resolved: Some(version_id.clone()),
            fetched_at: Some(0),
        }),
        declaration,
        executable: Some(executable),
        contract_range: Some(v.contract_range.clone()),
    };

    // conformance_claims — `publisher_claim` reports only (the codec refuses
    // anything else); `registry_ci`/`lab` rows are evidence, never claims, and
    // are named in the loss report rather than exported as claims.
    let mut conformance_claims = Vec::new();
    let mut non_claim_reports = 0usize;
    for c in &resolved.publisher_claims {
        if c.produced_by == ProducedBy::PublisherClaim {
            conformance_claims.push(Json::obj([
                ("produced_by", Json::str(c.produced_by.as_str())),
                (
                    "report",
                    schema::body_json(&RegistryRecord::Report(c.clone()), false),
                ),
            ]));
        } else {
            non_claim_reports += 1;
        }
    }
    if non_claim_reports > 0 {
        loss.conformance.push(format!(
            "{non_claim_reports} registry_ci/lab report(s) — schema-conformance evidence is not a publisher claim; not exported"
        ));
    }

    let mut claims = BTreeMap::new();
    claims.insert("export_of".to_string(), Json::str(version_id.clone()));
    claims.insert("variant_id".to_string(), Json::str(v.variant_id.clone()));
    if let Some(es) = exported_summary {
        claims.insert("exported_summary".to_string(), es);
    }
    claims.insert(
        "registry_admission".to_string(),
        Json::str(resolved.admission.as_str()),
    );

    let parameters = if v.param_schema.is_empty() {
        None
    } else {
        match schema::params_json(&v.param_schema) {
            Json::Obj(m) => Some(m),
            _ => None,
        }
    };

    let manifest = PluginManifest {
        identity,
        tier,
        depends_on,
        requires: Requires {
            hir_dialect: "1".to_string(),
            registry_dialect: "1".to_string(),
            plugin_abi: "1".to_string(),
            contracts: vec![],
            records: vec![PinnedRecordRef {
                version_id: v.class_ref.clone(),
                kind: Some("class".to_string()),
            }],
        },
        contributions: vec![contribution],
        requests: Requests::default(),
        claims: Json::Obj(claims),
        conformance_claims,
        parameters,
        summary,
        ext: BTreeMap::new(),
    };
    let document = manifest_to_canonical(&manifest)?;
    Ok(ExportedDocument {
        version_id,
        document,
        loss_report: loss,
    })
}

/// Encode + self-check: the emitted document must re-decode through the
/// manifest codec (an export that cannot re-parse itself is refused, never
/// shipped).
fn manifest_to_canonical(m: &PluginManifest) -> Result<Json, RegistryError> {
    let j = manifest_to_json(m);
    manifest_from_json(&j, "export").map_err(|e| RegistryError::SchemaViolation {
        path: "export".to_string(),
        detail: format!("emitted manifest fails decode: {e:?}"),
    })?;
    Ok(j)
}

// ── re-import lift ───────────────────────────────────────────────────────────

/// `lift_variant_from_manifest(document)` — the re-import side of the
/// `plugin_manifest/1` round-trip: decode the manifest, take the `variant`
/// contribution's `declaration.variant` member, and reconstitute the
/// `VariantRecord` via the same strict body codec `register` uses. A
/// byte-identical body mints the same `version_id` (AC-11 — nothing lost on
/// the wire). `claims.exported_summary` restores a verbatim summary the slot
/// could not carry.
pub fn lift_variant_from_manifest(document: &Json) -> Result<VariantRecord, RegistryError> {
    let manifest = manifest_from_json(document, "import.plugin_manifest").map_err(|e| {
        RegistryError::SchemaViolation {
            path: "import.plugin_manifest".to_string(),
            detail: format!("{e:?}"),
        }
    })?;
    let contribution = manifest
        .contributions
        .iter()
        .find(|c| c.kind == ContributionKind::Variant)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: "import.plugin_manifest.contributions".to_string(),
            detail: "no `variant` contribution".to_string(),
        })?;
    let variant_body = match contribution.declaration.get("variant") {
        Some(v) => v.clone(),
        None => {
            return Err(RegistryError::SchemaViolation {
                path: "import.plugin_manifest.contributions[].declaration".to_string(),
                detail: "missing `variant` member".to_string(),
            })
        }
    };
    let mut v: VariantRecord =
        schema::variant_from_json(&variant_body, "import.plugin_manifest.variant")?;
    // The verbatim registry summary rides `claims.exported_summary` when the
    // `external` ceiling pushed it out of the manifest slot — restore it.
    if let Some(es) = manifest.claims.get("exported_summary") {
        if let Ok(t) = Text::from_json(es, "claims.exported_summary") {
            v.summary = t;
        }
    }
    Ok(v)
}
