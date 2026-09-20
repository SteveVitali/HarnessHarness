//! `hh-bundle/1` — the `BundleManifest` record and its member types
//! (spec §5h.3 §3; ADR-0139 D1/D3; ADR-0038's section list). Identity:
//! `version_id` = `idp/1` over `H("bundle" ∥ canonical(manifest −
//! version_id))` — the tree rule, never archive bytes (ADR-0139 D2;
//! AC-R-2.9.3-1).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::error::BundleError;

/// The one schema id (CC7).
pub const BUNDLE_SCHEMA: &str = "hh-bundle/1";
/// The one identity profile (CC1).
pub const BUNDLE_IDP: &str = "idp/1";
/// The identity domain tag for the bundle id.
pub const BUNDLE_DOMAIN: &str = "bundle";
/// The identity domain tag for a `LedgerExport` page tree.
pub const LEDGER_TREE_DOMAIN: &str = "bundle.ledger_tree";

/// Reproducibility level (R-2.12.1; ADR-0038/0140).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReproLevel {
    /// Validation only.
    R0,
    /// Re-derivation (views, rows, compiled bundle, manifest).
    R1,
    /// Paired re-runs under the recorded seed material.
    R2,
    /// R2 over `seeds[]` with model roles unpinned + fingerprints.
    R3,
}

impl ReproLevel {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ReproLevel::R0 => "R0",
            ReproLevel::R1 => "R1",
            ReproLevel::R2 => "R2",
            ReproLevel::R3 => "R3",
        }
    }
    /// Parse; `None` on anything else.
    pub fn parse(s: &str) -> Option<ReproLevel> {
        match s {
            "R0" => Some(ReproLevel::R0),
            "R1" => Some(ReproLevel::R1),
            "R2" => Some(ReproLevel::R2),
            "R3" => Some(ReproLevel::R3),
            _ => None,
        }
    }
    /// Every level, low to high.
    pub fn all() -> [ReproLevel; 4] {
        [
            ReproLevel::R0,
            ReproLevel::R1,
            ReproLevel::R2,
            ReproLevel::R3,
        ]
    }
}

/// `MemberRef.status` (ADR-0139 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatus {
    /// Bytes are in the payload tree.
    Present,
    /// Listed, not materialized — `fetch[]` entry required.
    Fetch,
    /// Withheld under a recorded redaction.
    Redacted,
    /// Collected under a recorded GC.
    Gc,
}

impl MemberStatus {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            MemberStatus::Present => "present",
            MemberStatus::Fetch => "fetch",
            MemberStatus::Redacted => "redacted",
            MemberStatus::Gc => "gc",
        }
    }
    /// Parse; `None` on anything else.
    pub fn parse(s: &str) -> Option<MemberStatus> {
        match s {
            "present" => Some(MemberStatus::Present),
            "fetch" => Some(MemberStatus::Fetch),
            "redacted" => Some(MemberStatus::Redacted),
            "gc" => Some(MemberStatus::Gc),
            _ => None,
        }
    }
}

/// `MemberRef{role, ref, media_type?, size?, status, foreign?}` (R-ID-1:
/// `ref` is a full `idp/1` digest).
#[derive(Debug, Clone, PartialEq)]
pub struct MemberRef {
    /// The section role the member fills (`definition`, `traces`,
    /// `compiled_bundle`, `ledger_page:<run>:<n>`, …).
    pub role: String,
    /// The content address (`sha256:<hex>` under `idp/1`).
    pub address: String,
    /// The media type hint.
    pub media_type: String,
    /// Byte size of the payload.
    pub size: u64,
    /// Presence.
    pub status: MemberStatus,
    /// A foreign-system digest claim (never satisfies > R0).
    pub foreign: Option<Json>,
}

impl MemberRef {
    /// A present member.
    pub fn present(role: impl Into<String>, address: String, media_type: &str, size: u64) -> Self {
        MemberRef {
            role: role.into(),
            address,
            media_type: media_type.to_string(),
            size,
            status: MemberStatus::Present,
            foreign: None,
        }
    }
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("role".into(), Json::str(self.role.clone()));
        m.insert("ref".into(), Json::str(self.address.clone()));
        m.insert("media_type".into(), Json::str(self.media_type.clone()));
        m.insert("size".into(), Json::Int(self.size as i64));
        m.insert("status".into(), Json::str(self.status.name()));
        if let Some(f) = &self.foreign {
            m.insert("foreign".into(), f.clone());
        }
        Json::Obj(m)
    }
    /// Decode; strict on the required members.
    pub fn from_json(j: &Json) -> Result<MemberRef, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("MemberRef: {d}"),
        };
        let role = j.get("role").and_then(Json::as_str).ok_or(bad("role"))?;
        let address = j
            .get("ref")
            .and_then(Json::as_str)
            .ok_or(bad("ref"))?;
        let status = j
            .get("status")
            .and_then(Json::as_str)
            .and_then(MemberStatus::parse)
            .ok_or(bad("status"))?;
        Ok(MemberRef {
            role: role.into(),
            address: address.into(),
            media_type: j
                .get("media_type")
                .and_then(Json::as_str)
                .unwrap_or("")
                .into(),
            size: j.get("size").and_then(Json::as_int).unwrap_or(0) as u64,
            status,
            foreign: j.get("foreign").cloned(),
        })
    }
}

/// `Claim{role, value, provenance}` — an assertion carried, never a pin
/// (`authority ≤ external` / `unverified` for lifted content).
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    /// What is claimed.
    pub role: String,
    /// The claimed value.
    pub value: Json,
    /// The claim's provenance record.
    pub provenance: Json,
}

impl Claim {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("role", Json::str(self.role.clone())),
            ("value", self.value.clone()),
            ("provenance", self.provenance.clone()),
        ])
    }
    /// Decode.
    pub fn from_json(j: &Json) -> Result<Claim, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("Claim: {d}"),
        };
        Ok(Claim {
            role: j.get("role").and_then(Json::as_str).ok_or(bad("role"))?.into(),
            value: j.get("value").cloned().unwrap_or(Json::Null),
            provenance: j
                .get("provenance")
                .cloned()
                .ok_or(bad("provenance"))?,
        })
    }
}

/// `Unpinned{role, reason, claim?}` — the declared holes (ADR-0139 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct Unpinned {
    /// The member role left unpinned.
    pub role: String,
    /// `provider_opaque | not_captured | hosted_mechanism | redacted | gc |
    /// foreign_only`.
    pub reason: String,
    /// The claim that stands in for the pin, when one exists.
    pub claim: Option<Json>,
}

impl Unpinned {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("role".into(), Json::str(self.role.clone()));
        m.insert("reason".into(), Json::str(self.reason.clone()));
        if let Some(c) = &self.claim {
            m.insert("claim".into(), c.clone());
        }
        Json::Obj(m)
    }
    /// Decode.
    pub fn from_json(j: &Json) -> Result<Unpinned, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("Unpinned: {d}"),
        };
        Ok(Unpinned {
            role: j.get("role").and_then(Json::as_str).ok_or(bad("role"))?.into(),
            reason: j
                .get("reason")
                .and_then(Json::as_str)
                .ok_or(bad("reason"))?
                .into(),
            claim: j.get("claim").cloned(),
        })
    }
}

/// `FetchEntry{address, size?, locations[], expires?}` — every locator
/// credential-free (R-ID-7).
#[derive(Debug, Clone, PartialEq)]
pub struct FetchEntry {
    /// The member's address.
    pub address: String,
    /// Declared size when known.
    pub size: Option<u64>,
    /// Credential-free URIs.
    pub locations: Vec<String>,
    /// Optional expiry (RFC 3339).
    pub expires: Option<String>,
}

impl FetchEntry {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("address".into(), Json::str(self.address.clone()));
        if let Some(s) = self.size {
            m.insert("size".into(), Json::Int(s as i64));
        }
        m.insert(
            "locations".into(),
            Json::Arr(self.locations.iter().map(|l| Json::str(l.clone())).collect()),
        );
        if let Some(e) = &self.expires {
            m.insert("expires".into(), Json::str(e.clone()));
        }
        Json::Obj(m)
    }
    /// Decode.
    pub fn from_json(j: &Json) -> Result<FetchEntry, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("FetchEntry: {d}"),
        };
        let locations = match j.get("locations") {
            Some(Json::Arr(ls)) => ls.iter().filter_map(|l| l.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        };
        Ok(FetchEntry {
            address: j
                .get("address")
                .and_then(Json::as_str)
                .ok_or(bad("address"))?
                .into(),
            size: j.get("size").and_then(Json::as_int).map(|i| i as u64),
            locations,
            expires: j.get("expires").and_then(Json::as_str).map(String::from),
        })
    }
}

/// A `LedgerExport` blob-index row.
#[derive(Debug, Clone, PartialEq)]
pub struct BlobIndexEntry {
    /// The blob's content address.
    pub address: String,
    /// `present | redacted | gc`.
    pub status: String,
    /// Byte size when present.
    pub size: u64,
}

impl BlobIndexEntry {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("address", Json::str(self.address.clone())),
            ("status", Json::str(self.status.clone())),
            ("size", Json::Int(self.size as i64)),
        ])
    }
}

/// `LedgerExport` (per subject run; layer P): chunked canonical event
/// pages by seq range (a tree), checkpoints, blob index, head and
/// lineage prefixes (§5h.3 §3; CF-299).
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerExport {
    /// The subject run.
    pub run_id: String,
    /// Events per page.
    pub page_size: u64,
    /// Page member addresses in seq order.
    pub pages: Vec<String>,
    /// The tree address — `idp/1` over the page-address list.
    pub tree: String,
    /// Signed tree heads as `EventRef`s.
    pub checkpoints: Vec<Json>,
    /// The offloaded-blob index.
    pub blob_index: Vec<BlobIndexEntry>,
    /// `{seq, event_id, hash}` at export time.
    pub head: Json,
    /// `[{run_id, up_to_seq, head_hash}]`.
    pub lineage_prefixes: Vec<Json>,
}

impl LedgerExport {
    /// The tree address over an ordered page-address list (the one tree
    /// rule — R-ID-3).
    pub fn tree_address(pages: &[String]) -> String {
        let preimage = Json::Arr(pages.iter().map(|p| Json::str(p.clone())).collect());
        hh_identity::idp_id(
            LEDGER_TREE_DOMAIN,
            preimage.to_canonical_string().as_bytes(),
        )
    }
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("run_id".into(), Json::str(self.run_id.clone()));
        m.insert("page_size".into(), Json::Int(self.page_size as i64));
        m.insert(
            "pages".into(),
            Json::Arr(self.pages.iter().map(|p| Json::str(p.clone())).collect()),
        );
        m.insert("tree".into(), Json::str(self.tree.clone()));
        m.insert(
            "checkpoints".into(),
            Json::Arr(self.checkpoints.clone()),
        );
        m.insert(
            "blob_index".into(),
            Json::Arr(self.blob_index.iter().map(|b| b.to_json()).collect()),
        );
        m.insert("head".into(), self.head.clone());
        m.insert(
            "lineage_prefixes".into(),
            Json::Arr(self.lineage_prefixes.clone()),
        );
        Json::Obj(m)
    }
    /// Decode (structural — validation re-checks the tree).
    pub fn from_json(j: &Json) -> Result<LedgerExport, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("LedgerExport: {d}"),
        };
        let pages = match j.get("pages") {
            Some(Json::Arr(ps)) => ps.iter().filter_map(|p| p.as_str().map(String::from)).collect(),
            _ => return Err(bad("pages")),
        };
        let blob_index = match j.get("blob_index") {
            Some(Json::Arr(bs)) => bs
                .iter()
                .map(|b| BlobIndexEntry {
                    address: b
                        .get("address")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .into(),
                    status: b
                        .get("status")
                        .and_then(Json::as_str)
                        .unwrap_or("present")
                        .into(),
                    size: b.get("size").and_then(Json::as_int).unwrap_or(0) as u64,
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(LedgerExport {
            run_id: j
                .get("run_id")
                .and_then(Json::as_str)
                .ok_or(bad("run_id"))?
                .into(),
            page_size: j.get("page_size").and_then(Json::as_int).unwrap_or(0) as u64,
            pages,
            tree: j.get("tree").and_then(Json::as_str).unwrap_or("").into(),
            checkpoints: match j.get("checkpoints") {
                Some(Json::Arr(cs)) => cs.clone(),
                _ => Vec::new(),
            },
            blob_index,
            head: j.get("head").cloned().unwrap_or(Json::Null),
            lineage_prefixes: match j.get("lineage_prefixes") {
                Some(Json::Arr(ls)) => ls.clone(),
                _ => Vec::new(),
            },
        })
    }
}

/// `LevelBasis{level, requirement_id, satisfied_by}` (ADR-0140 D2).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelBasis {
    /// The level the requirement gates.
    pub level: ReproLevel,
    /// `B-R0-*` … `B-R3-*` — the closed requirement set.
    pub requirement_id: String,
    /// What satisfies it: a member address, `unpinned{role}`, or
    /// `n/a{class}` — claims never satisfy above R0 (R-ID-2).
    pub satisfied_by: BasisSatisfaction,
}

/// What a basis requirement resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum BasisSatisfaction {
    /// A member carries it.
    Member(String),
    /// Declared unpinned (satisfies nothing above R0).
    Unpinned(String),
    /// Class-inapplicable — neither blocks nor counts.
    NotApplicable(String),
    /// Unsatisfied.
    Missing,
}

impl LevelBasis {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let sat = match &self.satisfied_by {
            BasisSatisfaction::Member(a) => Json::obj([("member", Json::str(a.clone()))]),
            BasisSatisfaction::Unpinned(r) => Json::obj([("unpinned", Json::str(r.clone()))]),
            BasisSatisfaction::NotApplicable(c) => {
                Json::obj([("n/a", Json::str(c.clone()))])
            }
            BasisSatisfaction::Missing => Json::obj([("missing", Json::Bool(true))]),
        };
        Json::obj([
            ("level", Json::str(self.level.name())),
            ("requirement_id", Json::str(self.requirement_id.clone())),
            ("satisfied_by", sat),
        ])
    }
}

/// The subject section (kind=run at this stage).
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectSection {
    /// The subject run ids (one at `kind = run`).
    pub run_ids: Vec<String>,
    /// `{seq, event_id, hash}` per run — keyed by run id.
    pub heads: BTreeMap<String, Json>,
    /// The run's lineage links (fork/continuation) verbatim.
    pub lineage: Vec<Json>,
    /// `{run_id: head_seq}` — the watermark the bundle was assembled at.
    pub watermarks: BTreeMap<String, u64>,
    /// `open | finished` — an open run caps at R0 (`RunNotDurable` on
    /// higher claims).
    pub status: String,
}

impl SubjectSection {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "run_ids".into(),
            Json::Arr(self.run_ids.iter().map(|r| Json::str(r.clone())).collect()),
        );
        m.insert(
            "heads".into(),
            Json::Obj(
                self.heads
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        m.insert("lineage".into(), Json::Arr(self.lineage.clone()));
        m.insert(
            "watermarks".into(),
            Json::Obj(
                self.watermarks
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        m.insert("status".into(), Json::str(self.status.clone()));
        Json::Obj(m)
    }
    /// Decode.
    pub fn from_json(j: &Json) -> SubjectSection {
        let heads = match j.get("heads") {
            Some(Json::Obj(h)) => h.clone(),
            _ => BTreeMap::new(),
        };
        let watermarks = match j.get("watermarks") {
            Some(Json::Obj(w)) => w
                .iter()
                .filter_map(|(k, v)| v.as_int().map(|i| (k.clone(), i as u64)))
                .collect(),
            _ => BTreeMap::new(),
        };
        SubjectSection {
            run_ids: match j.get("run_ids") {
                Some(Json::Arr(rs)) => rs.iter().filter_map(|r| r.as_str().map(String::from)).collect(),
                _ => Vec::new(),
            },
            heads,
            lineage: match j.get("lineage") {
                Some(Json::Arr(ls)) => ls.clone(),
                _ => Vec::new(),
            },
            watermarks,
            status: j
                .get("status")
                .and_then(Json::as_str)
                .unwrap_or("finished")
                .into(),
        }
    }
}

/// `BundlePolicy` — `bundle()`'s input record (§5h.3 §2).
#[derive(Debug, Clone, PartialEq)]
pub struct BundlePolicy {
    /// `self_contained | detached | manifest_only`.
    pub materialize: String,
    /// The readers the bundle is assembled for (S8 reader checks are
    /// Stage-4; recorded verbatim now).
    pub readers: Vec<String>,
    /// A declared redaction policy ref.
    pub redaction: Option<String>,
    /// Carry the interchange export as a member (Stage-4 — recorded).
    pub include_interchange_export: bool,
    /// `claims[]` the producer asserts.
    pub claims: Vec<Claim>,
    /// The level the producer claims (validated against the derived max).
    pub claimed_level: Option<ReproLevel>,
    /// The export sink this bundle is delivered to — when set the caller
    /// appends `measurement.export.delivered` to each subject run.
    pub deliver_sink: Option<String>,
}

impl Default for BundlePolicy {
    fn default() -> Self {
        BundlePolicy {
            materialize: "self_contained".to_string(),
            readers: Vec::new(),
            redaction: None,
            include_interchange_export: false,
            claims: Vec::new(),
            claimed_level: None,
            deliver_sink: None,
        }
    }
}

impl BundlePolicy {
    /// Decode from boundary params (`policy` member of `kernel.bundle`).
    pub fn from_json(j: Option<&Json>) -> Result<BundlePolicy, BundleError> {
        let mut p = BundlePolicy::default();
        let Some(j) = j else { return Ok(p) };
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("BundlePolicy: {d}"),
        };
        if let Some(m) = j.get("materialize").and_then(Json::as_str) {
            match m {
                "self_contained" | "detached" | "manifest_only" => {
                    p.materialize = m.to_string()
                }
                other => return Err(bad(&format!("materialize {other}"))),
            }
        }
        if let Some(Json::Arr(rs)) = j.get("readers") {
            p.readers = rs.iter().filter_map(|r| r.as_str().map(String::from)).collect();
        }
        p.redaction = j.get("redaction").and_then(Json::as_str).map(String::from);
        p.include_interchange_export = matches!(
            j.get("include_interchange_export"),
            Some(Json::Bool(true))
        );
        if let Some(Json::Arr(cs)) = j.get("claims") {
            p.claims = cs
                .iter()
                .map(Claim::from_json)
                .collect::<Result<Vec<_>, _>>()?;
        }
        if let Some(l) = j.get("claimed_level").and_then(Json::as_str) {
            p.claimed_level = Some(ReproLevel::parse(l).ok_or(bad("claimed_level"))?);
        }
        p.deliver_sink = j
            .get("deliver_sink")
            .and_then(Json::as_str)
            .map(String::from);
        Ok(p)
    }
}

/// `BundleManifest` — the layer-M record (§5h.3 §3 row 1). Sections are
/// canonical `Json` produced by the assembler; the header, subject,
/// reproducibility and member list are typed (validation computes over
/// them).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleManifest {
    /// `run` at this stage (arm/experiment/lineage are Stage 4).
    pub bundle_kind: String,
    /// RFC 3339 ms.
    pub created_at: String,
    /// The producer's `ProvenanceRecord` (native: `kernel(component)`;
    /// imported: `import`).
    pub producer: Json,
    /// `native | hosted`.
    pub participant_class: String,
    /// The declared observability subset.
    pub observability_levels: Vec<String>,
    /// Producer claims.
    pub claims: Vec<Claim>,
    /// Display-name bindings (never identity — R-ID-4).
    pub name_bindings: Vec<Json>,
    /// `self_contained | detached_allowed`.
    pub fetch_policy: String,
    /// Unmaterialized members.
    pub fetch: Vec<FetchEntry>,
    /// ADR-0038's `subject`.
    pub subject: SubjectSection,
    /// `definition` section (canonical record Json).
    pub definition: Json,
    /// `configuration` section.
    pub configuration: Json,
    /// `resolved_dependencies` section.
    pub resolved_dependencies: Json,
    /// `model` section (`{snapshots: ModelSnapshotRecord[]}`).
    pub model: Json,
    /// `instrument` section (`InstrumentRecord` incl. `component_versions`
    /// carrying `ContractIdentity`).
    pub instrument: Json,
    /// `traces` — one `LedgerExport` per subject run.
    pub traces: BTreeMap<String, LedgerExport>,
    /// `results` section (`rows[]` empty at this stage).
    pub results: Json,
    /// `reproducibility` section.
    pub reproducibility: Json,
    /// The flat member index (S3/S4 fold over it).
    pub members: Vec<MemberRef>,
    /// Declared-unpinned roles.
    pub unpinned: Vec<Unpinned>,
    /// Registered extensions (preserved, never deciding validity).
    pub ext: BTreeMap<String, Json>,
    /// The bundle id — `idp/1` over `canonical(manifest − version_id)`.
    pub version_id: String,
}

impl BundleManifest {
    /// The canonical document *without* `version_id` — the identity
    /// preimage.
    fn preimage(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(BUNDLE_SCHEMA));
        m.insert("idp".into(), Json::str(BUNDLE_IDP));
        m.insert("bundle_kind".into(), Json::str(self.bundle_kind.clone()));
        m.insert("created_at".into(), Json::str(self.created_at.clone()));
        m.insert("producer".into(), self.producer.clone());
        m.insert(
            "participant_class".into(),
            Json::str(self.participant_class.clone()),
        );
        m.insert(
            "observability_levels".into(),
            Json::Arr(
                self.observability_levels
                    .iter()
                    .map(|l| Json::str(l.clone()))
                    .collect(),
            ),
        );
        m.insert(
            "claims".into(),
            Json::Arr(self.claims.iter().map(|c| c.to_json()).collect()),
        );
        m.insert(
            "name_bindings".into(),
            Json::Arr(self.name_bindings.clone()),
        );
        m.insert(
            "fetch_policy".into(),
            Json::str(self.fetch_policy.clone()),
        );
        m.insert(
            "fetch".into(),
            Json::Arr(self.fetch.iter().map(|f| f.to_json()).collect()),
        );
        m.insert("subject".into(), self.subject.to_json());
        m.insert("definition".into(), self.definition.clone());
        m.insert("configuration".into(), self.configuration.clone());
        m.insert(
            "resolved_dependencies".into(),
            self.resolved_dependencies.clone(),
        );
        m.insert("model".into(), self.model.clone());
        m.insert("instrument".into(), self.instrument.clone());
        m.insert(
            "traces".into(),
            Json::Obj(
                self.traces
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        );
        m.insert("results".into(), self.results.clone());
        m.insert("reproducibility".into(), self.reproducibility.clone());
        m.insert(
            "members".into(),
            Json::Arr(self.members.iter().map(|r| r.to_json()).collect()),
        );
        m.insert(
            "unpinned".into(),
            Json::Arr(self.unpinned.iter().map(|u| u.to_json()).collect()),
        );
        m.insert(
            "ext".into(),
            Json::Obj(self.ext.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        );
        Json::Obj(m)
    }

    /// `H("bundle" ∥ canonical(manifest − version_id))` — the bundle id
    /// (the tree rule; archive bytes never enter).
    pub fn compute_id(&self) -> String {
        hh_identity::idp_id(
            BUNDLE_DOMAIN,
            self.preimage().to_canonical_string().as_bytes(),
        )
    }

    /// Canonical JSON (with `version_id`).
    pub fn to_json(&self) -> Json {
        let mut m = match self.preimage() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("version_id".into(), Json::str(self.version_id.clone()));
        Json::Obj(m)
    }

    /// Decode a manifest document (structural; `validate_bundle` owns the
    /// staged checks).
    pub fn from_json(j: &Json) -> Result<BundleManifest, BundleError> {
        let bad = |d: &str| BundleError::UnknownBundleSchema {
            detail: format!("BundleManifest: {d}"),
        };
        if j.get("schema").and_then(Json::as_str) != Some(BUNDLE_SCHEMA) {
            return Err(bad("schema is not hh-bundle/1"));
        }
        if j.get("idp").and_then(Json::as_str) != Some(BUNDLE_IDP) {
            return Err(bad("idp is not idp/1"));
        }
        let members = match j.get("members") {
            Some(Json::Arr(ms)) => ms
                .iter()
                .map(MemberRef::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let fetch = match j.get("fetch") {
            Some(Json::Arr(fs)) => fs
                .iter()
                .map(FetchEntry::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let claims = match j.get("claims") {
            Some(Json::Arr(cs)) => cs
                .iter()
                .map(Claim::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let unpinned = match j.get("unpinned") {
            Some(Json::Arr(us)) => us
                .iter()
                .map(Unpinned::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let traces = match j.get("traces") {
            Some(Json::Obj(t)) => t
                .iter()
                .map(|(k, v)| LedgerExport::from_json(v).map(|e| (k.clone(), e)))
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            _ => BTreeMap::new(),
        };
        let observability_levels = match j.get("observability_levels") {
            Some(Json::Arr(ls)) => ls.iter().filter_map(|l| l.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        };
        Ok(BundleManifest {
            bundle_kind: j
                .get("bundle_kind")
                .and_then(Json::as_str)
                .ok_or(bad("bundle_kind"))?
                .into(),
            created_at: j
                .get("created_at")
                .and_then(Json::as_str)
                .unwrap_or("")
                .into(),
            producer: j.get("producer").cloned().ok_or(bad("producer"))?,
            participant_class: j
                .get("participant_class")
                .and_then(Json::as_str)
                .unwrap_or("native")
                .into(),
            observability_levels,
            claims,
            name_bindings: match j.get("name_bindings") {
                Some(Json::Arr(ns)) => ns.clone(),
                _ => Vec::new(),
            },
            fetch_policy: j
                .get("fetch_policy")
                .and_then(Json::as_str)
                .unwrap_or("self_contained")
                .into(),
            fetch,
            subject: SubjectSection::from_json(j.get("subject").unwrap_or(&Json::Null)),
            definition: j.get("definition").cloned().unwrap_or(Json::Null),
            configuration: j.get("configuration").cloned().unwrap_or(Json::Null),
            resolved_dependencies: j
                .get("resolved_dependencies")
                .cloned()
                .unwrap_or(Json::Null),
            model: j.get("model").cloned().unwrap_or(Json::Null),
            instrument: j.get("instrument").cloned().unwrap_or(Json::Null),
            traces,
            results: j.get("results").cloned().unwrap_or(Json::Null),
            reproducibility: j
                .get("reproducibility")
                .cloned()
                .unwrap_or(Json::Null),
            members,
            unpinned,
            ext: match j.get("ext") {
                Some(Json::Obj(e)) => e.clone(),
                _ => BTreeMap::new(),
            },
            version_id: j
                .get("version_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_order_and_parse() {
        assert!(ReproLevel::R0 < ReproLevel::R3);
        assert_eq!(ReproLevel::parse("R2"), Some(ReproLevel::R2));
        assert_eq!(ReproLevel::parse("R4"), None);
    }

    #[test]
    fn member_ref_round_trip() {
        let r = MemberRef::present("definition", "sha256:ab".to_string(), "application/json", 7);
        let back = MemberRef::from_json(&r.to_json()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn manifest_id_is_tree_rule() {
        let m = BundleManifest {
            bundle_kind: "run".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            producer: Json::obj([("origin", Json::str("kernel"))]),
            participant_class: "native".into(),
            observability_levels: vec!["ledger".into()],
            claims: vec![],
            name_bindings: vec![],
            fetch_policy: "self_contained".into(),
            fetch: vec![],
            subject: SubjectSection {
                run_ids: vec!["run-1".into()],
                heads: BTreeMap::new(),
                lineage: vec![],
                watermarks: BTreeMap::new(),
                status: "finished".into(),
            },
            definition: Json::Null,
            configuration: Json::Null,
            resolved_dependencies: Json::Null,
            model: Json::Null,
            instrument: Json::Null,
            traces: BTreeMap::new(),
            results: Json::Null,
            reproducibility: Json::Null,
            members: vec![],
            unpinned: vec![],
            ext: BTreeMap::new(),
            version_id: String::new(),
        };
        let id = m.compute_id();
        assert!(id.starts_with("sha256:"));
        // The computed id does not perturb itself: stamping it and
        // recomputing over the same preimage is stable.
        let mut stamped = m.clone();
        stamped.version_id = id.clone();
        assert_eq!(stamped.compute_id(), id);
        // to_json/from_json round-trips the stamped manifest.
        let back = BundleManifest::from_json(&stamped.to_json()).unwrap();
        assert_eq!(back.compute_id(), id);
    }
}
