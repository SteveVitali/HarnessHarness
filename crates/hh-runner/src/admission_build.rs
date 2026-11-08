//! The §5.2 two-stage prebuilt layer machinery (REQ-XC-303/304/305;
//! REQ-XC-330/327/322/329 contributing; AR-18/ADR-0436): the bundle
//! recipe renderer derived from the pinned Harbor commit's `install()`
//! with attribution and listed deviations; the per-(harness × task
//! image) layer Containerfile; the `podman build` host act with digest
//! and package-delta capture; and the ST2 verify-only setup whose
//! mismatch is `withheld{admission:harness_layer_mismatch}` through the
//! closed set (X-04). No build executes here — W1/R6.32's live leg does.

use std::collections::BTreeMap;
use std::path::Path;

use hharness_wire::json::Json;

use crate::json_ext::JsonExt;
use hharness_wire::sha256::sha256_hex;

use crate::admission::{image_ref_by_digest, AdmissionError};
use crate::cell::CellExec;
use crate::disposition::AdmissionReason;

/// The bundle install root inside the layer (design/34 §1.2).
pub const BUNDLE_ROOT: &str = "\u{2f}opt/hh-harness";
/// The build record kind — one per recorded host act.
pub const BUILD_RECORD_SCHEMA: &str = "hh-harness-build/1";
/// The package-delta manifest kind (REQ-XC-304).
pub const DELTA_SCHEMA: &str = "hh-package-delta/1";

fn err(rule: &'static str, detail: impl Into<String>) -> AdmissionError {
    AdmissionError {
        rule,
        detail: detail.into(),
    }
}

// ── the §5.8 build-failure disposition ────────────────────────────────────

/// A build step's `n/a{install:<step>}` spelling (§5.8 row 1 — a fixed
/// recipe is a new admission, never a retry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStep {
    /// `bundle` — the (a) bundle build failed.
    Bundle,
    /// `layer` — the (b) layer build failed.
    Layer,
    /// `delta_manifest` — the package-delta capture failed.
    DeltaManifest,
    /// `digest_capture` — the iidfile/digest read failed.
    DigestCapture,
}

impl InstallStep {
    /// The canonical `<step>` member.
    pub fn as_str(self) -> &'static str {
        match self {
            InstallStep::Bundle => "bundle",
            InstallStep::Layer => "layer",
            InstallStep::DeltaManifest => "delta_manifest",
            InstallStep::DigestCapture => "digest_capture",
        }
    }
    /// `n/a{install:<step>}` — the §5.8 spelling.
    pub fn na(self) -> String {
        format!("n/a{{install:{}}}", self.as_str())
    }
}

/// A failed build step — carries its §5.8 spelling.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallFailure {
    /// The step that failed.
    pub step: InstallStep,
    /// The verbatim detail.
    pub detail: String,
}

impl InstallFailure {
    /// `n/a{install:<step>}` — the §5.8 outcome spelling.
    pub fn na(&self) -> String {
        self.step.na()
    }
}

impl std::fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.na(), self.detail)
    }
}
impl std::error::Error for InstallFailure {}

// ── stage (a): the bundle recipe (REQ-XC-303) ─────────────────────────────

/// A lock/resolution-cutoff leg (REQ-XC-302's "missing lock or
/// resolution cutoff" refusal rides the admission validator).
#[derive(Debug, Clone, PartialEq)]
pub enum LockSpec {
    /// `UV_EXCLUDE_NEWER=<cutoff>` — uv's resolution cutoff.
    UvExcludeNewer(String),
    /// an npm lockfile reference (the lock bytes ride the bundle log).
    NpmLockfile(String),
    /// the Q-X26 binary path — version plus binary sha256.
    BinarySha256(String),
}

impl LockSpec {
    /// The rendered recipe lines.
    fn lines(&self) -> Vec<String> {
        match self {
            LockSpec::UvExcludeNewer(cut) => {
                vec![format!("ENV UV_EXCLUDE_NEWER={cut}")]
            }
            LockSpec::NpmLockfile(lock) => {
                vec![format!("COPY {lock} package-lock.json")]
            }
            LockSpec::BinarySha256(sha) => {
                vec![format!("ENV HH_BINARY_SHA256={sha}")]
            }
        }
    }
}

/// The (a) bundle recipe — HH-owned text derived from the pinned Harbor
/// commit's `install()` with attribution and listed deviations.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleRecipe {
    /// The harness name (`install` lands at `$HH_BUNDLE_ROOT`).
    pub harness: String,
    /// The pinned harness version.
    pub version: String,
    /// The Harbor adapter (`<-a name>`) the recipe derives from.
    pub harbor_agent: String,
    /// The pinned Harbor commit (`harbor::HARBOR_PIN`).
    pub harbor_commit: String,
    /// The declared builder base (`name@sha256:<64>`).
    pub builder_base: String,
    /// The lock/resolution-cutoff leg.
    pub lock: LockSpec,
    /// The recipe steps (each a `RUN` line), versions pinned.
    pub steps: Vec<String>,
    /// The recorded deviations `{what — why}` (verbatim lines).
    pub deviations: Vec<String>,
}

impl BundleRecipe {
    /// The rendered Containerfile text — deterministic, golden-pinned.
    pub fn render(&self) -> String {
        let root = format!("{BUNDLE_ROOT}/{}-{}", self.harness, self.version);
        let mut out = String::new();
        out.push_str("# hh-harness-bundle/1 — HH-owned recipe (ADMIT-CORE a; AR-18/ADR-0436)\n");
        out.push_str(&format!(
            "# derived from harbor@{} installed/{}.py::install()\n",
            self.harbor_commit, self.harbor_agent
        ));
        out.push_str(&format!(
            "# attribution: Harbor install templates (Apache-2.0) at {}\n",
            self.harbor_commit
        ));
        out.push_str(&format!("# deviations ({}):\n", self.deviations.len()));
        for d in &self.deviations {
            out.push_str(&format!("#   - {d}\n"));
        }
        out.push_str(&format!("FROM {}\n", self.builder_base));
        out.push_str(&format!("ENV HH_BUNDLE_ROOT={root}\n"));
        for l in self.lock.lines() {
            out.push_str(&l);
            out.push('\n');
        }
        out.push_str(&format!("RUN mkdir -p {root}\n"));
        for s in &self.steps {
            out.push_str(&format!("RUN {s}\n"));
        }
        out
    }
}

// ── stage (b): the per-(harness × task image) layer (REQ-XC-304) ──────────

/// The (b) layer spec — `FROM` the task image digest, replay the
/// adapter's system-dependency step, place the bundle at the
/// adapter-expected paths, write the version marker.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerSpec {
    /// The task image ref (`name@sha256:<64>`) — digest only.
    pub task_image: String,
    /// The harness name.
    pub harness: String,
    /// The pinned harness version.
    pub version: String,
    /// The adapter's `ensure_system_dependencies` replay lines.
    pub system_deps: Vec<String>,
    /// The Harbor-expected install paths the layer links into place.
    pub adapter_paths: Vec<String>,
}

impl LayerSpec {
    /// The rendered Containerfile — `FROM` by digest only; the REQ-XC-204
    /// alias predicate (O and H share the image; no install-time network
    /// at run) holds by construction.
    pub fn render(&self) -> Result<String, AdmissionError> {
        if !image_ref_by_digest(&self.task_image) {
            return Err(err(
                "layer_digest",
                format!("task image {:?} is not digest-pinned", self.task_image),
            ));
        }
        let root = format!("{BUNDLE_ROOT}/{}-{}", self.harness, self.version);
        let mut out = String::new();
        out.push_str("# hh-harness-layer/1 — per (harness × task image) layer (AR-18/ADR-0436)\n");
        out.push_str(&format!("FROM {}\n", self.task_image));
        for s in &self.system_deps {
            out.push_str(&format!("RUN {s}\n"));
        }
        out.push_str(&format!("COPY bundle/ {root}/\n"));
        out.push_str(&format!(
            "RUN printf '%s\\n' '{}' > {root}/.hh-version\n",
            self.version
        ));
        for p in &self.adapter_paths {
            out.push_str(&format!(
                "RUN mkdir -p \"$(dirname '{p}')\" && ln -sfn '{root}' '{p}'\n"
            ));
        }
        Ok(out)
    }
}

// ── the recorded host act over `podman build` ─────────────────────────────

/// One build plan — argv is derived once; the stub `podman` on `PATH`
/// pins it in the battery (the r5_56 pattern).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    /// `bundle | layer`.
    pub kind: InstallStep,
    /// The rendered Containerfile's host path (declared, packet-relative).
    pub containerfile: String,
    /// The build context dir.
    pub context: String,
    /// The local tag the build records (`hh-admit/<h>-<v>`).
    pub tag: String,
    /// The `--iidfile` the digest capture reads.
    pub iidfile: String,
}

impl BuildPlan {
    /// `podman build -f <cf> --iidfile <iid> -t <tag> <ctx>` — the only
    /// argv the host act emits.
    pub fn argv(&self) -> Vec<String> {
        vec![
            "build".to_string(),
            "-f".to_string(),
            self.containerfile.clone(),
            "--iidfile".to_string(),
            self.iidfile.clone(),
            "-t".to_string(),
            self.tag.clone(),
            self.context.clone(),
        ]
    }
}

/// The recorded build act — `{schema, kind, argv, tag, digest,
/// delta_manifest_ref?, log_ref}` — a host act like the image pull,
/// never an install-time cell fact.
pub fn build_record(
    plan: &BuildPlan,
    digest: &str,
    delta_ref: Option<&str>,
    log_ref: &str,
) -> Json {
    let mut m = vec![
        ("schema", Json::str(BUILD_RECORD_SCHEMA)),
        ("kind", Json::str(plan.kind.as_str())),
        (
            "argv",
            Json::Arr(plan.argv().iter().map(Json::str).collect()),
        ),
        ("tag", Json::str(&plan.tag)),
        ("digest", Json::str(digest)),
        ("log_ref", Json::str(log_ref)),
    ];
    if let Some(d) = delta_ref {
        m.push(("delta_manifest_ref", Json::str(d)));
    }
    Json::obj(m)
}

/// `run_build(plan, log_dir)` — the host act: `podman build` under the
/// caller's PATH (the battery's stub writes argv to `$HH_PODMAN_LOG` and
/// a digest to the iidfile); the build log records argv+rc; a non-zero
/// exit is `n/a{install:<step>}` — never a retry.
pub fn run_build(plan: &BuildPlan, log_path: &Path) -> Result<Json, InstallFailure> {
    let argv = plan.argv();
    let out = std::process::Command::new("podman")
        .args(&argv)
        .output()
        .map_err(|e| InstallFailure {
            step: plan.kind,
            detail: format!("podman build spawn: {e}"),
        })?;
    let mut log = format!("podman {}\nexit={}\n", argv.join(" "), out.status);
    log.push_str(&String::from_utf8_lossy(&out.stdout));
    let _ = std::fs::write(log_path, &log);
    if !out.status.success() {
        return Err(InstallFailure {
            step: plan.kind,
            detail: format!("podman build rc={}", out.status.code().unwrap_or(-1)),
        });
    }
    let iid = std::fs::read_to_string(&plan.iidfile).map_err(|e| InstallFailure {
        step: InstallStep::DigestCapture,
        detail: format!("iidfile {}: {e}", plan.iidfile),
    })?;
    let digest = iid.trim().to_string();
    if digest.is_empty() {
        return Err(InstallFailure {
            step: InstallStep::DigestCapture,
            detail: "iidfile empty".into(),
        });
    }
    Ok(build_record(
        plan,
        &digest,
        None,
        &format!("sha256:{}", sha256_hex(log.as_bytes())),
    ))
}

/// The package-delta probe argv (REQ-XC-304): the adapter's
/// system-dependency replay is recorded as a dpkg/apk delta manifest —
/// `podman run --rm <image> sh -c <list>`.
pub fn package_delta_argv(image: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--rm".to_string(),
        image.to_string(),
        "sh".to_string(),
        "-c".to_string(),
        "if command -v dpkg-query >/dev/null 2>&1; then dpkg-query -W -f='${Package}=${Version}\\n'; else apk info -vv | sort; fi".to_string(),
    ]
}

/// `parse_package_delta(stdout, image)` — the `name=version` rows into
/// the `hh-package-delta/1` manifest (every package@version, REQ-XC-303's
/// manifest rule).
pub fn parse_package_delta(stdout: &str, image: &str) -> Result<Json, InstallFailure> {
    let mut packages = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once('=') {
            Some((n, v)) if !n.is_empty() && !v.is_empty() => packages.push(Json::obj([
                ("name", Json::str(n.trim())),
                ("version", Json::str(v.trim())),
            ])),
            _ => {
                return Err(InstallFailure {
                    step: InstallStep::DeltaManifest,
                    detail: format!("delta row {line:?} is not name=version"),
                })
            }
        }
    }
    if packages.is_empty() {
        return Err(InstallFailure {
            step: InstallStep::DeltaManifest,
            detail: "no packages captured".into(),
        });
    }
    Ok(Json::obj([
        ("schema", Json::str(DELTA_SCHEMA)),
        ("image", Json::str(image)),
        ("packages", Json::Arr(packages)),
    ]))
}

/// The manifest's content address — `sha256:<hex>` over canonical bytes.
pub fn manifest_ref(manifest: &Json) -> String {
    format!(
        "sha256:{}",
        sha256_hex(manifest.to_canonical_string().as_bytes())
    )
}

/// `check_cell_image_ref(image)` — cells run the layer by digest only
/// (REQ-XC-304): a `name:tag`, a bare name or a bare `sha256:` without
/// the name part all refuse.
pub fn check_cell_image_ref(image: &str) -> Result<(), AdmissionError> {
    if image_ref_by_digest(image) {
        Ok(())
    } else {
        Err(err(
            "cell_image_digest",
            format!("cell image {image:?} refused — layers run by digest only (name@sha256:<64>)"),
        ))
    }
}

/// `layer_for_task(rec, task_image)` — the recorded layer digest for one
/// task image; a task with no built layer refuses (never built).
pub fn layer_for_task(rec: &Json, task_image: &str) -> Result<String, AdmissionError> {
    let layers = rec
        .get("install")
        .and_then(|i| i.get("layers"))
        .and_then(Json::as_obj)
        .ok_or_else(|| err("layer_digest", "install.layers missing"))?;
    layers
        .get(task_image)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            err(
                "layer_digest",
                format!("no layer built for {task_image} — a build is a host act, never a slot-time install"),
            )
        })
}

/// `build_once_guard(rec, task_image)` — REQ-XC-330 contributing: a
/// layer exists at most once per pass for the same (harness, version,
/// lock, task image); a second build is a new admission, never a silent
/// rebuild.
pub fn build_once_guard(rec: &Json, task_image: &str) -> Result<(), AdmissionError> {
    match layer_for_task(rec, task_image) {
        Ok(existing) => Err(err(
            "already_built",
            format!("a layer already exists for {task_image} ({existing}) — a rebuild is a new admission (REQ-XC-330)"),
        )),
        Err(_) => Ok(()),
    }
}

/// `artifact_upload(posture)` — REQ-XC-330 contributing: a bundle or
/// layer containing a proprietary/source-available harness is
/// `digest_only` — its bytes stay on the operator controller, never
/// uploaded to the archive.
pub fn artifact_upload(posture: &str) -> &'static str {
    match posture {
        "proprietary" | "source_available" => "digest_only",
        _ => "archived",
    }
}

// ── ST2 — the verify-only setup (REQ-XC-305) ──────────────────────────────

/// The ST2 probe plan — version probe, bundle tree hash, rendered-config
/// hash; plus the Q-X26 binary sha256 probe where declared. No install,
/// no network — the exec set is closed.
#[derive(Debug, Clone, PartialEq)]
pub struct St2Plan {
    /// The probe execs in order.
    pub probes: Vec<CellExec>,
}

fn probe(argv: Vec<String>) -> CellExec {
    CellExec {
        argv,
        cwd: "\u{2f}".to_string(),
        deadline_ms: Some(30_000),
    }
}

/// The tree-hash recipe — `find <tree>` sorted, per-file sha256 folded
/// to one digest == `bundle.digest` (design/34 §1.2 S2).
pub fn tree_hash_argv(root: &str) -> Vec<String> {
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("find '{root}' -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1"),
    ]
}

/// `st2_plan(rec)` — the verify-only setup plan: version probe argv from
/// the admission, the bundle tree hash over `install_tree_paths[0]`,
/// the rendered-config hash over `config.rendered_paths`, and the Q-X26
/// `binary_sha256` probe when declared.
pub fn st2_plan(rec: &Json) -> Result<St2Plan, AdmissionError> {
    let i = rec
        .get("install")
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", "install required"))?;
    let mut probes = Vec::new();
    let vp = i
        .get("version_probe")
        .and_then(Json::as_arr)
        .ok_or_else(|| err("member_missing", "install.version_probe required"))?;
    probes.push(probe(
        vp.iter()
            .map(|x| x.as_str().unwrap_or("").to_string())
            .collect(),
    ));
    let tree = i
        .get("install_tree_paths")
        .and_then(Json::as_arr)
        .and_then(|a| a.first())
        .and_then(Json::as_str)
        .ok_or_else(|| err("member_missing", "install_tree_paths[0] required"))?;
    probes.push(probe(tree_hash_argv(tree)));
    let paths: Vec<String> = rec
        .get("config")
        .and_then(|c| c.get("rendered_paths"))
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .ok_or_else(|| err("member_missing", "config.rendered_paths required"))?;
    probes.push(probe(vec![
        "sh".to_string(),
        "-c".to_string(),
        format!(
            "sha256sum {} | sha256sum | cut -d' ' -f1",
            paths
                .iter()
                .map(|p| format!("'{p}'"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ]));
    if let Some(sha) = rec
        .get("harness")
        .and_then(|h| h.get("legal"))
        .and_then(|l| l.get("qx26"))
        .and_then(|q| q.get("binary_sha256"))
        .and_then(Json::as_str)
    {
        let _ = sha;
        let bin = format!("{BUNDLE_ROOT}/bin");
        probes.push(probe(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("find '{bin}' -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1"),
        ]));
    }
    Ok(St2Plan { probes })
}

/// Network verbs ST2 never runs — the verify plan is closed: probe execs
/// only (no install, fetch or image act).
pub fn st2_offline(plan: &St2Plan) -> bool {
    const BANNED: [&str; 16] = [
        "podman", "docker", "curl", "wget", "apt", "apt-get", "apk", "npm", "npx", "uv", "pip",
        "pip3", "git", "ssh", "nc", "ncat",
    ];
    plan.probes.iter().all(|p| {
        p.argv
            .iter()
            .all(|a| !BANNED.contains(&a.as_str()) && !a.starts_with("http"))
    })
}

/// What the ST2 probes observed in the cell.
#[derive(Debug, Clone, PartialEq)]
pub struct St2Observation {
    /// The version-probe stdout.
    pub version_stdout: String,
    /// The bundle tree hash (`sha256:<64>`).
    pub bundle_tree_sha256: String,
    /// The rendered-config hash (`sha256:<64>`).
    pub config_sha256: String,
    /// The binary sha256 probe (Q-X26 records only).
    pub binary_sha256: Option<String>,
}

/// `st2_verify(rec, obs)` — REQ-XC-305: every member must equal the
/// admission's declared value; any mismatch is
/// `AdmissionReason::HarnessLayerMismatch` (the §5.8 row —
/// `withheld{admission:harness_layer_mismatch}`, arm halted, no retry).
pub fn st2_verify(rec: &Json, obs: &St2Observation) -> Result<(), AdmissionReason> {
    let bad = || AdmissionReason::HarnessLayerMismatch;
    let version = rec
        .get("harness")
        .and_then(|h| h.get("version"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if !obs.version_stdout.contains(version) {
        return Err(bad());
    }
    let digest = rec
        .get("install")
        .and_then(|i| i.get("bundle"))
        .and_then(|b| b.get("digest"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if obs.bundle_tree_sha256 != digest {
        return Err(bad());
    }
    let thash = rec
        .get("config")
        .and_then(|c| c.get("template_hash"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if obs.config_sha256 != thash {
        return Err(bad());
    }
    if let Some(want) = rec
        .get("harness")
        .and_then(|h| h.get("legal"))
        .and_then(|l| l.get("qx26"))
        .and_then(|q| q.get("binary_sha256"))
        .and_then(Json::as_str)
    {
        if obs.binary_sha256.as_deref() != Some(want) {
            return Err(bad());
        }
    }
    Ok(())
}

/// The BTreeMap helper the fixtures use to keep member order canonical.
pub fn obj_map(pairs: Vec<(&str, Json)>) -> BTreeMap<String, Json> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}
