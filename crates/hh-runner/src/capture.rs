//! §5.6 trajectory and integrity capture (R6.25 stage 1; REQ-XC-317,
//! REQ-XC-318 superseding REQ-XB-013, REQ-XC-328, REQ-XC-207;
//! design/34 §1.7 + F12 + F18).
//!
//! Proxy receipts stay the transcript of record (X-04): `/logs/agent`
//! is copied out by a kernel-mediated container-path download *after
//! agent end and before verification*, hashed into the slot record's
//! `trajectory.native_log`; ATIF is produced post hoc by Harbor's
//! converter at the pinned commit and never gates (its absence is a
//! reported attribute — the publication check requires the
//! receipt-index transcript, C-16); harness native logs reconcile
//! against receipts — an unreceipted call is
//! `inconclusive{infra:unreceipted_call}`, never scored (REQ-XC-207);
//! ST4 re-runs the version probe and re-hashes the install tree — any
//! change flags `harness_self_modified` (REQ-XC-328).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::cell::{CellError, CellOps};
use crate::disposition::{Disposition, InconclusiveCause, InfraCause, SlotFlag};
use crate::json_ext::JsonExt;
use crate::slot_record::ExperimentSlot;

/// The agent log root inside the cell (REQ-XC-317's copy-out source).
pub const LOGS_AGENT: &str = "\u{2f}logs\u{2f}agent";
/// The shared work mount — copy-out never stages through it (the
/// kernel-mediated container path is the only channel, REQ-XB-106).
pub const WORK_MOUNT: &str = "\u{2f}work";
/// The integrity-bundle schema spelling.
pub const BUNDLE_SCHEMA: &str = "hh-integrity-bundle/1";

/// A typed capture refusal — every leg names its rule (honest refusal).
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureError {
    /// The refusal rule.
    pub rule: &'static str,
    /// The verbatim detail.
    pub detail: String,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "capture:{}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for CaptureError {}

fn err(rule: &'static str, detail: impl Into<String>) -> CaptureError {
    CaptureError {
        rule,
        detail: detail.into(),
    }
}

// ── /logs/agent copy-out (REQ-XC-317) ────────────────────────────────────────

/// The validated ST4 copy-out leg — the container source and the host
/// destination, with the `/work` alias refused at construction.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyoutSpec {
    /// The container-side source (must be `/logs/agent` — kernel
    /// mediation names a container path, never a `/work` alias).
    pub container_src: String,
    /// The host destination tree (kernel-mediated `podman cp` target).
    pub host_dst: PathBuf,
    /// The slot's shared `/work` mount on the host — a destination under
    /// it would be the cell writing its own staging area, the same
    /// aliasing hole `/work` copy-in forbids.
    pub work_mount: PathBuf,
}

/// `copyout_spec(src, dst, work)` — the `/work`-alias refusal: the
/// container source must be `/logs/agent` (never `/work` or a `/work`
/// child) and the host destination must not sit inside the slot's work
/// mount. A typed `work_alias` refusal, never a relabelled path.
pub fn copyout_spec(
    container_src: &str,
    host_dst: &Path,
    work_mount: &Path,
) -> Result<CopyoutSpec, CaptureError> {
    if container_src == WORK_MOUNT || container_src.starts_with(&format!("{WORK_MOUNT}\u{2f}")) {
        return Err(err(
            "work_alias",
            format!("container source {container_src} aliases {WORK_MOUNT} — copy-out is kernel-mediated, never through the shared mount"),
        ));
    }
    if container_src != LOGS_AGENT {
        return Err(err(
            "copyout_source",
            format!("the ST4 copy-out source is {LOGS_AGENT} verbatim, found {container_src}"),
        ));
    }
    if host_dst.starts_with(work_mount) {
        return Err(err(
            "work_alias",
            format!(
                "host destination {} sits inside the slot's work mount — copy-out never stages through {WORK_MOUNT}",
                host_dst.display()
            ),
        ));
    }
    Ok(CopyoutSpec {
        container_src: container_src.to_string(),
        host_dst: host_dst.to_path_buf(),
        work_mount: work_mount.to_path_buf(),
    })
}

/// One hashed member of a copied-out tree.
#[derive(Debug, Clone, PartialEq)]
pub struct HashedFile {
    /// `sha256:<hex>` of the file bytes.
    pub sha256: String,
    /// Byte length.
    pub bytes: u64,
}

impl HashedFile {
    /// Hash `bytes` into the member.
    pub fn of(bytes: &[u8]) -> HashedFile {
        HashedFile {
            sha256: format!("sha256:{}", sha256_hex(bytes)),
            bytes: bytes.len() as u64,
        }
    }

    /// The member JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("sha256", Json::str(&self.sha256)),
            ("bytes", Json::Int(self.bytes as i64)),
        ])
    }
}

/// The copied-out `/logs/agent` tree, hashed (the slot record's
/// `trajectory.native_log` member — REQ-XC-317's "hashed into the slot
/// record").
#[derive(Debug, Clone, PartialEq)]
pub struct NativeLogBundle {
    /// Relative path → digest.
    pub files: BTreeMap<String, HashedFile>,
    /// The manifest digest — `sha256` over the sorted
    /// `"{sha256}  {path}\n"` listing (the ST2 composite recipe).
    pub manifest_sha256: String,
}

impl NativeLogBundle {
    /// The `trajectory.native_log` member JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "files",
                Json::Obj(
                    self.files
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_json()))
                        .collect(),
                ),
            ),
            ("manifest_sha256", Json::str(&self.manifest_sha256)),
        ])
    }

    /// `verify(bundle)` — every recorded digest recomputes against the
    /// member bytes and the manifest listing recomputes (AC-1's "the
    /// slot-record digests recompute").
    pub fn verify(&self, bytes_of: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<(), CaptureError> {
        let mut listing = String::new();
        for (path, h) in &self.files {
            let b = bytes_of(path)
                .ok_or_else(|| err("verify", format!("{path} absent from the copy-out")))?;
            if HashedFile::of(&b) != *h {
                return Err(err("verify", format!("{path} digest drifted")));
            }
            listing.push_str(&format!("{}  {}\n", h.sha256, path));
        }
        if format!("sha256:{}", sha256_hex(listing.as_bytes())) != self.manifest_sha256 {
            return Err(err("verify", "manifest_sha256 does not recompute"));
        }
        Ok(())
    }
}

/// `hash_files(files)` — fold a `{rel-path → bytes}` map into a
/// [`NativeLogBundle`] (the sorted-listing composite).
pub fn hash_files(files: BTreeMap<String, Vec<u8>>) -> NativeLogBundle {
    let mut out = BTreeMap::new();
    let mut listing = String::new();
    for (path, b) in &files {
        let h = HashedFile::of(b);
        listing.push_str(&format!("{}  {}\n", h.sha256, path));
        out.insert(path.clone(), h);
    }
    NativeLogBundle {
        files: out,
        manifest_sha256: format!("sha256:{}", sha256_hex(listing.as_bytes())),
    }
}

/// `hash_host_tree(root)` — walk the host-side copy-out destination:
/// a single file or a directory tree (sorted relative paths). Reads
/// the copied bytes only — never an existence-resume (REQ-XB-105).
pub fn hash_host_tree(root: &Path) -> Result<NativeLogBundle, CellError> {
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<(), CellError> {
        for e in std::fs::read_dir(dir).map_err(|e| CellError {
            op: "cp_out",
            detail: format!("walk {}: {e}", dir.display()),
        })? {
            let p = e.map_err(|e| CellError {
                op: "cp_out",
                detail: e.to_string(),
            })?;
            let ft = p.file_type().map_err(|e| CellError {
                op: "cp_out",
                detail: e.to_string(),
            })?;
            if ft.is_dir() {
                walk(&p.path(), acc)?;
            } else {
                acc.push(p.path());
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    match std::fs::read_dir(root) {
        Ok(_) => {
            let mut paths = Vec::new();
            walk(root, &mut paths)?;
            paths.sort();
            for p in paths {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .to_string();
                let b = std::fs::read(&p).map_err(|e| CellError {
                    op: "cp_out",
                    detail: format!("read {}: {e}", p.display()),
                })?;
                files.insert(rel, b);
            }
        }
        Err(_) => {
            let b = std::fs::read(root).map_err(|e| CellError {
                op: "cp_out",
                detail: format!("read {}: {e}", root.display()),
            })?;
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "copyout".to_string());
            files.insert(name, b);
        }
    }
    Ok(hash_files(files))
}

/// `copy_out_logs(cell, spec)` — the ST4 leg: one kernel-mediated
/// container-path download, then hash what landed. Called after agent
/// end and before verification (the `run_cell` ordering leg).
pub fn copy_out_logs(
    cell: &mut dyn CellOps,
    spec: &CopyoutSpec,
) -> Result<NativeLogBundle, CellError> {
    cell.cp_out(&spec.container_src, &spec.host_dst)?;
    hash_host_tree(&spec.host_dst)
}

// ── post-hoc ATIF conversion (REQ-XC-318; never a gate) ──────────────────────

/// A converted ATIF document — bytes + its declared schema version.
#[derive(Debug, Clone, PartialEq)]
pub struct AtifDoc {
    /// The ATIF document bytes.
    pub bytes: Vec<u8>,
    /// The declared schema version (`atif_version`/`version`/`schema`).
    pub schema_version: Option<String>,
}

/// The converter seam — production is the Harbor host venv at the pin
/// (`uv run`, R6.17's `hh-harbor-venv/1`); fixtures inject a canned
/// conversion. Conversion is post hoc and never a slot gate.
pub trait AtifConverter {
    /// Convert the collected native logs (`{rel-path → bytes}`) into
    /// ATIF at the pinned commit. `Err` = the converter failed; the
    /// slot still scores with the absence reported.
    fn convert(&self, native_log: &BTreeMap<String, Vec<u8>>) -> Result<AtifDoc, String>;
}

/// The `artifacts.atif.converter_at` pin check (REQ-XC-318 — "Harbor's
/// converter at the pinned commit"): a conversion sourced anywhere
/// else refuses; `artifacts.atif: "none"` means the harness produces
/// no ATIF — a reported attribute, never a failure.
pub fn atif_pin_check(rec: &Json) -> Result<bool, CaptureError> {
    match rec.get("artifacts").and_then(|a| a.get("atif")) {
        Some(Json::Str(s)) if s == "none" => Ok(false),
        Some(v) => {
            let at = v
                .get("converter_at")
                .and_then(Json::as_str)
                .ok_or_else(|| err("atif", "artifacts.atif.converter_at required"))?;
            if at != crate::harbor::HARBOR_PIN {
                return Err(err(
                    "atif",
                    format!("converter_at {at} is not the pinned Harbor commit"),
                ));
            }
            Ok(true)
        }
        None => Err(err("atif", "artifacts.atif required")),
    }
}

/// `convert_atif(rec, converter, native_log)` — the post-hoc leg:
/// `None` where the harness declares no ATIF *or* the converter
/// failed/absent (the reported attribute); `Some` carries the doc and
/// its schema version verbatim.
pub fn convert_atif(
    rec: &Json,
    converter: Option<&dyn AtifConverter>,
    native_log: &BTreeMap<String, Vec<u8>>,
) -> Result<Option<AtifDoc>, CaptureError> {
    if !atif_pin_check(rec)? {
        return Ok(None);
    }
    let Some(c) = converter else {
        return Ok(None);
    };
    match c.convert(native_log) {
        Ok(doc) => Ok(Some(doc)),
        // A converter failure is the reported absence — never a slot
        // failure (HF's export bug is why ATIF never gates, §5.6).
        Err(_) => Ok(None),
    }
}

/// `trajectory_members(native_log, atif, transcript_ref)` — the slot
/// record's `trajectory` section: the hashed copy-out, the ATIF ref +
/// schema (or `atif_absent`, the reported attribute) and the
/// receipt-index transcript ref (the publication-check input).
pub fn trajectory_members(
    native_log: Option<&NativeLogBundle>,
    atif: Option<(&str, Option<&str>)>,
    transcript_ref: Option<&str>,
) -> BTreeMap<String, Json> {
    let mut m = BTreeMap::new();
    if let Some(b) = native_log {
        m.insert("native_log".into(), b.to_json());
    }
    match atif {
        Some((ref_, schema)) => {
            m.insert("atif_ref".into(), Json::str(ref_));
            if let Some(v) = schema {
                m.insert("atif_schema".into(), Json::str(v));
            }
        }
        None => {
            m.insert("atif_absent".into(), Json::Bool(true));
        }
    }
    if let Some(t) = transcript_ref {
        m.insert("proxy_transcript_ref".into(), Json::str(t));
    }
    m
}

/// `publication_check_input(slot)` — REQ-XC-318's C-16 rule: the
/// publication check's per-trial input is the receipt-index transcript
/// (REQ-XF-124), never ATIF. Every *scored* trial must carry
/// `trajectory.proxy_transcript_ref`; an ATIF ref alone never
/// discharges it.
pub fn publication_check_input(slot: &ExperimentSlot) -> Result<String, CaptureError> {
    let transcript = slot
        .trajectory
        .get("proxy_transcript_ref")
        .and_then(Json::as_str);
    match (&slot.disposition, transcript) {
        (Disposition::Executed { .. }, Some(t)) => Ok(t.to_string()),
        (Disposition::Executed { .. }, None) => Err(err(
            "publication",
            "a scored trial lacks trajectory.proxy_transcript_ref — the receipt-index transcript is the publication input (C-16)",
        )),
        (_, Some(t)) => Ok(t.to_string()),
        (_, None) => Ok(String::new()),
    }
}

// ── receipt reconciliation (REQ-XC-207) ──────────────────────────────────────

/// The model-call kind a harness log can show — subagent, background,
/// title and compaction calls count exactly like the main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallKind {
    /// The main agent loop.
    Main,
    /// A subagent call.
    Subagent,
    /// A background task call.
    Background,
    /// A title-generation call.
    Title,
    /// A compaction call.
    Compaction,
}

impl CallKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CallKind::Main => "main",
            CallKind::Subagent => "subagent",
            CallKind::Background => "background",
            CallKind::Title => "title",
            CallKind::Compaction => "compaction",
        }
    }
}

/// One model call a harness's native log shows (the reconciliation's
/// per-harness extractor output).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeCall {
    /// The call's position in the harness's own sequence.
    pub seq: u64,
    /// Its kind.
    pub kind: CallKind,
}

/// `reconcile_receipts(native, receipts)` — REQ-XC-207: every call the
/// native log shows must have a proxy receipt (`proxy.slot_call_seq`,
/// the §4.3 member on the receipt's `proxy` view — the same shape
/// `sm_pos_assert` reads). Returns the unreceipted calls; a returned
/// non-empty list maps to `inconclusive{infra:unreceipted_call}` —
/// the trial is never scored. A receipt with no native line is the
/// documented attribution-loss side-channel, not a reconciliation
/// failure (receipts stay complete — C4 F9).
pub fn reconcile_receipts(native: &[NativeCall], receipts: &[Json]) -> Vec<NativeCall> {
    let mut seqs = std::collections::BTreeSet::new();
    for r in receipts {
        let seq = r
            .get("slot_call_seq")
            .or_else(|| r.get("proxy").and_then(|p| p.get("slot_call_seq")))
            .and_then(Json::as_int);
        if let Some(s) = seq {
            seqs.insert(s as u64);
        }
    }
    native
        .iter()
        .filter(|c| !seqs.contains(&c.seq))
        .cloned()
        .collect()
}

/// The REQ-XC-207 disposition — `inconclusive{infra:unreceipted_call}`.
pub fn unreceipted_disposition() -> Disposition {
    Disposition::Inconclusive {
        cause: InconclusiveCause::Infra(InfraCause::UnreceiptedCall),
    }
}

// ── the integrity-input bundle (§5.6 + design/34 F12; REQ-XC-317 leg) ────────

/// The bundle member roles — receipts, egress rows, the seal, native
/// artifacts and the terminal transcript (design/34 §1.7's scan inputs).
pub const BUNDLE_ROLES: &[&str] = &[
    "receipts",
    "egress_rows",
    "seal",
    "native_artifacts",
    "terminal_transcript",
];

/// `bundle_member(role, bytes)` — one manifest entry
/// `{role, sha256, bytes}`; a role outside the closed set refuses.
pub fn bundle_member(role: &str, bytes: &[u8]) -> Result<Json, CaptureError> {
    if !BUNDLE_ROLES.contains(&role) {
        return Err(err(
            "bundle_role",
            format!("role {role} not in the closed set"),
        ));
    }
    Ok(Json::obj([
        ("role", Json::str(role)),
        ("sha256", Json::str(sha256_hex(bytes))),
        ("bytes", Json::Int(bytes.len() as i64)),
    ]))
}

/// `integrity_bundle(members)` — the `hh-integrity-bundle/1` manifest
/// over `{role → bytes}` pairs: member digests plus a manifest digest
/// over the canonical members array.
pub fn integrity_bundle(members: &[(String, Vec<u8>)]) -> Result<Json, CaptureError> {
    let mut arr = Vec::new();
    for (role, bytes) in members {
        arr.push(bundle_member(role, bytes)?);
    }
    let manifest = format!(
        "sha256:{}",
        sha256_hex(Json::Arr(arr.clone()).to_canonical_string().as_bytes())
    );
    Ok(Json::obj([
        ("schema", Json::str(BUNDLE_SCHEMA)),
        ("members", Json::Arr(arr)),
        ("manifest_sha256", Json::str(manifest)),
    ]))
}

/// `verify_bundle(doc, bytes_of)` — every member's recorded digest and
/// byte count recompute against the supplied role bytes, and the
/// manifest digest recomputes over the canonical member array.
pub fn verify_bundle(
    doc: &Json,
    bytes_of: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), CaptureError> {
    let m = doc
        .as_obj()
        .ok_or_else(|| err("bundle", "bundle must be an object"))?;
    if m.get("schema").and_then(Json::as_str) != Some(BUNDLE_SCHEMA) {
        return Err(err("bundle", "schema is not hh-integrity-bundle/1"));
    }
    let members = m
        .get("members")
        .and_then(Json::as_arr)
        .ok_or_else(|| err("bundle", "members must be an array"))?;
    for (i, e) in members.iter().enumerate() {
        let em = e
            .as_obj()
            .ok_or_else(|| err("bundle", format!("members[{i}] must be an object")))?;
        let role = em
            .get("role")
            .and_then(Json::as_str)
            .ok_or_else(|| err("bundle", format!("members[{i}].role required")))?;
        if !BUNDLE_ROLES.contains(&role) {
            return Err(err(
                "bundle_role",
                format!("role {role} not in the closed set"),
            ));
        }
        let bytes =
            bytes_of(role).ok_or_else(|| err("bundle", format!("member {role} bytes absent")))?;
        let want = em
            .get("sha256")
            .and_then(Json::as_str)
            .ok_or_else(|| err("bundle", format!("members[{i}].sha256 required")))?;
        if want != sha256_hex(&bytes) {
            return Err(err(
                "bundle",
                format!("member {role} digest does not recompute"),
            ));
        }
        if em.get("bytes").and_then(Json::as_int) != Some(bytes.len() as i64) {
            return Err(err("bundle", format!("member {role} byte count drifted")));
        }
    }
    let manifest = m
        .get("manifest_sha256")
        .and_then(Json::as_str)
        .ok_or_else(|| err("bundle", "manifest_sha256 required"))?;
    let recomputed = format!(
        "sha256:{}",
        sha256_hex(Json::Arr(members.clone()).to_canonical_string().as_bytes())
    );
    if manifest != recomputed {
        return Err(err("bundle", "manifest_sha256 does not recompute"));
    }
    Ok(())
}

// ── ST4 self-modification probe (REQ-XC-328) ─────────────────────────────────

/// What the ST4 probe observed in the cell: the re-run version
/// command's stdout and the install-tree digest.
#[derive(Debug, Clone, PartialEq)]
pub struct St4Observation {
    /// The re-run version-probe stdout, verbatim.
    pub version_stdout: String,
    /// The re-hashed install-tree digest (`sha256:<64>` — the
    /// `tree_hash_argv` recipe over `install_tree_paths[0]`).
    pub install_tree_sha256: String,
}

/// `st4_verify(st2, st4)` — REQ-XC-328: re-running the adapter's
/// version command and re-hashing the install tree must reproduce the
/// ST2 observation byte for byte; any change flags
/// `harness_self_modified` (the slot still scores — §5.8's row).
pub fn st4_verify(
    st2: &crate::admission_build::St2Observation,
    st4: &St4Observation,
) -> Vec<SlotFlag> {
    if st4.version_stdout != st2.version_stdout || st4.install_tree_sha256 != st2.bundle_tree_sha256
    {
        vec![SlotFlag::HarnessSelfModified]
    } else {
        Vec::new()
    }
}

/// `st4_flags_to_json` — the flag spellings for the record leg.
pub fn st4_flags_to_json(flags: &[SlotFlag]) -> Json {
    Json::Arr(flags.iter().map(|f| Json::str(f.as_str())).collect())
}
