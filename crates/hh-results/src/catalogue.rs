//! The bundle catalogue — header-only `BundleCatalogueEntry` records over
//! `measurement.experiment.bundle_assembled` facts + the bundle manifests
//! in the blob pool (§6.5 §2.1 `catalogue_refresh`; ADR-0161 D3 "header-only
//! entries"; ADR-0139). The catalogue never duplicates payload bytes — it is
//! the `bundle_refs[]` source for rows and the L1 admission input.

use std::collections::BTreeMap;
use std::fs;

use hh_bundle::export::MemberBytes;
use hh_bundle::manifest::{BundleManifest, MemberStatus, ReproLevel};
use hh_bundle::validate;
use hh_identity::idp::{idp_id, parse_id, ContentAddress};
use hh_ledger::store::Store;
use hh_wire::json::{parse as json_parse, Json};

use crate::error::ResultsError;
use crate::store::ResultsStore;
use crate::watermark::WatermarkSet;

/// The catalogue schema tag.
pub const CATALOGUE_SCHEMA: &str = "hh-bundle-catalogue/1";

/// `status ∈ {assembled, validated, attested, reproduced}` — the admission
/// ladder (`attested`/`reproduced` are Stage-4+ productions; the ordering is
/// fixed so `status ≥ min` comparisons are total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BundleStatus {
    /// Assembled, not (or not yet / no longer) validated.
    Assembled,
    /// `hh_bundle::validate` passes over materialized members.
    Validated,
    /// Attested (Stage 4).
    Attested,
    /// Independently reproduced (Stage 4+).
    Reproduced,
}

impl BundleStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BundleStatus::Assembled => "assembled",
            BundleStatus::Validated => "validated",
            BundleStatus::Attested => "attested",
            BundleStatus::Reproduced => "reproduced",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<BundleStatus> {
        Some(match s {
            "assembled" => BundleStatus::Assembled,
            "validated" => BundleStatus::Validated,
            "attested" => BundleStatus::Attested,
            "reproduced" => BundleStatus::Reproduced,
            _ => return None,
        })
    }
}

/// `BundleCatalogueEntry{bundle_id, kind, subject_runs[],
/// configuration_ids[], experiment_run_id?, claimed_level,
/// max_supported_level, status, readers[], name_bindings[], size,
/// created_at, producer, level_now}` — header only (§6.5 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleCatalogueEntry {
    /// The bundle id (`manifest.version_id` — the manifest's own blob
    /// address).
    pub bundle_id: String,
    /// `run | arm | experiment | lineage` (`run` at Stage 3).
    pub kind: String,
    /// The runs the bundle covers.
    pub subject_runs: Vec<String>,
    /// The configuration ids the bundle binds.
    pub configuration_ids: Vec<String>,
    /// The binding experiment run, when the subject carries one.
    pub experiment_run_id: Option<String>,
    /// The declared claimed level.
    pub claimed_level: Option<ReproLevel>,
    /// The derived maximum.
    pub max_supported_level: Option<ReproLevel>,
    /// The catalogue status (validated ⟺ `hh_bundle::validate` passes over
    /// the materialized members at refresh).
    pub status: BundleStatus,
    /// The declared readers (S8 reader checks are Stage-4; recorded).
    pub readers: Vec<String>,
    /// Display-name bindings — never identity (R-ID-4).
    pub name_bindings: Vec<Json>,
    /// Total member bytes.
    pub size: u64,
    /// `created_at` from the manifest.
    pub created_at: String,
    /// The producer record.
    pub producer: Json,
    /// Whether the manifest bytes were in the pool at refresh (a GC'd
    /// manifest keeps the entry — annotate, never hide; `level_now` drops).
    pub manifest_available: bool,
    /// `level_now` — the highest level whose basis members still resolve
    /// `present` in the pool (GC/redaction lowers it; the entry remains —
    /// L1's "never removes an entry").
    pub level_now: Option<ReproLevel>,
}

impl BundleCatalogueEntry {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("bundle_id".into(), Json::str(&self.bundle_id));
        m.insert("kind".into(), Json::str(&self.kind));
        m.insert(
            "subject_runs".into(),
            Json::Arr(self.subject_runs.iter().map(Json::str).collect()),
        );
        m.insert(
            "configuration_ids".into(),
            Json::Arr(self.configuration_ids.iter().map(Json::str).collect()),
        );
        if let Some(e) = &self.experiment_run_id {
            m.insert("experiment_run_id".into(), Json::str(e));
        }
        if let Some(l) = self.claimed_level {
            m.insert("claimed_level".into(), Json::str(l.name()));
        }
        if let Some(l) = self.max_supported_level {
            m.insert("max_supported_level".into(), Json::str(l.name()));
        }
        m.insert("status".into(), Json::str(self.status.as_str()));
        m.insert(
            "readers".into(),
            Json::Arr(self.readers.iter().map(Json::str).collect()),
        );
        m.insert(
            "name_bindings".into(),
            Json::Arr(self.name_bindings.clone()),
        );
        m.insert("size".into(), Json::Int(self.size as i64));
        m.insert("created_at".into(), Json::str(&self.created_at));
        m.insert("producer".into(), self.producer.clone());
        m.insert(
            "manifest_available".into(),
            Json::Bool(self.manifest_available),
        );
        m.insert(
            "level_now".into(),
            self.level_now.map_or(Json::Null, |l| Json::str(l.name())),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Option<BundleCatalogueEntry> {
        let Json::Obj(m) = j else {
            return None;
        };
        let strs = |name: &str| -> Vec<String> {
            match m.get(name) {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            }
        };
        Some(BundleCatalogueEntry {
            bundle_id: m.get("bundle_id")?.as_str()?.to_string(),
            kind: m.get("kind")?.as_str()?.to_string(),
            subject_runs: strs("subject_runs"),
            configuration_ids: strs("configuration_ids"),
            experiment_run_id: m
                .get("experiment_run_id")
                .and_then(Json::as_str)
                .map(String::from),
            claimed_level: m
                .get("claimed_level")
                .and_then(Json::as_str)
                .and_then(ReproLevel::parse),
            max_supported_level: m
                .get("max_supported_level")
                .and_then(Json::as_str)
                .and_then(ReproLevel::parse),
            status: BundleStatus::parse(m.get("status")?.as_str()?)?,
            readers: strs("readers"),
            name_bindings: match m.get("name_bindings") {
                Some(Json::Arr(items)) => items.clone(),
                _ => Vec::new(),
            },
            size: m.get("size").and_then(Json::as_int).unwrap_or(0) as u64,
            created_at: m
                .get("created_at")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            producer: m.get("producer").cloned().unwrap_or(Json::Null),
            manifest_available: matches!(
                m.get("manifest_available"),
                Some(Json::Bool(true)) | None
            ),
            level_now: m
                .get("level_now")
                .and_then(Json::as_str)
                .and_then(ReproLevel::parse),
        })
    }
}

/// `BundleCatalogue @ watermark` — `{entries[], watermark_set, view_hash}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BundleCatalogue {
    /// The entries, `bundle_id`-ordered.
    pub entries: Vec<BundleCatalogueEntry>,
    /// The watermark set the refresh scanned at (`run → head` for every run
    /// scanned — the refresh's coverage bound).
    pub watermark_set: WatermarkSet,
    /// `idp/1` over `{entries, watermark_set}`.
    pub view_hash: String,
}

impl BundleCatalogue {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(CATALOGUE_SCHEMA)),
            (
                "entries",
                Json::Arr(self.entries.iter().map(|e| e.to_json()).collect()),
            ),
            ("watermark_set", self.watermark_set.to_json()),
            ("view_hash", Json::str(&self.view_hash)),
        ])
    }

    /// Decode a persisted catalogue.
    pub fn from_json(j: &Json) -> Option<BundleCatalogue> {
        let Json::Obj(m) = j else {
            return None;
        };
        let entries = match m.get("entries") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(BundleCatalogueEntry::from_json)
                .collect::<Option<Vec<_>>>()?,
            _ => Vec::new(),
        };
        Some(BundleCatalogue {
            entries,
            watermark_set: WatermarkSet::from_json(m.get("watermark_set")?)?,
            view_hash: m.get("view_hash")?.as_str()?.to_string(),
        })
    }
}

fn catalogue_path(results: &ResultsStore) -> std::path::PathBuf {
    results.root().join("catalogue.json")
}

/// The persisted catalogue (empty when never refreshed).
pub fn load(results: &ResultsStore) -> Result<BundleCatalogue, ResultsError> {
    let path = catalogue_path(results);
    if !path.exists() {
        return Ok(BundleCatalogue::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| ResultsError::Store {
        detail: format!("read catalogue: {e}"),
    })?;
    let j = json_parse(&text).map_err(|e| ResultsError::Store {
        detail: format!("parse catalogue: {e:?}"),
    })?;
    BundleCatalogue::from_json(&j).ok_or_else(|| ResultsError::Store {
        detail: "catalogue decode failed".into(),
    })
}

fn blob_addr(id: &str) -> Option<ContentAddress> {
    parse_id(id).ok().map(|p| ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest: p.digest_hex,
        media_type: String::new(),
        size: 0,
    })
}

/// A `ContentAddress` naming a pool digest directly (the blob file's name
/// is the `blob`-domain digest of its bytes).
fn digest_addr(digest: &str) -> ContentAddress {
    ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest: digest.to_string(),
        media_type: String::new(),
        size: 0,
    }
}

/// `version_id → blob digest` over the pool — every decodable
/// `hh-bundle/1` manifest indexed by its recomputed identity.
fn manifest_index(store: &Store) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for digest in store.blob_digests() {
        let Ok(bytes) = store.get_blob(&digest_addr(&digest)) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let Ok(j) = json_parse(&text) else {
            continue;
        };
        let Ok(m) = BundleManifest::from_json(&j) else {
            continue;
        };
        out.insert(m.compute_id(), digest);
    }
    out
}

/// `catalogue_refresh(bundle_ids | all)` — fold every
/// `measurement.experiment.bundle_assembled` fact, decode the manifest
/// headers, and validate over materialized members. `bundle_ids = None`
/// scans every run (`all`); `Some(ids)` refreshes exactly those entries,
/// preserving the rest of the persisted catalogue.
pub fn refresh(
    results: &ResultsStore,
    store: &Store,
    bundle_ids: Option<&[String]>,
) -> Result<BundleCatalogue, ResultsError> {
    // The assembly facts — `{bundle_id, run_id, kind}` per emission; a run
    // may assemble repeatedly (each assembly is a distinct bundle id).
    let mut facts: BTreeMap<String, (String, String)> = BTreeMap::new(); // bundle → (run, kind)
    let mut watermark_set = WatermarkSet::new();
    for run_id in store.run_ids() {
        let head = match store.head(&run_id) {
            Ok(h) => h.seq,
            Err(_) => continue,
        };
        watermark_set.pin(&run_id, head);
        let Ok(events) = store.envelopes(&run_id) else {
            continue;
        };
        for e in events {
            if e.class != "measurement.experiment.bundle_assembled" {
                continue;
            }
            let bid = e
                .payload
                .get("bundle_id")
                .or_else(|| e.payload.get("version_id"))
                .and_then(Json::as_str);
            let kind = e
                .payload
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("run");
            if let Some(bid) = bid {
                facts.insert(bid.to_string(), (run_id.clone(), kind.to_string()));
            }
        }
    }

    // Resolve `bundle_id → manifest` over the pool once — a bundle's
    // `version_id` is a derived identity (`idp` domain `bundle` over the
    // manifest preimage), never the manifest blob's `blob`-domain address,
    // so the catalogue indexes manifests by recomputation (the catalogue
    // is the derived index that amortizes the pool pass per refresh).
    let manifests = manifest_index(store);

    let mut entries: Vec<BundleCatalogueEntry> = Vec::new();
    match bundle_ids {
        None => {
            for bid in facts.keys() {
                entries.push(entry_for(store, bid, facts.get(bid), &manifests));
            }
        }
        Some(ids) => {
            let prior = load(results)?;
            let wanted: std::collections::BTreeSet<&str> = ids.iter().map(|s| s.as_str()).collect();
            for e in prior.entries {
                if !wanted.contains(e.bundle_id.as_str()) {
                    entries.push(e);
                }
            }
            for bid in ids {
                entries.push(entry_for(store, bid, facts.get(bid), &manifests));
            }
        }
    }
    entries.sort_by(|a, b| a.bundle_id.cmp(&b.bundle_id));

    let mut cat = BundleCatalogue {
        entries,
        watermark_set,
        view_hash: String::new(),
    };
    let mut pre = cat.to_json();
    if let Json::Obj(ref mut m) = pre {
        m.remove("view_hash");
    }
    cat.view_hash = idp_id("results_view", pre.to_canonical_string().as_bytes());
    write_catalogue(results, &cat)?;
    Ok(cat)
}

fn write_catalogue(results: &ResultsStore, cat: &BundleCatalogue) -> Result<(), ResultsError> {
    let path = catalogue_path(results);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, cat.to_json().to_canonical_string()).map_err(|e| ResultsError::Store {
        detail: format!("write catalogue: {e}"),
    })?;
    fs::rename(&tmp, &path).map_err(|e| ResultsError::Store {
        detail: format!("rename catalogue: {e}"),
    })?;
    Ok(())
}

/// Build one header entry: decode the manifest blob, run `validate` over
/// the materialized member bytes, compute `level_now` over current member
/// presence.
fn entry_for(
    store: &Store,
    bundle_id: &str,
    fact: Option<&(String, String)>,
    manifests: &BTreeMap<String, String>,
) -> BundleCatalogueEntry {
    let emitter = fact.map(|(r, _)| r.clone());
    let kind = fact.map(|(_, k)| k.clone()).unwrap_or_else(|| "run".into());
    let mut entry = BundleCatalogueEntry {
        bundle_id: bundle_id.to_string(),
        kind,
        subject_runs: emitter.clone().into_iter().collect(),
        configuration_ids: Vec::new(),
        experiment_run_id: None,
        claimed_level: None,
        max_supported_level: None,
        status: BundleStatus::Assembled,
        readers: Vec::new(),
        name_bindings: Vec::new(),
        size: 0,
        created_at: String::new(),
        producer: Json::Null,
        manifest_available: false,
        level_now: None,
    };
    let Some(digest) = manifests.get(bundle_id) else {
        return entry;
    };
    let bytes = match store.get_blob(&digest_addr(digest)) {
        Ok(b) => b,
        Err(_) => return entry,
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return entry;
    };
    let Ok(j) = json_parse(&text) else {
        return entry;
    };
    let Ok(manifest) = BundleManifest::from_json(&j) else {
        return entry;
    };
    entry.manifest_available = true;
    entry.kind = manifest.bundle_kind.clone();
    entry.subject_runs = manifest.subject.run_ids.clone();
    entry.created_at = manifest.created_at.clone();
    entry.producer = manifest.producer.clone();
    entry.name_bindings = manifest.name_bindings.clone();
    entry.size = manifest.members.iter().map(|m| m.size).sum();
    entry.claimed_level = manifest
        .reproducibility
        .get("claimed_level")
        .and_then(Json::as_str)
        .and_then(ReproLevel::parse);
    entry.max_supported_level = manifest
        .reproducibility
        .get("max_supported_level")
        .and_then(Json::as_str)
        .and_then(ReproLevel::parse);
    if let Some(cid) = manifest
        .configuration
        .get("configuration_id")
        .and_then(Json::as_str)
    {
        entry.configuration_ids.push(cid.to_string());
    }
    if let Some(Json::Arr(ids)) = manifest.configuration.get("configuration_ids") {
        for i in ids {
            if let Some(s) = i.as_str() {
                entry.configuration_ids.push(s.to_string());
            }
        }
    }
    // The binding experiment run — through the subject's manifest.
    if let Some(r) = entry.subject_runs.first() {
        if let Ok(m) = store.manifest(r) {
            entry.experiment_run_id = m
                .experiment
                .as_ref()
                .and_then(|b| b.experiment_run_id.clone());
        }
    }
    // Validate over the materialized members — `present` members read from
    // the pool; absent bytes stay absent (the check reports, never hides).
    let mut member_bytes: MemberBytes = BTreeMap::new();
    for m in &manifest.members {
        if m.status != MemberStatus::Present {
            continue;
        }
        if let Some(a) = blob_addr(&m.address) {
            if let Ok(b) = store.get_blob(&a) {
                member_bytes.insert(m.address.clone(), b);
            }
        }
    }
    // `validated` = the kernel's admission gate (`check_completeness` —
    // `validate_bundle` S1/S2/S4/S7), the same bar `kernel.bundle`'s
    // boundary applies; S3/S5/S6/S8 are assemble/reproduce-phase checks,
    // not the catalogue's admission test.
    let report = validate::check_completeness(&manifest, &member_bytes);
    if report.complete && report.stages.iter().all(|s| s.ok()) {
        entry.status = BundleStatus::Validated;
    }
    // `level_now` — the basis the current pool still carries.
    entry.level_now = level_now(store, &manifest);
    entry
}

/// `level_now` — the highest `ReproLevel` whose basis still resolves under
/// the *current* pool (§6.5 §2.1's GC/redaction rule: the entry persists but
/// `level_now` drops). A `Member(addr)` requirement resolves when the
/// member still resolves: `fetch` satisfies by declaration, `present` needs
/// the bytes, `redacted`/`gc` fail. `Unpinned` satisfies at R0 only
/// ("satisfies nothing above R0" — manifest.rs); `NotApplicable` never
/// blocks; `Missing` fails.
fn level_now(store: &Store, manifest: &BundleManifest) -> Option<ReproLevel> {
    let (max, basis) = hh_bundle::levels::derive(manifest, false);
    let resolves = |addr: &str| -> bool {
        manifest
            .members
            .iter()
            .find(|m| m.address == addr)
            .map(|m| match m.status {
                MemberStatus::Fetch => true,
                MemberStatus::Present => store.blob_present(addr),
                MemberStatus::Redacted | MemberStatus::Gc => false,
            })
            .unwrap_or(false)
    };
    let mut best: Option<ReproLevel> = None;
    for level in ReproLevel::all() {
        if level > max {
            continue;
        }
        let ok = basis
            .iter()
            .filter(|b| b.level == level)
            .all(|b| match &b.satisfied_by {
                hh_bundle::manifest::BasisSatisfaction::Member(a) => resolves(a),
                hh_bundle::manifest::BasisSatisfaction::Unpinned(_) => level == ReproLevel::R0,
                hh_bundle::manifest::BasisSatisfaction::NotApplicable(_) => true,
                hh_bundle::manifest::BasisSatisfaction::Missing => false,
            });
        if ok {
            best = Some(level);
        }
    }
    best
}
