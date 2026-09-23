//! `VariantPackage` — the sealed plugin package on disk (§8.4 §3
//! `PluginManifest/1` + contributions; the `pin` half of V5).
//!
//! Layout (the package root is the fs extent the lowered policy grants):
//!
//! ```text
//! <root>/
//!   plugin.manifest.json   — the PluginManifest/1 document (canonical idp/1)
//!   bin/<exec>             — the executable each contribution pins
//!   …contribution paths…
//! ```
//!
//! `load` decodes the manifest strictly, resolves each `executable`
//! contribution's `path_or_locator` under the root (never escaping it —
//! `PathOrLocator` decode already refuses `..`), recomputes every
//! `code_pointer` content address and refuses `PinMismatch` on a miss.
//! `seal` is the packaging half — compute the pins and write the manifest
//! with them baked in (the packaged first-party variant ships sealed).

use std::path::{Path, PathBuf};

use hh_registry::extension::plugin::{
    manifest_from_json, ContributionKind, PathOrLocator, PluginManifest,
};
use hh_wire::json::Json;

use crate::HostError;

/// The manifest's package-relative path.
pub const MANIFEST_PATH: &str = "plugin.manifest.json";

/// A loaded, pin-verified package.
#[derive(Debug, Clone)]
pub struct VariantPackage {
    /// The package root.
    pub root: PathBuf,
    /// The decoded manifest.
    pub manifest: PluginManifest,
    /// The manifest's content address (`hello`'s `plugin.content`).
    pub content: String,
    /// The sealed `version_id` pin (`hello`'s `plugin.version_id` — the
    /// registry record's pin; defaults to the manifest content address,
    /// settable via [`VariantPackage::with_version_id`] when the sealed
    /// form's id differs).
    pub version_id: String,
    /// Each executable contribution's resolved binary path (in
    /// contribution order).
    pub execs: Vec<PathBuf>,
}

impl VariantPackage {
    /// The package's `version_id` pin (V5 — the hello assertion).
    pub fn version_id(&self) -> &str {
        &self.version_id
    }

    /// Set the sealed `version_id` (when the registry's pin differs from
    /// the manifest content address — e.g. the registered record's id).
    pub fn with_version_id(mut self, version_id: impl Into<String>) -> VariantPackage {
        self.version_id = version_id.into();
        self
    }

    /// The package content address (`hello`'s `plugin.content`).
    pub fn content(&self) -> &str {
        &self.content
    }

    /// The executable binary the variant host spawns (the first executable
    /// contribution — the variant entry point).
    pub fn exec_binary(&self) -> &Path {
        self.execs
            .first()
            .map(|p| p.as_path())
            .unwrap_or(&self.root)
    }

    /// Every executable path (`fs.exec.allow`).
    pub fn exec_paths(&self) -> Vec<String> {
        self.execs
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    }
}

/// `load(root)` — decode + pin-verify the sealed package.
pub fn load(root: &Path) -> Result<VariantPackage, HostError> {
    let manifest_path = root.join(MANIFEST_PATH);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| HostError::Package(format!("read {}: {e}", manifest_path.display())))?;
    let j = hh_wire::json::parse(&text)
        .map_err(|e| HostError::Package(format!("manifest parse: {e}")))?;
    let manifest = manifest_from_json(&j, MANIFEST_PATH)
        .map_err(|e| HostError::Package(format!("manifest decode: {e}")))?;
    let content = hh_identity::address(text.as_bytes(), "application/json").id();
    verify(root, &manifest, &content)
}

/// `verify(root, manifest, content)` — the pin check: every executable
/// contribution's `code_pointer` recomputes over the bytes on disk (V5 —
/// the kernel refuses a plugin whose `version_id`/content differs from the
/// sealed pin, and the package refuses bytes that don't match their pins).
pub fn verify(
    root: &Path,
    manifest: &PluginManifest,
    content: &str,
) -> Result<VariantPackage, HostError> {
    let mut execs = Vec::new();
    for c in &manifest.contributions {
        let Some(exe) = &c.executable else { continue };
        let path = match &c.path_or_locator {
            PathOrLocator::PackagePath(p) => root.join(p),
            PathOrLocator::PinnedLocator(l) => {
                return Err(HostError::Package(format!(
                    "contribution {} names a locator — sealed packages carry \
                     package paths (the locator resolves before seal)",
                    l.credential_free_uri
                )))
            }
        };
        let bytes = std::fs::read(&path)
            .map_err(|e| HostError::Package(format!("exec {} unreadable: {e}", path.display())))?;
        let actual = hh_identity::address(&bytes, "application/octet-stream");
        if actual != exe.code_pointer {
            return Err(HostError::Abi(
                hh_embed_schema::plugin_abi::AbiError::PinMismatch,
            ));
        }
        execs.push(path);
    }
    Ok(VariantPackage {
        root: root.to_path_buf(),
        manifest: manifest.clone(),
        content: content.to_string(),
        version_id: content.to_string(),
        execs,
    })
}

/// `seal(root, manifest)` — the packaging half: compute each executable
/// contribution's `code_pointer` from the bytes on disk and return the
/// manifest with pins baked in (the caller writes it back — sealing is a
/// principal act, never automatic).
pub fn seal(root: &Path, manifest: &PluginManifest) -> Result<PluginManifest, HostError> {
    let mut m = manifest.clone();
    for c in &mut m.contributions {
        let Some(exe) = &mut c.executable else {
            continue;
        };
        let PathOrLocator::PackagePath(p) = &c.path_or_locator else {
            continue;
        };
        let path = root.join(p);
        let bytes = std::fs::read(&path)
            .map_err(|e| HostError::Package(format!("exec {} unreadable: {e}", path.display())))?;
        exe.code_pointer = hh_identity::address(&bytes, "application/octet-stream");
    }
    Ok(m)
}

/// The variant contribution of a package (the class it binds).
pub fn variant_class(pkg: &VariantPackage) -> Option<String> {
    pkg.manifest
        .contributions
        .iter()
        .find(|c| c.kind == ContributionKind::Variant)
        .and_then(|c| {
            c.declaration
                .get("class_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
}

/// `materialize(src_root, exec_binary, dst_root)` — build a sealed package
/// on disk: copy the built executable under `bin/`, decode the template
/// manifest from `src_root`, seal the pins into it, write the sealed
/// manifest, then `load`+verify the result. The committed
/// `plugins/<name>/` tree is the *template* (`code_pointer` placeholder —
/// sealing is a principal act at package time); `materialize` is what the
/// suites and the packager run.
pub fn materialize(
    src_root: &Path,
    exec_binary: &Path,
    dst_root: &Path,
) -> Result<VariantPackage, HostError> {
    let manifest_path = src_root.join(MANIFEST_PATH);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| HostError::Package(format!("read {}: {e}", manifest_path.display())))?;
    let j = hh_wire::json::parse(&text)
        .map_err(|e| HostError::Package(format!("manifest parse: {e}")))?;
    let manifest = manifest_from_json(&j, MANIFEST_PATH)
        .map_err(|e| HostError::Package(format!("manifest decode: {e}")))?;
    // Copy every executable contribution under the package root.
    std::fs::create_dir_all(dst_root).map_err(|e| HostError::Io(e.to_string()))?;
    for c in &manifest.contributions {
        if c.executable.is_none() {
            continue;
        }
        let PathOrLocator::PackagePath(p) = &c.path_or_locator else {
            return Err(HostError::Package(format!(
                "{}: executable contributions must be package paths",
                c.kind.as_str()
            )));
        };
        let dst = dst_root.join(p);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| HostError::Io(e.to_string()))?;
        }
        std::fs::copy(exec_binary, &dst)
            .map_err(|e| HostError::Package(format!("copy {}: {e}", dst.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&dst)
                .map_err(|e| HostError::Io(e.to_string()))?
                .permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&dst, perm).map_err(|e| HostError::Io(e.to_string()))?;
        }
    }
    let sealed = seal(dst_root, &manifest)?;
    let sealed_json =
        hh_registry::extension::plugin::manifest_to_json(&sealed).to_canonical_string();
    std::fs::write(dst_root.join(MANIFEST_PATH), &sealed_json)
        .map_err(|e| HostError::Io(e.to_string()))?;
    load(dst_root)
}
