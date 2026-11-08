//! `carried` — the §5.2 "build once, carry by digest" machinery
//! (REQ-XC-330; O2 RA-1, O3 RD-13; AR-20/ADR-0437): the two HH binary
//! sets, the export/load/verify path over `hh-receipt-index/1`'s
//! pass-level `artifacts[]` (REQ-XF-124 — R6.12's codec is the one
//! writer, X-04/CC1), the slot-identity digests, and the typed
//! refusals. The load path uses only the REQ-XB-203 IAP route (X-06).
//! Nothing builds, exports or loads for real — the W1/W3/W4 window
//! skeletons (R6.32, R6.44, R6.47) run the host acts after their gates.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::admission::{image_ref_by_digest, AdmissionError};
use crate::disposition::AdmissionReason;
use crate::json_ext::JsonExt;
use crate::receipt_index::{artifact_entry, IndexError};

fn err(rule: &'static str, detail: impl Into<String>) -> AdmissionError {
    AdmissionError {
        rule,
        detail: detail.into(),
    }
}

// ── the two carried binary sets (REQ-XC-330) ────────────────────────────

/// `hh-binary-set/1` — the one-manifest-per-set record kind.
pub const BINARY_SET_SCHEMA: &str = "hh-binary-set/1";
/// The infrastructure set's name (built in W1, carried through W4).
pub const INFRA_SET_NAME: &str = "infrastructure";
/// The H set's name (built at W3's start, frozen at G-L1).
pub const H_SET_NAME: &str = "h";

/// `(index member, build-output path)` — the REQ-XC-330 infrastructure
/// set: runner, helper, relay, `hh-model-proxy` and the egress mediator
/// binaries.
pub const INFRA_SET_MEMBERS: &[(&str, &str)] = &[
    ("runner", "target/release/hh-runner"),
    ("helper", "target/release/hh-helper"),
    ("relay", "target/release/hh-cell-relay"),
    ("model_proxy", "target/release/hh-model-proxy"),
    ("egress_mediator", "target/release/hh-egress-mediator"),
];

/// The H set — the `hh-kernel` participant build plus the HIR and the
/// compiled artifacts (the phase-4 level's included; §5.2 RD-1).
pub const H_SET_MEMBERS: &[(&str, &str)] = &[
    ("hh_kernel", "target/release/hh-kernel"),
    ("hir", "artifacts/hir"),
    ("compiled_artifacts", "artifacts/compiled"),
];

fn declared(set: &str) -> Result<&'static [(&'static str, &'static str)], AdmissionError> {
    match set {
        "infrastructure" => Ok(INFRA_SET_MEMBERS),
        "h" => Ok(H_SET_MEMBERS),
        _ => Err(err(
            "set_unknown",
            format!("binary set {set:?} is not in the closed set"),
        )),
    }
}

fn sha64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The one carried identity — `sha256:<64>` over the canonical member
/// map (the workspace canonicalizer; never a second scheme, CC1).
pub fn set_digest(set: &str, members: &BTreeMap<String, String>) -> Result<String, AdmissionError> {
    let decl = declared(set)?;
    let mut canon = BTreeMap::new();
    for (name, _) in decl {
        let sha = members.get(*name).ok_or_else(|| {
            err(
                "member_missing",
                format!("{set}-set member {name} required"),
            )
        })?;
        if !sha64(sha) {
            return Err(err(
                "member_sha256",
                format!("member {name} sha256 must be 64-hex"),
            ));
        }
        canon.insert(name.to_string(), Json::str(sha));
    }
    for k in members.keys() {
        if !canon.contains_key(k) {
            return Err(err(
                "member_unknown",
                format!("{k} is not a {set}-set member"),
            ));
        }
    }
    let doc = Json::obj([("set", Json::str(set)), ("members", Json::Obj(canon))]);
    Ok(format!(
        "sha256:{}",
        sha256_hex(doc.to_canonical_string().as_bytes())
    ))
}

/// `hh-binary-set/1` — `{schema, set, members{name:{binary_sha256}},
/// set_digest}`; the closed member set is enforced.
pub fn set_manifest(set: &str, members: &BTreeMap<String, String>) -> Result<Json, AdmissionError> {
    let digest = set_digest(set, members)?;
    let mut mj = BTreeMap::new();
    for (name, _) in declared(set)? {
        mj.insert(
            name.to_string(),
            Json::obj([("binary_sha256", Json::str(members.get(*name).unwrap()))]),
        );
    }
    Ok(Json::obj([
        ("schema", Json::str(BINARY_SET_SCHEMA)),
        ("set", Json::str(set)),
        ("members", Json::Obj(mj)),
        ("set_digest", Json::str(&digest)),
    ]))
}

/// The pinned-container build recipe — one Containerfile per set. `FROM`
/// is digest-pinned; the workspace lands at a 40-hex commit; `cargo
/// build --release --locked` is the only build act; a `sha256sum`
/// manifest seals the member digests (REQ-XC-330, REQ-XC-303's
/// content-addressed-manifest rule carried over).
#[derive(Debug, Clone, PartialEq)]
pub struct BinarySetRecipe {
    /// `infrastructure | h`.
    pub set: String,
    /// The toolchain image (`name@sha256:<64>` — digest only).
    pub toolchain_base: String,
    /// The committed workspace revision the build checks out (40-hex).
    pub workspace_commit: String,
    /// `cargo build -p` selections, in declared order.
    pub packages: Vec<String>,
    /// Extra carried outputs the recipe stages before the member copy
    /// (the H set's `artifacts/hir` + `artifacts/compiled` producers).
    pub extra_outputs: Vec<String>,
}

impl BinarySetRecipe {
    /// The rendered Containerfile — deterministic, golden-pinned.
    pub fn render(&self) -> Result<String, AdmissionError> {
        if !image_ref_by_digest(&self.toolchain_base) {
            return Err(err(
                "toolchain_digest",
                "toolchain base must be digest-pinned (name@sha256:<64>)",
            ));
        }
        if self.workspace_commit.len() != 40
            || !self.workspace_commit.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(err("commit_pin", "workspace_commit must be a 40-hex pin"));
        }
        let decl = declared(&self.set)?;
        if self.packages.is_empty() {
            return Err(err("packages", "at least one -p selection required"));
        }
        let set_dir = format!("\u{2f}opt/hh-set-{}", self.set);
        let mut out = String::new();
        out.push_str("# hh-binary-set/1 — REQ-XC-330 carried set build (R6.23; AR-20/ADR-0437)\n");
        out.push_str(&format!("# set: {}\n", self.set));
        out.push_str(&format!("FROM {}\n", self.toolchain_base));
        out.push_str(&format!(
            "ARG HH_WORKSPACE_COMMIT={}\n",
            self.workspace_commit
        ));
        out.push_str("COPY workspace/ /src/\n");
        out.push_str("WORKDIR /src\n");
        out.push_str(&format!("RUN git checkout {}\n", self.workspace_commit));
        let pkgs = self
            .packages
            .iter()
            .map(|p| format!("-p {p}"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("RUN cargo build --release --locked {pkgs}\n"));
        for step in &self.extra_outputs {
            out.push_str(&format!("RUN {step}\n"));
        }
        out.push_str(&format!("RUN mkdir -p {set_dir}\n"));
        for (name, src) in decl {
            out.push_str(&format!("RUN cp -r '{src}' '{set_dir}/{name}'\n"));
        }
        let names = decl.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(" ");
        out.push_str(&format!(
            "RUN cd {set_dir} && sha256sum {names} > MANIFEST.sha256\n"
        ));
        Ok(out)
    }
}

// ── export into the harvested run root + `artifacts[]` ──────────────────

/// The export shape (REQ-XC-330): a full `podman save`, the
/// harness-specific-blobs-plus-config small-transfer variant (base
/// blobs re-pulled by digest at load), or the binary-set archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportShape {
    /// `podman save --format oci-archive -o <out> <image>` — the whole image.
    PodmanSave,
    /// `podman save --format oci-dir` — only the harness-specific blobs
    /// plus the image config travel; `base_blobs` re-pull by digest.
    BlobsPlusConfig,
    /// `tar -cf <out> -C <ctx> <inputs>` — the binary-set archive.
    ArchiveTar,
}

/// One export act — argv derived once; the shims pin it in the battery.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlan {
    /// The export shape.
    pub shape: ExportShape,
    /// `layer_export | bundle | binary_set` (the closed role set).
    pub role: String,
    /// The artifact name.
    pub name: String,
    /// The window that exports it (`W1`/`W3`).
    pub window: String,
    /// The image ref for the podman shapes (`name@sha256` — digest only).
    pub image: Option<String>,
    /// The base blobs omitted from the transfer, re-pulled by digest.
    pub base_blobs: Vec<String>,
    /// `ArchiveTar` inputs, context-relative.
    pub inputs: Vec<String>,
    /// `ArchiveTar` `-C` context dir.
    pub context_dir: String,
    /// The export path under the harvested run root.
    pub out_path: String,
    /// The slot-recorded identity — `("image_id"|"digest", sha256:<64>)`;
    /// a `name@sha256` RepoDigest is never an identity.
    pub identity: (String, String),
}

/// A refused host act — `{rule, detail}`, typed like `InstallFailure`.
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedFailure {
    /// The refusal rule.
    pub rule: &'static str,
    /// The verbatim detail.
    pub detail: String,
}
impl std::fmt::Display for CarriedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for CarriedFailure {}
fn cf(rule: &'static str, detail: impl Into<String>) -> CarriedFailure {
    CarriedFailure {
        rule,
        detail: detail.into(),
    }
}

/// `hh-carried-export/1` — the export act's record kind.
pub const EXPORT_RECORD_SCHEMA: &str = "hh-carried-export/1";
/// `hh-carried-load/1` — the load act's record kind.
pub const LOAD_RECORD_SCHEMA: &str = "hh-carried-load/1";

/// The identity-member contract: `image_id` or `digest` values are
/// `sha256:<64>`; a `name@…` RepoDigest is never the identity a slot
/// records (a load drops it — REQ-XC-330).
pub fn identity_member_ok((key, value): &(String, String)) -> Result<(), AdmissionError> {
    match key.as_str() {
        "image_id" | "digest" => {}
        k => {
            return Err(err(
                "identity_member",
                format!("member {k} is not image_id|digest"),
            ))
        }
    }
    if value.contains('@') || !value.strip_prefix("sha256:").map(sha64).unwrap_or(false) {
        return Err(err(
            "identity_member",
            format!("identity {value:?} is not a sha256:<64> — a RepoDigest is never the identity"),
        ));
    }
    Ok(())
}

impl ExportPlan {
    /// The pinned argv — `podman save` or `tar`; image refs are
    /// digest-only.
    pub fn argv(&self) -> Result<Vec<String>, AdmissionError> {
        identity_member_ok(&self.identity)?;
        match self.shape {
            ExportShape::PodmanSave | ExportShape::BlobsPlusConfig => {
                let image = self
                    .image
                    .as_deref()
                    .ok_or_else(|| err("image_ref", "podman export requires an image ref"))?;
                if !image_ref_by_digest(image) {
                    return Err(err(
                        "image_ref",
                        format!("image {image:?} must be digest-pinned"),
                    ));
                }
                let fmt = match self.shape {
                    ExportShape::PodmanSave => "oci-archive",
                    _ => "oci-dir",
                };
                Ok(vec![
                    "save".to_string(),
                    "--format".to_string(),
                    fmt.to_string(),
                    "-o".to_string(),
                    self.out_path.clone(),
                    image.to_string(),
                ])
            }
            ExportShape::ArchiveTar => {
                if self.inputs.is_empty() {
                    return Err(err("inputs", "archive export requires inputs"));
                }
                let mut a = vec![
                    "-cf".to_string(),
                    self.out_path.clone(),
                    "-C".to_string(),
                    self.context_dir.clone(),
                ];
                a.extend(self.inputs.iter().cloned());
                Ok(a)
            }
        }
    }

    /// The executable the argv rides (`podman` or `tar`).
    pub fn tool(&self) -> &'static str {
        match self.shape {
            ExportShape::ArchiveTar => "tar",
            _ => "podman",
        }
    }

    /// The `artifacts[]` entry through R6.12's codec (X-04 — the codec
    /// is the one writer).
    pub fn index_entry(
        &self,
        bytes: &[u8],
        upload: &str,
        dataset_path: Option<&str>,
    ) -> Result<Json, IndexError> {
        artifact_entry(
            &self.role,
            &self.name,
            &self.window,
            (&self.identity.0, &self.identity.1),
            dataset_path,
            bytes,
            upload,
        )
    }

    /// The REQ-XH-118 egress input — a labelled quantity for the
    /// exporting window's quote line (`artifact_export:<role>:<name>`).
    pub fn egress_line(&self, bytes: u64) -> Json {
        Json::obj([
            (
                "label",
                Json::str(format!("artifact_export:{}:{}", self.role, self.name)),
            ),
            ("bytes", Json::Int(bytes as i64)),
            ("window", Json::str(&self.window)),
            ("priced", Json::str("n/a{estimate}")),
        ])
    }
}

fn dir_bytes_sha256(root: &Path) -> Result<(u64, String), CarriedFailure> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|x| cf("export_bytes", format!("{x}")))? {
            let p = e.map_err(|x| cf("export_bytes", format!("{x}")))?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .to_string();
                files.push((
                    rel,
                    std::fs::read(&p).map_err(|x| cf("export_bytes", format!("{x}")))?,
                ));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut total = 0u64;
    let mut fold = String::new();
    for (rel, b) in &files {
        total += b.len() as u64;
        fold.push_str(rel);
        fold.push('\0');
        fold.push_str(&sha256_hex(b));
        fold.push('\n');
    }
    Ok((total, sha256_hex(fold.as_bytes())))
}

/// `run_export(plan, log_path)` — the host act: spawn the plan's tool,
/// log argv+rc, then record `{schema, role, name, window, argv,
/// out_path, sha256, bytes, elapsed_ms, base_blobs?}` — the sha256 the
/// index entry carries. A non-zero exit refuses; nothing is retried.
pub fn run_export(plan: &ExportPlan, log_path: &Path) -> Result<Json, CarriedFailure> {
    let argv = plan.argv().map_err(|e| cf(e.rule, e.detail))?;
    let started = Instant::now();
    let out = std::process::Command::new(plan.tool())
        .args(&argv)
        .output()
        .map_err(|e| cf("export_spawn", format!("{} spawn: {e}", plan.tool())))?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let mut log = format!("{} {}\nexit={}\n", plan.tool(), argv.join(" "), out.status);
    log.push_str(&String::from_utf8_lossy(&out.stdout));
    let _ = std::fs::write(log_path, &log);
    if !out.status.success() {
        return Err(cf(
            "export_failed",
            format!("{} rc={}", plan.tool(), out.status.code().unwrap_or(-1)),
        ));
    }
    let (bytes, sha256) = if plan.shape == ExportShape::BlobsPlusConfig {
        dir_bytes_sha256(Path::new(&plan.out_path))?
    } else {
        let b = std::fs::read(&plan.out_path)
            .map_err(|e| cf("export_bytes", format!("{}: {e}", plan.out_path)))?;
        (b.len() as u64, sha256_hex(&b))
    };
    Ok(Json::obj([
        ("schema", Json::str(EXPORT_RECORD_SCHEMA)),
        ("role", Json::str(&plan.role)),
        ("name", Json::str(&plan.name)),
        ("window", Json::str(&plan.window)),
        ("tool", Json::str(plan.tool())),
        ("argv", Json::Arr(argv.iter().map(Json::str).collect())),
        ("out_path", Json::str(&plan.out_path)),
        ("sha256", Json::str(&sha256)),
        ("bytes", Json::Int(bytes as i64)),
        ("elapsed_ms", Json::Int(elapsed_ms as i64)),
        (
            "base_blobs",
            Json::Arr(plan.base_blobs.iter().map(Json::str).collect()),
        ),
    ]))
}

// ── load over the IAP path + image-ID verification ──────────────────────

/// The load form — `podman load` for image artifacts, the archive
/// extract for binary sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// `podman load -i <remote>` then base-blob re-pull + image inspect.
    PodmanLoad,
    /// `tar -xf <remote> -C <dest>` then `sha256sum -c MANIFEST.sha256`.
    Archive,
}

/// One load act — the controller copies over IAP (REQ-XB-203), the VM
/// re-materializes the artifact, the image ID is verified.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadPlan {
    /// The artifact name.
    pub name: String,
    /// The controller-side export path.
    pub source: String,
    /// The VM instance (IAP target).
    pub instance: String,
    /// The GCP project.
    pub project: String,
    /// The zone.
    pub zone: String,
    /// The remote dir the export lands in.
    pub remote_dir: String,
    /// The load form.
    pub kind: LoadKind,
    /// Base blobs re-pulled by digest (`name@sha256:<64>` refs).
    pub base_blobs: Vec<String>,
    /// The ref `podman image inspect` reads after `load` (image_id
    /// verification target).
    pub image_ref: Option<String>,
    /// The expected image ID (config digest `sha256:<64>`).
    pub expected_image_id: Option<String>,
    /// `Archive` dest dir.
    pub dest_dir: String,
}

fn basename(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

impl LoadPlan {
    /// The remote file path.
    fn remote(&self) -> String {
        format!(
            "{}/{}",
            self.remote_dir.trim_end_matches('/'),
            basename(&self.source)
        )
    }
    /// The controller-side argv — `gcloud compute scp --tunnel-through-iap`
    /// (the R6.13 REQ-XB-203 idiom; X-06 — no other route).
    pub fn scp_argv(&self) -> Vec<String> {
        vec![
            "compute".to_string(),
            "scp".to_string(),
            "--tunnel-through-iap".to_string(),
            "--project".to_string(),
            self.project.clone(),
            "--zone".to_string(),
            self.zone.clone(),
            self.source.clone(),
            format!("{}:{}", self.instance, self.remote()),
        ]
    }
    /// The VM-side commands (each rides one `gcloud compute ssh
    /// --command` over the same tunnel).
    pub fn vm_commands(&self) -> Vec<String> {
        let remote = self.remote();
        match self.kind {
            LoadKind::PodmanLoad => {
                let mut v = vec![format!("podman load -i '{remote}'")];
                for b in &self.base_blobs {
                    v.push(format!("podman pull '{b}'"));
                }
                if let Some(r) = &self.image_ref {
                    v.push(format!("podman image inspect --format '{{{{.Id}}}}' '{r}'"));
                }
                v
            }
            LoadKind::Archive => vec![
                format!(
                    "mkdir -p '{}' && tar -xf '{}' -C '{}'",
                    self.dest_dir, remote, self.dest_dir
                ),
                format!("cd '{}' && sha256sum -c MANIFEST.sha256", self.dest_dir),
            ],
        }
    }
    /// The full `gcloud compute ssh` argv for one VM command.
    pub fn ssh_argv(&self, command: &str) -> Vec<String> {
        vec![
            "compute".to_string(),
            "ssh".to_string(),
            self.instance.clone(),
            "--project".to_string(),
            self.project.clone(),
            "--zone".to_string(),
            self.zone.clone(),
            "--tunnel-through-iap".to_string(),
            "--command".to_string(),
            command.to_string(),
        ]
    }
}

/// `image_id_normalize` — the identity form: `sha256:<64>` or a bare
/// 64-hex `podman` `.Id`; a `name@sha256:<d>` RepoDigest is never the
/// identity (REQ-XC-330).
pub fn image_id_normalize(s: &str) -> Result<String, AdmissionError> {
    if s.contains('@') {
        return Err(err(
            "image_id",
            format!("{s:?} is a RepoDigest — never the identity a slot records"),
        ));
    }
    let hex = s.strip_prefix("sha256:").unwrap_or(s);
    if sha64(hex) {
        Ok(hex.to_string())
    } else {
        Err(err(
            "image_id",
            format!("{s:?} is not an image ID (sha256:<64>)"),
        ))
    }
}

/// `verify_image_id(expected, observed)` — the config-digest check a W4
/// slot's admission rides: a mismatch is `HarnessLayerMismatch`, the
/// §5.8 row → `withheld{admission:harness_layer_mismatch}` through
/// `map_slot_facts` (REQ-XC-305/330's one refusal surface).
pub fn verify_image_id(expected: &str, observed: &str) -> Result<(), AdmissionReason> {
    let e = image_id_normalize(expected).map_err(|_| AdmissionReason::HarnessLayerMismatch)?;
    let o = image_id_normalize(observed).map_err(|_| AdmissionReason::HarnessLayerMismatch)?;
    if e == o {
        Ok(())
    } else {
        Err(AdmissionReason::HarnessLayerMismatch)
    }
}

/// `run_load(plan, log_path)` — the host act: `gcloud compute scp` over
/// IAP, then one `gcloud compute ssh --command` per VM act; the
/// `podman image inspect` reply is captured and, when the plan declares
/// `expected_image_id`, verified — a mismatch refuses
/// `load.image_id_mismatch`. `{schema, name, scp_argv, vm_commands,
/// image_id, elapsed_ms, verified}` records the act; `elapsed_ms` is
/// the window-overhead load term (REQ-XH-118).
pub fn run_load(plan: &LoadPlan, log_path: &Path) -> Result<Json, CarriedFailure> {
    let started = Instant::now();
    let mut log = String::new();
    let run = |argv: &[String], log: &mut String| -> Result<std::process::Output, CarriedFailure> {
        let out = std::process::Command::new("gcloud")
            .args(argv)
            .output()
            .map_err(|e| cf("load_spawn", format!("gcloud spawn: {e}")))?;
        log.push_str(&format!("gcloud {}\nexit={}\n", argv.join(" "), out.status));
        log.push_str(&String::from_utf8_lossy(&out.stdout));
        if !out.status.success() {
            let _ = std::fs::write(log_path, log);
            return Err(cf(
                "load_failed",
                format!("gcloud rc={}", out.status.code().unwrap_or(-1)),
            ));
        }
        Ok(out)
    };
    run(&plan.scp_argv(), &mut log)?;
    let mut observed_id: Option<String> = None;
    for cmd in plan.vm_commands() {
        let argv = plan.ssh_argv(&cmd);
        let out = run(&argv, &mut log)?;
        if cmd.starts_with("podman image inspect") {
            observed_id = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
        }
    }
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let _ = std::fs::write(log_path, &log);
    if let (Some(expected), Some(observed)) = (&plan.expected_image_id, &observed_id) {
        verify_image_id(expected, observed).map_err(|_| {
            cf(
                "load.image_id_mismatch",
                format!("expected {expected}, observed {observed}"),
            )
        })?;
    }
    Ok(Json::obj([
        ("schema", Json::str(LOAD_RECORD_SCHEMA)),
        ("name", Json::str(&plan.name)),
        (
            "kind",
            Json::str(if plan.kind == LoadKind::PodmanLoad {
                "podman_load"
            } else {
                "archive"
            }),
        ),
        (
            "scp_argv",
            Json::Arr(plan.scp_argv().iter().map(Json::str).collect()),
        ),
        (
            "vm_commands",
            Json::Arr(plan.vm_commands().iter().map(Json::str).collect()),
        ),
        ("image_id", observed_id.map(Json::str).unwrap_or(Json::Null)),
        ("elapsed_ms", Json::Int(elapsed_ms as i64)),
        ("verified", Json::Bool(plan.expected_image_id.is_some())),
    ]))
}

// ── build-once at pass level + the proprietary rule ─────────────────────

/// `carried_rebuild_guard(index, role, name, gate)` — REQ-XC-330: an
/// `artifacts[]` member already carried this pass refuses to rebuild
/// without a gate record `{gate, new_version}` (a rebuild is a new
/// participant or a new runner version — PROV-Wc's recorded scope,
/// never a silent overwrite).
pub fn carried_rebuild_guard(
    index: &Json,
    role: &str,
    name: &str,
    gate: Option<&Json>,
) -> Result<(), AdmissionError> {
    let carried = index
        .get("artifacts")
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter().any(|e| {
                e.get("role").and_then(Json::as_str) == Some(role)
                    && e.get("name").and_then(Json::as_str) == Some(name)
            })
        })
        .unwrap_or(false);
    if !carried {
        return Ok(());
    }
    let ok = gate
        .and_then(Json::as_obj)
        .map(|g| {
            g.get("gate").and_then(Json::as_str).map(|s| !s.is_empty()) == Some(true)
                && g.get("new_version")
                    .and_then(Json::as_str)
                    .map(|s| !s.is_empty())
                    == Some(true)
        })
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(err(
            "carried_rebuild",
            format!("artifact {role}:{name} is already carried this pass — a rebuild is a new participant/runner version and needs a gate record (REQ-XC-330)"),
        ))
    }
}

/// REQ-XC-330's last clause — a proprietary/source-available bundle or
/// layer is `digest_only` (its bytes stay on the controller) unless an
/// operator redistribution record names it (Q-X26's terms grant none).
pub fn artifact_upload_decision(posture: &str, redistribution_ref: Option<&str>) -> &'static str {
    match posture {
        "proprietary" | "source_available" => match redistribution_ref {
            Some(r) if !r.is_empty() => "archived",
            _ => "digest_only",
        },
        _ => "archived",
    }
}

// ── slot-identity digests (REQ-XC-330; §3.3 identity) ───────────────────

/// The arm lane — REQ-XC-330: every W4 slot records the
/// infrastructure-set digest; HH-native and V5 (`rsn-drop`) slots also
/// record the H-set digest frozen at G-L1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmLane {
    /// A hosted arm (original mini-swe-agent, Pi, Claude Code).
    Hosted,
    /// The HH-native arm.
    HhNative,
    /// The V5 `rsn-drop` perturbation arm.
    V5,
}
impl ArmLane {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ArmLane::Hosted => "hosted",
            ArmLane::HhNative => "hh_native",
            ArmLane::V5 => "v5",
        }
    }
    /// `true` when the arm's slots record the H-set digest.
    pub fn carries_h_set(self) -> bool {
        matches!(self, ArmLane::HhNative | ArmLane::V5)
    }
    /// The strict parse.
    pub fn parse(s: &str) -> Option<ArmLane> {
        match s {
            "hosted" => Some(ArmLane::Hosted),
            "hh_native" => Some(ArmLane::HhNative),
            "v5" => Some(ArmLane::V5),
            _ => None,
        }
    }
}

/// The pass's carried digests a slot records.
#[derive(Debug, Clone, Default)]
pub struct CarriedSets {
    /// `member → sha256` for the infrastructure set.
    pub infra_members: BTreeMap<String, String>,
    /// `member → sha256` for the H set, when built.
    pub h_members: Option<BTreeMap<String, String>>,
    /// The gate the H set froze at (`G-L1`).
    pub h_frozen_at: Option<String>,
}

/// `identity.infrastructure` — the per-binary digests plus the one
/// `set_digest` (§3.3's infrastructure-set digests member).
pub fn identity_infrastructure(sets: &CarriedSets) -> Result<Json, AdmissionError> {
    let _ = set_digest(INFRA_SET_NAME, &sets.infra_members)?;
    let mut m = BTreeMap::new();
    for (name, _) in INFRA_SET_MEMBERS {
        m.insert(
            name.to_string(),
            Json::obj([(
                "binary_sha256",
                Json::str(sets.infra_members.get(*name).unwrap()),
            )]),
        );
    }
    m.insert(
        "set_digest".to_string(),
        Json::str(&set_digest(INFRA_SET_NAME, &sets.infra_members)?),
    );
    Ok(Json::Obj(m))
}

/// `identity.h_set` — `{digest, frozen_at, hh_kernel, hir,
/// compiled_artifacts}`; the H-set digest frozen at G-L1.
pub fn identity_h_set(sets: &CarriedSets) -> Result<Json, AdmissionError> {
    let members = sets
        .h_members
        .as_ref()
        .ok_or_else(|| err("h_set", "no H set carried"))?;
    let digest = set_digest(H_SET_NAME, members)?;
    let frozen = sets
        .h_frozen_at
        .as_ref()
        .ok_or_else(|| err("h_set", "h_frozen_at required (frozen at G-L1)"))?;
    let member = |n: &str| Json::obj([("sha256", Json::str(members.get(n).unwrap()))]);
    Ok(Json::obj([
        ("digest", Json::str(&digest)),
        ("frozen_at", Json::str(frozen)),
        ("hh_kernel", member("hh_kernel")),
        ("hir", member("hir")),
        ("compiled_artifacts", member("compiled_artifacts")),
    ]))
}

/// The `identity` members a W4 slot inserts — `infrastructure` always;
/// `h_set` only on the H-carrying lanes.
pub fn slot_identity(
    sets: &CarriedSets,
    lane: ArmLane,
) -> Result<BTreeMap<String, Json>, AdmissionError> {
    let mut m = BTreeMap::new();
    m.insert("infrastructure".to_string(), identity_infrastructure(sets)?);
    if lane.carries_h_set() {
        m.insert("h_set".to_string(), identity_h_set(sets)?);
    }
    Ok(m)
}

/// `check_slot_identity(identity, sets, lane)` — the W4 predicate: one
/// `infrastructure.set_digest` equal to the pass's; on H-carrying lanes
/// `h_set.digest` equal to the G-L1-frozen set; a hosted lane carrying
/// `h_set` or an H lane missing it refuses.
pub fn check_slot_identity(
    identity: &Json,
    sets: &CarriedSets,
    lane: ArmLane,
) -> Result<(), AdmissionError> {
    let want_infra = set_digest(INFRA_SET_NAME, &sets.infra_members)?;
    let infra = identity
        .get("infrastructure")
        .and_then(Json::as_obj)
        .ok_or_else(|| {
            err(
                "slot_identity",
                "identity.infrastructure required on a W4 slot",
            )
        })?;
    match infra.get("set_digest").and_then(Json::as_str) {
        Some(s) if s == want_infra => {}
        Some(s) => {
            return Err(err(
                "slot_identity",
                format!("infrastructure set_digest {s} != carried {want_infra}"),
            ))
        }
        None => {
            return Err(err(
                "slot_identity",
                "identity.infrastructure.set_digest required (REQ-XC-330)",
            ))
        }
    }
    match (lane.carries_h_set(), identity.get("h_set")) {
        (true, None) => Err(err(
            "slot_identity",
            format!(
                "{} slot requires identity.h_set (frozen at G-L1)",
                lane.as_str()
            ),
        )),
        (true, Some(h)) => {
            let want = set_digest(
                H_SET_NAME,
                sets.h_members
                    .as_ref()
                    .ok_or_else(|| err("slot_identity", "H-carrying lane with no H set carried"))?,
            )?;
            match h.get("digest").and_then(Json::as_str) {
                Some(s) if s == want => Ok(()),
                Some(s) => Err(err(
                    "slot_identity",
                    format!("h_set digest {s} != the G-L1-frozen {want}"),
                )),
                None => Err(err("slot_identity", "identity.h_set.digest required")),
            }
        }
        (false, Some(_)) => Err(err(
            "slot_identity",
            format!(
                "{} lane carries no H set — identity.h_set refused",
                lane.as_str()
            ),
        )),
        (false, None) => Ok(()),
    }
}
