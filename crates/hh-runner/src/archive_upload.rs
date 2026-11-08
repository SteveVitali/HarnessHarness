//! `archive_upload` — the §4.10 per-window upload kit's controller-side
//! machinery (REQ-XH-102/103/104/107; AR-20/ADR-0437; R6.23 ARCHIVE (b)):
//! the tool admission record, the upload receipt's closed codec, the
//! pre-upload secret + live-credential scan over `hh-secrets`' one
//! detector set (CC1 — `hh-secret-scan` is the same surface), read-back
//! verification, append-only supersession, and the ordered remediation
//! records. `scripts/hf-upload.sh` is the operator-facing command; the
//! window skeletons (R6.40, R6.44, R6.45, R6.47) run the acts — nothing
//! uploads here.

use std::collections::BTreeMap;

use hharness_secrets::mask::{MaskEntry, MaskSet};
use hharness_secrets::redact::{detect, DetectorKind, DetectorSet, Leak, ScanTarget};
use hharness_secrets::types::SecretFingerprint;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;
use crate::receipt_index::{validate_index, FILE_ROLES};

/// The admitted tool (AR-20/ADR-0437): `hf`, the `huggingface_hub` CLI.
pub const UPLOAD_TOOL_NAME: &str = "hf";
/// The admitted package the tool ships in.
pub const UPLOAD_TOOL_PACKAGE: &str = "huggingface_hub";
/// The admitted exact version.
pub const UPLOAD_TOOL_VERSION: &str = "2.4.0";
/// The admission record the receipts cite.
pub const UPLOAD_TOOL_ADR: &str = "ADR-0437";
/// The upload receipt's schema.
pub const UPLOAD_RECEIPT_SCHEMA: &str = "hh-archive-upload/1";
/// The remediation rotation row's schema (the HUMAN act).
pub const ROTATION_ROW_SCHEMA: &str = "hh-human-act/1";
/// The operator purge record's schema.
pub const PURGE_RECORD_SCHEMA: &str = "hh-archive-purge/1";
/// The env-file member the write token is read from at use (REQ-XH-023).
pub const HF_WRITE_TOKEN_MEMBER: &str = "HF_WRITE_TOKEN";
/// The per-invocation env name the tool reads.
pub const HF_TOKEN_ENV: &str = "HF_TOKEN";

/// A refused upload-kit act — `{rule, detail}`.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadError {
    /// The refusal rule.
    pub rule: &'static str,
    /// The verbatim detail.
    pub detail: String,
}
impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for UploadError {}
fn ue(rule: &'static str, detail: impl Into<String>) -> UploadError {
    UploadError {
        rule,
        detail: detail.into(),
    }
}

/// The tool admission record — `{name, package, version, admission,
/// scope, token_handling}`; the machine-readable half of ADR-0437 (the
/// AR-15 `gcloud` precedent: identity + exact version + scope).
pub fn tool_admission() -> Json {
    Json::obj([
        ("name", Json::str(UPLOAD_TOOL_NAME)),
        ("package", Json::str(UPLOAD_TOOL_PACKAGE)),
        ("version", Json::str(UPLOAD_TOOL_VERSION)),
        ("admission", Json::str(UPLOAD_TOOL_ADR)),
        ("scope", Json::str("one private HF dataset repository")),
        (
            "token_handling",
            Json::str("HF_TOKEN env per invocation; no login"),
        ),
    ])
}

/// `upload` argv for one file — the only upload verb the kit emits;
/// `login`/`auth` can never appear (REQ-XH-023).
pub fn upload_argv(repo: &str, local: &str, path_in_repo: &str) -> Vec<String> {
    vec![
        "upload".to_string(),
        repo.to_string(),
        local.to_string(),
        path_in_repo.to_string(),
        "--repo-type".to_string(),
        "dataset".to_string(),
    ]
}

/// `download` argv for one read-back file.
pub fn download_argv(repo: &str, path_in_repo: &str, local_dir: &str) -> Vec<String> {
    vec![
        "download".to_string(),
        repo.to_string(),
        path_in_repo.to_string(),
        "--repo-type".to_string(),
        "dataset".to_string(),
        "--local-dir".to_string(),
        local_dir.to_string(),
    ]
}

/// `hh-archive-upload/1` — `{schema, kind, window, dataset{repo,
/// revision, visibility}, tool{…admission…}, provided{HF_WRITE_TOKEN:
/// yes|no}, files[{path, sha256, bytes}], commands[], verification}` —
/// the receipt the upload act commits; secret presence is `provided`
/// only (REQ-XH-023/X-05).
pub fn upload_receipt(
    kind: &str,
    window: &str,
    dataset: (&str, &str, &str),
    files: &[Json],
    provided_token: bool,
    commands: &[Json],
    verification: Option<&str>,
) -> Json {
    let mut m = vec![
        ("schema", Json::str(UPLOAD_RECEIPT_SCHEMA)),
        ("kind", Json::str(kind)),
        ("window", Json::str(window)),
        (
            "dataset",
            Json::obj([
                ("repo", Json::str(dataset.0)),
                ("revision", Json::str(dataset.1)),
                ("visibility", Json::str(dataset.2)),
            ]),
        ),
        ("tool", tool_admission()),
        (
            "provided",
            Json::obj([(
                HF_WRITE_TOKEN_MEMBER,
                Json::str(if provided_token { "yes" } else { "no" }),
            )]),
        ),
        ("files", Json::Arr(files.to_vec())),
        ("commands", Json::Arr(commands.to_vec())),
    ];
    if let Some(v) = verification {
        m.push(("verification", Json::str(v)));
    }
    Json::obj(m)
}

/// The closed upload-receipt codec — unknown members and wrong shapes
/// refuse; `visibility` is `private`-only (the dataset stays private
/// until a publication gate — REQ-XH-105's flip is not this row's).
pub fn validate_upload_receipt(doc: &Json) -> Result<(), UploadError> {
    let m = doc
        .as_obj()
        .ok_or_else(|| ue("shape", "receipt must be an object"))?;
    for k in m.keys() {
        if ![
            "schema",
            "kind",
            "window",
            "dataset",
            "tool",
            "provided",
            "files",
            "commands",
            "verification",
        ]
        .contains(&k.as_str())
        {
            return Err(ue("member", format!("receipt unknown member {k}")));
        }
    }
    if m.get("schema").and_then(Json::as_str) != Some(UPLOAD_RECEIPT_SCHEMA) {
        return Err(ue("schema", "schema is not hh-archive-upload/1"));
    }
    match m.get("kind").and_then(Json::as_str) {
        Some("upload") | Some("verify") | Some("remediate") => {}
        _ => return Err(ue("kind", "kind not upload|verify|remediate")),
    }
    let ds = m
        .get("dataset")
        .and_then(Json::as_obj)
        .ok_or_else(|| ue("dataset", "dataset object required"))?;
    for k in ds.keys() {
        if !["repo", "revision", "visibility"].contains(&k.as_str()) {
            return Err(ue("dataset", format!("dataset unknown member {k}")));
        }
    }
    if ds.get("repo").and_then(Json::as_str).map(|s| s.is_empty()) != Some(false) {
        return Err(ue("dataset", "dataset.repo required"));
    }
    if ds.get("visibility").and_then(Json::as_str) != Some("private") {
        return Err(ue(
            "dataset.not_private",
            "the dataset target is not private (REQ-XH-102)",
        ));
    }
    let tool = m
        .get("tool")
        .and_then(Json::as_obj)
        .ok_or_else(|| ue("tool", "tool admission block required"))?;
    if tool.get("name").and_then(Json::as_str) != Some(UPLOAD_TOOL_NAME)
        || tool.get("version").and_then(Json::as_str) != Some(UPLOAD_TOOL_VERSION)
        || tool.get("admission").and_then(Json::as_str) != Some(UPLOAD_TOOL_ADR)
    {
        return Err(ue("tool", "tool does not match the ADR-0437 admission"));
    }
    let prov = m
        .get("provided")
        .and_then(Json::as_obj)
        .ok_or_else(|| ue("provided", "provided block required"))?;
    match prov.get(HF_WRITE_TOKEN_MEMBER).and_then(Json::as_str) {
        Some("yes") | Some("no") => {}
        _ => {
            return Err(ue(
                "provided",
                "provided records only yes|no — never a value",
            ))
        }
    }
    for (fi, f) in m
        .get("files")
        .and_then(Json::as_arr)
        .ok_or_else(|| ue("files", "files array required"))?
        .iter()
        .enumerate()
    {
        let fm = f
            .as_obj()
            .ok_or_else(|| ue("files", format!("files[{fi}] must be an object")))?;
        for k in fm.keys() {
            if !["path", "sha256", "bytes"].contains(&k.as_str()) {
                return Err(ue("files", format!("files[{fi}] unknown member {k}")));
            }
        }
        let sha = fm.get("sha256").and_then(Json::as_str).unwrap_or_default();
        if !(sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err(ue("files", format!("files[{fi}].sha256 must be 64-hex")));
        }
    }
    if let Some(v) = m.get("verification").and_then(Json::as_str) {
        if !v.starts_with("archive_verified")
            && !v.starts_with("inconclusive{readback_unavailable}")
            && !v.starts_with("refused{")
        {
            return Err(ue("verification", "verification spelling refused"));
        }
    }
    Ok(())
}

// ── REQ-XH-103 — the pre-upload scan (three tiers) ──────────────────────

/// The file roles a revoked slot token may remain in — private raw
/// receipts and store segments.
pub const RAW_FILE_ROLES: &[&str] = &["raw_receipts", "egress_log", "native_log", "store_segment"];
/// The condensed/derived roles — a slot token found here was not
/// tombstoned and refuses (REQ-XH-103).
pub const DERIVED_FILE_ROLES: &[&str] = &["transcript", "atif"];

/// One declared slot token with its revocation evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotToken {
    /// The token value (kernel-side only — never recorded).
    pub value: String,
    /// The `security.credential.revoked` row ref; `None` = live.
    pub revoked_ref: Option<String>,
}

/// The scan verdict — labels and refs only, never token bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanOutcome {
    /// The file passes; `stays` carries the revoked tokens' audit
    /// fingerprints permitted in the raw role.
    Clean {
        /// `sf:` fingerprints of revoked tokens permitted in raw files.
        stays: Vec<String>,
    },
    /// The file's upload refuses and the archive row stops.
    Refuse {
        /// The refused file.
        file: String,
        /// `live_credential | unrevoked_slot_token | tombstone_missing`.
        reason: &'static str,
        /// The detector labels that fired (never the bytes).
        hits: Vec<String>,
    },
}

fn fp(value: &str, channel: &str) -> SecretFingerprint {
    SecretFingerprint {
        channel_id: channel.into(),
        revision: 0,
        digest: sha256_hex(format!("sf\u{1f}{value}").as_bytes())[..32].to_string(),
    }
}

fn token_detectors(tokens: &[SlotToken]) -> DetectorSet {
    let mut ms = MaskSet::default();
    for t in tokens {
        ms.insert(MaskEntry {
            channel_id: "slot_token".into(),
            revision: 0,
            value: t.value.clone(),
            fingerprint: fp(&t.value, "slot_token"),
            live: true,
            placeholder: None,
        });
    }
    DetectorSet {
        mask_set: ms,
        patterns: Vec::new(),
        canaries: Vec::new(),
    }
}

/// `pre_upload_scan(role, path, text, live, slot_tokens)` — REQ-XH-103:
/// the repository detector set + the live-credential mask set sweep
/// first — a live hit refuses the file and stops the row. Then each
/// declared slot token: unrevoked is a live hit; revoked stays in a raw
/// role; a token in a derived role was not tombstoned and refuses.
pub fn pre_upload_scan(
    role: &str,
    path: &str,
    text: &str,
    live: &DetectorSet,
    slot_tokens: &[SlotToken],
) -> ScanOutcome {
    // Tier 1+3 — live credentials and the repository pattern set.
    let hits: Vec<Leak> = {
        let mut v = Vec::new();
        for h in detect(text, live) {
            if h.detector != DetectorKind::PlaceholderPassthrough {
                v.push(Leak {
                    target: ScanTarget::Artifact(path.to_string()),
                    detector: h.detector,
                    fingerprint: h.tombstone.as_ref().and_then(|t| t.fingerprint.clone()),
                    label: h.tombstone.as_ref().map(|t| t.label.clone()),
                });
            }
        }
        v
    };
    if !hits.is_empty() {
        return ScanOutcome::Refuse {
            file: path.to_string(),
            reason: "live_credential",
            hits: hits
                .iter()
                .map(|l| {
                    format!(
                        "{}:{}",
                        l.detector.as_str(),
                        l.label.clone().unwrap_or_default()
                    )
                })
                .collect(),
        };
    }
    // Tier 2 — declared slot tokens.
    let det = token_detectors(slot_tokens);
    let mut stays = Vec::new();
    for h in detect(text, &det) {
        if h.detector == DetectorKind::PlaceholderPassthrough {
            continue;
        }
        let fpr = h
            .tombstone
            .as_ref()
            .and_then(|t| t.fingerprint.clone())
            .unwrap_or_default();
        let tok = slot_tokens
            .iter()
            .find(|t| fp(&t.value, "slot_token").render() == fpr)
            .cloned()
            .unwrap_or(SlotToken {
                value: String::new(),
                revoked_ref: None,
            });
        if tok.revoked_ref.is_none() {
            return ScanOutcome::Refuse {
                file: path.to_string(),
                reason: "unrevoked_slot_token",
                hits: vec![fpr],
            };
        }
        if !RAW_FILE_ROLES.contains(&role) {
            return ScanOutcome::Refuse {
                file: path.to_string(),
                reason: "tombstone_missing",
                hits: vec![fpr],
            };
        }
        stays.push(fpr);
    }
    let _ = DERIVED_FILE_ROLES;
    ScanOutcome::Clean { stays }
}

// ── REQ-XH-104 — read-back verification + append-only supersession ──────

/// The read-back verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadbackOutcome {
    /// Every `archived` member hashed back equal → `archive_verified`.
    ArchiveVerified,
    /// A file's read-back sha256 differs from the index's — the row fails.
    Mismatch {
        /// The divergent path.
        path: String,
    },
    /// The read-back was unavailable — `inconclusive{readback_unavailable}`,
    /// never `archive_verified`.
    ReadbackUnavailable {
        /// The file whose read-back was unavailable.
        path: String,
    },
}
impl ReadbackOutcome {
    /// The receipt spelling.
    pub fn as_str(&self) -> String {
        match self {
            ReadbackOutcome::ArchiveVerified => "archive_verified".to_string(),
            ReadbackOutcome::Mismatch { path } => format!("refused{{sha_mismatch:{path}}}"),
            ReadbackOutcome::ReadbackUnavailable { .. } => {
                "inconclusive{readback_unavailable}".to_string()
            }
        }
    }
}

/// `verify_readback(index, readback)` — REQ-XH-104: every per-attempt
/// `files[]` member and every `upload: archived` `artifacts[]` member's
/// sha256 must equal the read-back; a missing member is
/// `inconclusive{readback_unavailable}` — never `archive_verified`.
/// `readback` maps `path → Some(sha256)`; `None` = unavailable.
pub fn verify_readback(
    index: &Json,
    readback: &BTreeMap<String, Option<String>>,
) -> ReadbackOutcome {
    let check = |path: &str, sha: &str| -> Option<ReadbackOutcome> {
        match readback.get(path) {
            Some(Some(got)) if got == sha => None,
            Some(Some(_)) => Some(ReadbackOutcome::Mismatch {
                path: path.to_string(),
            }),
            _ => Some(ReadbackOutcome::ReadbackUnavailable {
                path: path.to_string(),
            }),
        }
    };
    if let Some(attempts) = index.get("attempts").and_then(Json::as_arr) {
        for a in attempts {
            for f in a
                .get("files")
                .and_then(Json::as_arr)
                .cloned()
                .unwrap_or_default()
            {
                let path = f.get("path").and_then(Json::as_str).unwrap_or_default();
                let sha = f.get("sha256").and_then(Json::as_str).unwrap_or_default();
                if let Some(o) = check(path, sha) {
                    return o;
                }
            }
        }
    }
    if let Some(arts) = index.get("artifacts").and_then(Json::as_arr) {
        for a in arts {
            if a.get("upload").and_then(Json::as_str) != Some("archived") {
                continue;
            }
            let path = a.get("path").and_then(Json::as_str).unwrap_or_default();
            let sha = a.get("sha256").and_then(Json::as_str).unwrap_or_default();
            if let Some(o) = check(path, sha) {
                return o;
            }
        }
    }
    ReadbackOutcome::ArchiveVerified
}

/// `supersede_index(new_index, prior_ref)` — REQ-XH-104's append-only
/// rule: a re-upload mints a new index revision that references the
/// earlier one (`supersedes: [{ref}]`); the result must validate
/// through R6.12's codec (X-04).
pub fn supersede_index(new_index: &mut Json, prior_ref: &str) -> Result<(), UploadError> {
    let m = match new_index {
        Json::Obj(m) => m,
        _ => return Err(ue("shape", "index must be an object")),
    };
    let mut sup = m
        .remove("supersedes")
        .and_then(|v| v.as_arr().cloned())
        .unwrap_or_default();
    sup.push(Json::obj([("ref", Json::str(prior_ref))]));
    m.insert("supersedes".to_string(), Json::Arr(sup));
    validate_index(new_index).map_err(|e| ue("index", e.to_string()))
}

// ── REQ-XH-107 — the ordered remediation ────────────────────────────────

/// `remediation_records(rotated, purge{revision, note}, tombstone{
/// reason, refs})` — REQ-XH-107's exact order: the HUMAN rotation row
/// (`provided: yes/no` only), then the operator-recorded purge record
/// (Q-X59 pending — recorded, never executed by this row), then the
/// tombstone/supersede index patch naming `reason` and the affected
/// refs — with no secret value anywhere.
pub fn remediation_records(
    rotated_provided: bool,
    dataset_repo: &str,
    revision: &str,
    purge_note: &str,
    tombstone_reason: &str,
    tombstoned_refs: &[String],
) -> Vec<Json> {
    let rotation = Json::obj([
        ("schema", Json::str(ROTATION_ROW_SCHEMA)),
        ("act", Json::str("credential_rotation")),
        ("human", Json::Bool(true)),
        (
            "provided",
            Json::obj([(
                "credential_rotated",
                Json::str(if rotated_provided { "yes" } else { "no" }),
            )]),
        ),
    ]);
    let purge = Json::obj([
        ("schema", Json::str(PURGE_RECORD_SCHEMA)),
        ("act", Json::str("dataset_revision_purge")),
        ("repo", Json::str(dataset_repo)),
        ("revision", Json::str(revision)),
        ("operator_recorded", Json::Bool(true)),
        ("note", Json::str(purge_note)),
        (
            "pre_authorization",
            Json::str("Q-X59 pending — recorded default"),
        ),
    ]);
    let tombstone = Json::obj([
        ("schema", Json::str("hh-receipt-index-tombstone/1")),
        ("act", Json::str("tombstone")),
        (
            "tombstone",
            Json::obj([("reason", Json::str(tombstone_reason))]),
        ),
        (
            "supersedes",
            Json::Arr(
                tombstoned_refs
                    .iter()
                    .map(|r| Json::obj([("ref", Json::str(r))]))
                    .collect(),
            ),
        ),
    ]);
    vec![rotation, purge, tombstone]
}

/// The closed file-role check the kit applies (R6.12's set — `files[]`
/// roles only; artifact roles travel the pass-level section).
pub fn file_role_ok(role: &str) -> bool {
    FILE_ROLES.contains(&role)
}
