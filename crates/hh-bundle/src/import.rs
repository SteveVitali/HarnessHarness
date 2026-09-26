//! `import(artefact, format, policy)` — the lifting half of the bundle
//! contract (§5h.3 §2; ADR-0141 D1/D2/D3). A foreign artefact is lifted
//! to the lossiest native form that fits, everything lifted is stamped
//! `authority = unverified` with `origin = import(...)` provenance, and
//! the `ImportRecord` + `MappingReport` are kernel-appended facts the
//! caller commits through the ledger append path. Source bundles are
//! never rewritten (D1: supersession by new version_id only).

use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass, RunKind, RunManifest};
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::manifest::{BundleManifest, MemberStatus};

/// The only format this stage lifts: the native `hh-bundle/1` container
/// (directory or `HHB1`). Foreign formats (`harbor_atif`, `inspect_eval`,
/// `sarif`, `generic_jsonl`) land with their own tickets.
pub const NATIVE_FORMAT: &str = "ledger_native";

/// The foreign formats S4.2 lifts (§5h.3 §2 `import`; AC-R-2.9.3-9):
/// `harbor_trial_dir` — a foreign trial directory tree; `harbor_job_dir`
/// — our own harbor export shape re-imported (the AC-8 round-trip).
/// The remaining targets (`inspect_eval_log`, `swebench_submission`,
/// `in_toto_bundle`, `telemetry_trace`) land with their owning tickets —
/// `FormatUnknown`, never a guess.
pub const FOREIGN_FORMATS: [&str; 2] = ["harbor_trial_dir", "harbor_job_dir"];

/// One ledger row the import wants appended — provenance is applied by
/// the caller at append time (every lifted row carries the import-minted
/// `unverified` provenance).
#[derive(Debug, Clone)]
pub struct ImportEvent {
    /// The event class — must be a class the ledger accepts.
    pub class: String,
    /// The lifted payload.
    pub payload: Json,
    /// Content-address refs the row carries.
    pub refs: Vec<String>,
}

/// Everything the caller needs to commit an import: the lifted manifest
/// for `open_run`, the member bytes for `put_blob`, the event rows for
/// `append`, and the `ImportRecord`/`MappingReport` for the audit trail.
pub struct ImportLift {
    /// The lifted `RunManifest` to hand to `Store::open_run`.
    pub run_manifest: RunManifest,
    /// Member bytes to persist via `Store::put_blob` (address → bytes).
    pub blobs: Vec<(String, Vec<u8>)>,
    /// Event rows to append to the new run (provenance applied by caller).
    pub events: Vec<ImportEvent>,
    /// The import-minted provenance (`authority = unverified`).
    pub provenance: ProvenanceRecord,
    /// The `ImportRecord` document (content-addressed).
    pub import_record: Json,
    /// The `MappingReport` member of the import record.
    pub mapping_report: Json,
}

/// `import(artefact, format = ledger_native, policy)` — lift a decoded
/// `hh-bundle/1` bundle into an import plan (§5h.3 §2 `import`; ADR-0141
/// D1: every lifted fact gets `authority = unverified` and
/// `origin = import(...)`; D2: `MappingReport` is a mandatory,
/// content-addressed row; D3: source bytes are never rewritten).
pub fn lift(decoded: &Decoded, format: &str, now_ms: u64) -> Result<ImportLift, BundleError> {
    if format != NATIVE_FORMAT {
        return Err(BundleError::FormatUnknown {
            detail: format!("{format} — only {NATIVE_FORMAT} lifts at this stage"),
        });
    }
    let manifest = &decoded.manifest;
    if manifest.bundle_kind != "run" {
        return Err(BundleError::FormatUnknown {
            detail: format!(
                "bundle_kind {} — only run bundles lift at this stage",
                manifest.bundle_kind
            ),
        });
    }

    let provenance = ProvenanceRecord::minted(
        Origin::import("hh-bundle/1", "hh-bundle/1"),
        PersistenceScope::Run,
        now_ms,
    );
    let prov_json = provenance.to_json();

    // ── Lifted RunManifest ───────────────────────────────────────────
    let mut rm = RunManifest::minimal(RunKind::Agent);
    let cfg = &manifest.configuration;
    rm.configuration_id = cfg
        .get("configuration_id")
        .and_then(Json::as_str)
        .map(String::from);
    rm.configuration_version_id = cfg
        .get("configuration_version_id")
        .and_then(Json::as_str)
        .map(String::from);
    rm.harness_def_ref = manifest
        .definition
        .get("version_id")
        .and_then(Json::as_str)
        .map(String::from);
    rm.seed = manifest
        .configuration
        .get("seed")
        .and_then(Json::as_int)
        .map(|s| s as u64);
    rm.participant_class =
        ParticipantClass::parse(&manifest.participant_class).unwrap_or(ParticipantClass::Native);
    rm.observability_level = manifest
        .observability_levels
        .iter()
        .filter_map(|s| ObservabilityLevel::parse(s))
        .collect();
    rm.observability_level.insert(ObservabilityLevel::Ledger);
    rm.extra.insert(
        "import".into(),
        Json::obj([
            ("source_bundle", Json::str(manifest.version_id.clone())),
            ("format", Json::str(NATIVE_FORMAT)),
            (
                "participant_class",
                Json::str(manifest.participant_class.clone()),
            ),
        ]),
    );

    // ── Member bytes → blob-plane payloads ───────────────────────────
    let blobs: Vec<(String, Vec<u8>)> = decoded_members(decoded, manifest).collect();

    // ── Event rows ───────────────────────────────────────────────────
    // Provenance is stamped by the caller at append time; the payload
    // rows carry only refs/coordinates (imported facts stay `unverified`
    // through the provenance the caller stamps). Foreign envelopes are
    // NEVER re-minted under their original classes: audit-grade and
    // kernel-origin classes reject `authority < kernel` / `origin !=
    // kernel` provenance by construction, so a verbatim replay would be
    // both a refusal and a lie — the exported pages stay available as
    // blob members, and the receipt row names their coordinates.
    let source_heads: Vec<Json> = manifest
        .subject
        .heads
        .iter()
        .map(|(run, head)| Json::obj([("run_id", Json::str(run.clone())), ("head", head.clone())]))
        .collect();
    let mut receipt_refs: Vec<String> = Vec::new();
    if let Some(a) = manifest.definition.get("member").and_then(Json::as_str) {
        receipt_refs.push(a.to_string());
    }
    let events = vec![ImportEvent {
        class: "lifecycle.run.imported".into(),
        payload: Json::obj([
            ("source_bundle", Json::str(manifest.version_id.clone())),
            (
                "source_run_ids",
                Json::Arr(
                    manifest
                        .subject
                        .run_ids
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ),
            ("heads", Json::Arr(source_heads)),
            (
                "definition",
                manifest
                    .definition
                    .get("version_id")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
            ("member_count", Json::Int(manifest.members.len() as i64)),
            (
                "participant_class",
                Json::str(manifest.participant_class.clone()),
            ),
        ]),
        refs: receipt_refs,
    }];

    // ── MappingReport + ImportRecord ─────────────────────────────────
    let mut mapped = Vec::new();
    let mut unmapped = Vec::new();
    for (run, export) in &manifest.traces {
        mapped.push(format!("traces.{run} ({} pages)", export.pages.len()));
    }
    for m in &manifest.members {
        if matches!(m.status, MemberStatus::Redacted | MemberStatus::Gc) {
            unmapped.push(format!("{}: {}", m.role, m.status.name()));
        }
    }
    for u in &manifest.unpinned {
        unmapped.push(format!("unpinned:{} ({})", u.role, u.reason));
    }
    let mut dropped: Vec<String> = Vec::new();
    for key in manifest.ext.keys() {
        dropped.push(format!("ext.{key}"));
    }
    let mapping = Json::obj([
        ("format", Json::str(NATIVE_FORMAT)),
        (
            "mapped",
            Json::Arr(mapped.iter().map(|m| Json::str(m.clone())).collect()),
        ),
        (
            "unmapped",
            Json::Arr(unmapped.iter().map(|u| Json::str(u.clone())).collect()),
        ),
        (
            "dropped",
            Json::Arr(dropped.iter().map(|d| Json::str(d.clone())).collect()),
        ),
        ("notes", Json::Arr(vec![])),
    ]);
    let mapping_digest =
        hh_identity::idp_id("bundle.mapping", mapping.to_canonical_string().as_bytes());
    let import_id = hh_identity::idp_id(
        "bundle.import",
        format!("{}:{}", manifest.version_id, now_ms).as_bytes(),
    );
    let import_record = Json::obj([
        ("schema", Json::str("hh-import/1")),
        ("import_id", Json::str(import_id)),
        ("artefact", Json::str(manifest.version_id.clone())),
        ("format", Json::str(NATIVE_FORMAT)),
        ("mapping_report", mapping.clone()),
        (
            "imported_refs",
            Json::Arr(
                manifest
                    .members
                    .iter()
                    .map(|m| Json::str(m.address.clone()))
                    .collect(),
            ),
        ),
        ("mapping_report_ref", Json::str(mapping_digest)),
        ("imported_at", Json::Int(now_ms as i64)),
        ("provenance", prov_json),
    ]);

    Ok(ImportLift {
        run_manifest: rm,
        blobs,
        events,
        provenance,
        import_record,
        mapping_report: mapping,
    })
}

fn decoded_members<'a>(
    decoded: &'a Decoded,
    manifest: &'a BundleManifest,
) -> impl Iterator<Item = (String, Vec<u8>)> + 'a {
    manifest
        .members
        .iter()
        .filter(|m| m.status == MemberStatus::Present)
        .filter_map(|m| {
            decoded
                .members
                .get(&m.address)
                .map(|b| (m.address.clone(), b.clone()))
        })
}

// ── `import(artefact, format ∈ FOREIGN_FORMATS)` — the foreign-trial
// lift (§5h.3 §2; AC-R-2.9.3-8/-9; S4.2) ────────────────────────────────

/// Everything a foreign import commits: the hosted `R0` bundle (the
/// foreign artefact's lifted form — `participant_class = hosted`,
/// `claimed_level = R0`, every lifted fact `authority = unverified`),
/// the `ImportRecord` and the `MappingReport`.
pub struct ForeignLift {
    /// The lifted bundle (`Decoded` — manifest + member bytes).
    pub bundle: Decoded,
    /// The `ImportRecord` document (content-addressed).
    pub import_record: Json,
    /// The `MappingReport` — `mapped`/`unmapped`/`dropped`/`conflicts`
    /// (`ForeignIntegrityMismatch` rows are warning-level claim
    /// conflicts, never coerced).
    pub mapping_report: Json,
}

/// A path is a "lock" when the foreign tree pins dependencies there —
/// the member the AC names ("the foreign lock kept as a member") plus
/// the digest claims.
fn is_lock_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".lock")
        || matches!(
            name,
            "requirements.txt"
                | "package-lock.json"
                | "poetry.lock"
                | "Pipfile.lock"
                | "uv.lock"
                | "Cargo.lock"
                | "environment.yml"
                | "digests.json"
                | "manifest.json"
        )
}

/// `lift_foreign(store, run_id, files, format, …)` — lift a foreign
/// trial directory (already imported as a hosted run — the caller
/// opened `run_id` and appended the lifted rows) into a hosted `R0`
/// bundle: every foreign file is a `foreign:<path>` member, every lock
/// digest a `foreign` claim, declared digests that fail against the
/// supplied bytes land as `ForeignIntegrityMismatch` claim conflicts
/// (never coerced, never silently dropped — AC-8/AC-9).
#[allow(clippy::too_many_arguments)] // the lift's coordinates are the record's shape.
pub fn lift_foreign(
    store: &hh_ledger::store::Store,
    run_id: &str,
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    format: &str,
    producer: Json,
    created_at: String,
    kernel_version_id: &str,
    now_ms: u64,
) -> Result<ForeignLift, BundleError> {
    if !FOREIGN_FORMATS.contains(&format) {
        return Err(BundleError::FormatUnknown {
            detail: format!("{format} — supported foreign lifts: {FOREIGN_FORMATS:?}"),
        });
    }
    let provenance = ProvenanceRecord::minted(
        Origin::import(format, "hh-bundle/1"),
        PersistenceScope::Run,
        now_ms,
    );
    let prov_json = provenance.to_json();

    let mut members: crate::export::MemberBytes = std::collections::BTreeMap::new();
    let mut index: Vec<crate::manifest::MemberRef> = Vec::new();
    let doc_member = |members: &mut crate::export::MemberBytes,
                      index: &mut Vec<crate::manifest::MemberRef>,
                      role: &str,
                      doc: &Json| {
        let bytes = doc.to_canonical_string().into_bytes();
        let (addr, size) =
            crate::export::add_member(members, bytes, "application/vnd.hh.bundle-doc+json");
        index.push(crate::manifest::MemberRef::present(
            role,
            addr.clone(),
            "application/vnd.hh.bundle-doc+json",
            size,
        ));
        addr
    };

    // ── foreign files → members + digest claims ──────────────────────
    let mut claims: Vec<crate::manifest::Claim> = Vec::new();
    let mut conflicts: Vec<Json> = Vec::new();
    let mut declared_digests: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    // `digests.json` (our harbor shape) / `*.sha256` files declare foreign
    // digests — collect them before verifying.
    for (path, bytes) in files {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == "digests.json" || name == "checksums.json" {
            if let Some(Json::Obj(m)) = std::str::from_utf8(bytes)
                .ok()
                .and_then(|t| hh_wire::json::parse(t).ok())
            {
                for (k, v) in &m {
                    if let Some(d) = v.as_str() {
                        declared_digests.insert(k.clone(), d.to_string());
                    }
                }
            }
        }
        if let Some(hex) = name.strip_suffix(".sha256") {
            if let Ok(text) = std::str::from_utf8(bytes) {
                declared_digests.insert(hex.to_string(), text.trim().to_string());
            }
        }
    }
    for (path, bytes) in files {
        let (addr, size) =
            crate::export::add_member(&mut members, bytes.clone(), "application/octet-stream");
        index.push(crate::manifest::MemberRef::present(
            format!("foreign:{path}"),
            addr.clone(),
            "application/octet-stream",
            size,
        ));
        // Every file's content address is a `foreign` claim — the
        // lock-file digests AC-9 names are the same claim shape.
        let tag = if is_lock_file(path) {
            "foreign_lock_digest"
        } else {
            "foreign_digest"
        };
        claims.push(crate::manifest::Claim {
            role: format!("{tag}:{path}"),
            value: Json::str(addr.clone()),
            provenance: prov_json.clone(),
        });
        // A declared foreign digest that fails against the supplied
        // bytes is a recorded claim conflict (`ForeignIntegrityMismatch`
        // — warning-level, never coerced).
        if let Some(declared) = declared_digests.get(path.as_str()) {
            let bare = |s: &str| s.rsplit(':').next().unwrap_or(s).to_string();
            let recomputed = hh_identity::idp_id("blob", bytes);
            if bare(declared) != bare(&recomputed) && *declared != recomputed {
                conflicts.push(crate::fetch::integrity_conflict(
                    path,
                    declared,
                    &recomputed,
                ));
            }
        }
    }

    // ── the required section docs (lifted projections — the foreign
    //    fields have no typed home beyond `foreign` claims) ───────────
    let rm = store
        .manifest(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;
    let (export, page_roles) = crate::export::build_ledger_export(store, run_id, &mut members)?;
    for (role, addr, size) in page_roles {
        index.push(crate::manifest::MemberRef::present(
            role,
            addr,
            "application/vnd.hh.ledger-page+json",
            size,
        ));
    }
    let mut traces = std::collections::BTreeMap::new();
    traces.insert(run_id.to_string(), export.clone());
    let head = store.head(run_id).map_err(|_| BundleError::RunNotFound {
        run_id: run_id.to_string(),
    })?;
    let subject = crate::manifest::SubjectSection {
        run_ids: vec![run_id.to_string()],
        heads: [(
            run_id.to_string(),
            Json::obj([
                ("seq", Json::Int(head.seq as i64)),
                ("event_id", Json::str(head.event_id.clone())),
                ("hash", Json::str(head.hash.clone())),
            ]),
        )]
        .into_iter()
        .collect(),
        lineage: vec![],
        watermarks: [(run_id.to_string(), head.seq)].into_iter().collect(),
        status: "finished".into(),
        experiment: std::collections::BTreeMap::new(),
    };
    doc_member(
        &mut members,
        &mut index,
        "subject",
        &Json::obj([
            ("run_ids", Json::Arr(vec![Json::str(run_id)])),
            ("status", Json::str("finished")),
        ]),
    );
    let def_addr = doc_member(
        &mut members,
        &mut index,
        "definition",
        &Json::obj([
            ("kind", Json::str("foreign")),
            ("format", Json::str(format)),
            ("source_run", Json::str(run_id)),
        ]),
    );
    let cfg_addr = doc_member(
        &mut members,
        &mut index,
        "configuration",
        &Json::obj([
            ("foreign", Json::Bool(true)),
            ("seed", Json::Null),
            ("budget", Json::Null),
        ]),
    );
    let deps_addr = doc_member(
        &mut members,
        &mut index,
        "resolved_dependencies",
        &Json::obj([
            ("foreign", Json::Bool(true)),
            ("registry_snapshot_id", Json::Null),
            ("run_refs", Json::obj([])),
        ]),
    );
    doc_member(
        &mut members,
        &mut index,
        "model",
        &Json::obj([("snapshots", Json::Arr(vec![]))]),
    );
    doc_member(
        &mut members,
        &mut index,
        "environment",
        &Json::obj([("foreign", Json::Bool(true))]),
    );
    doc_member(
        &mut members,
        &mut index,
        "instrument",
        &Json::obj([
            ("version", Json::str(kernel_version_id)),
            ("dirty", Json::Bool(false)),
            ("foreign", Json::Bool(true)),
        ]),
    );
    let results_addr = doc_member(
        &mut members,
        &mut index,
        "results",
        &Json::obj([
            ("rows", Json::Arr(vec![])),
            ("metric_declarations", Json::Arr(vec![])),
            ("oracle_declarations", Json::Arr(vec![])),
            ("foreign", Json::Bool(true)),
        ]),
    );
    let nd_addr = doc_member(
        &mut members,
        &mut index,
        "nondeterminism",
        &Json::obj([("declarations", Json::Arr(vec![]))]),
    );
    let definition = Json::obj([
        ("kind", Json::str("foreign")),
        ("format", Json::str(format)),
        ("member", Json::str(def_addr)),
    ]);
    let configuration = Json::obj([
        ("foreign", Json::Bool(true)),
        ("member", Json::str(cfg_addr)),
        ("budget", Json::Null),
        ("seed", Json::Null),
    ]);
    let resolved_dependencies = Json::obj([
        ("foreign", Json::Bool(true)),
        ("member", Json::str(deps_addr)),
        ("run_refs", Json::obj([])),
    ]);

    let mut manifest = BundleManifest {
        bundle_kind: "run".into(),
        created_at,
        producer,
        participant_class: "hosted".into(),
        observability_levels: vec!["ledger".into()],
        claims,
        name_bindings: vec![],
        fetch_policy: "self_contained".into(),
        fetch: vec![],
        subject,
        definition,
        configuration,
        resolved_dependencies,
        model: Json::obj([("snapshots", Json::Arr(vec![]))]),
        instrument: Json::obj([
            ("version", Json::str(kernel_version_id)),
            ("dirty", Json::Bool(false)),
            ("foreign", Json::Bool(true)),
        ]),
        traces,
        results: Json::obj([
            ("rows", Json::Arr(vec![])),
            ("metric_declarations", Json::Arr(vec![])),
            ("oracle_declarations", Json::Arr(vec![])),
            ("member", Json::str(results_addr)),
        ]),
        composition: Json::Null,
        reproducibility: Json::Null,
        members: index,
        // The foreign environment/model are `unpinned{foreign_only}` —
        // claims satisfy nothing above R0 (R-ID-2).
        unpinned: vec![
            crate::manifest::Unpinned {
                role: "environment".into(),
                reason: "foreign_only".into(),
                claim: None,
            },
            crate::manifest::Unpinned {
                role: "model".into(),
                reason: "foreign_only".into(),
                claim: None,
            },
        ],
        ext: std::collections::BTreeMap::new(),
        version_id: String::new(),
    };
    let (max_supported, basis) = crate::levels::derive(&manifest, false);
    // The lifted claim is always R0 — the foreign facts are `unverified`
    // (AC-9), whatever the basis derives.
    manifest.reproducibility = Json::obj([
        ("claimed_level", Json::str("R0")),
        ("max_supported_level", Json::str(max_supported.name())),
        (
            "basis",
            Json::Arr(basis.iter().map(|b| b.to_json()).collect()),
        ),
        ("nondeterminism", Json::str(nd_addr)),
    ]);
    manifest.version_id = manifest.compute_id();
    crate::assemble::secret_scan(&members)?;

    // ── MappingReport + ImportRecord ─────────────────────────────────
    let mapped: Vec<String> = files.keys().map(|p| format!("foreign:{p}")).collect();
    // `mapping_loss` — the members the foreign shape could not express.
    // For `harbor_job_dir` re-imports the carried `manifest.json`'s own
    // member index is the source of truth: every member role that is not
    // a ledger page/tree arrives only as a manifest ref → `no_slot`.
    let mut mapping_loss: Vec<Json> = Vec::new();
    if format == "harbor_job_dir" {
        if let Some(src) = files
            .get("manifest.json")
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|t| hh_wire::json::parse(t).ok())
        {
            if let Some(Json::Arr(ms)) = src.get("members") {
                for m in ms {
                    let role = m.get("role").and_then(Json::as_str).unwrap_or("");
                    if role.is_empty()
                        || role.starts_with("ledger_page:")
                        || role.starts_with("ledger_tree:")
                    {
                        continue;
                    }
                    mapping_loss.push(Json::obj([
                        ("member", Json::str(role)),
                        ("class", Json::str("no_slot")),
                        (
                            "detail",
                            Json::str("arrived only as a manifest ref inside harbor_job_dir"),
                        ),
                    ]));
                }
            }
        }
    }
    let mapping = Json::obj([
        ("format", Json::str(format)),
        (
            "mapped",
            Json::Arr(mapped.iter().map(|m| Json::str(m.clone())).collect()),
        ),
        ("unmapped", Json::Arr(vec![])),
        ("dropped", Json::Arr(vec![])),
        ("conflicts", Json::Arr(conflicts)),
        ("mapping_loss", Json::Arr(mapping_loss)),
    ]);
    let mapping_digest =
        hh_identity::idp_id("bundle.mapping", mapping.to_canonical_string().as_bytes());
    let import_id = hh_identity::idp_id(
        "bundle.import",
        format!("{}:{}", manifest.version_id, now_ms).as_bytes(),
    );
    let import_record = Json::obj([
        ("schema", Json::str("hh-import/1")),
        ("import_id", Json::str(import_id)),
        ("artefact", Json::str(manifest.version_id.clone())),
        ("format", Json::str(format)),
        ("mapping_report", mapping.clone()),
        ("mapping_report_ref", Json::str(mapping_digest)),
        (
            "imported_refs",
            Json::Arr(
                manifest
                    .members
                    .iter()
                    .map(|m| Json::str(m.address.clone()))
                    .collect(),
            ),
        ),
        ("participant_class", Json::str("hosted")),
        ("claimed_level", Json::str("R0")),
        ("imported_at", Json::Int(now_ms as i64)),
        ("provenance", prov_json),
    ]);
    let _ = rm;
    Ok(ForeignLift {
        bundle: Decoded { manifest, members },
        import_record,
        mapping_report: mapping,
    })
}
