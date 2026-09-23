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
