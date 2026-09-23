//! `encode` / `decode` — two transports, one identity (§5h.3 §2; ADR-0139
//! D2; AC-R-2.9.3-1/2):
//!
//! - **directory** — `bundle.json` (the canonical manifest document,
//!   including `version_id`) plus `members/<domain>-<hex>` payload files;
//! - **`HHB1` container** — a deterministic single-file framing:
//!   `HHB1\n` + canonical `{manifest, members: {address: hex(bytes)}}`.
//!   The container is transport, never identity — the bundle id is the
//!   tree rule over the manifest regardless of which form it rode in.
//!
//! `decode` re-derives the manifest's `version_id` preimage check is
//! validation's job (`version_id_mismatch`); `decode` itself only
//! guarantees the structural round-trip.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hh_wire::json::Json;

use crate::error::BundleError;
use crate::export::MemberBytes;
use crate::manifest::BundleManifest;

/// The container magic + version.
pub const CONTAINER_MAGIC: &[u8] = b"HHB1\n";
/// The manifest sidecar name (the bundle's root).
pub const MANIFEST_NAME: &str = "bundle.json";
/// The member directory.
pub const MEMBERS_DIR: &str = "members";

/// `<address> → <domain>-<hex>` — the filesystem-safe member name.
pub fn member_filename(address: &str) -> String {
    address.replace(':', "-")
}
/// `<domain>-<hex> → <address>`; `None` when the name isn't ours.
pub fn member_address(filename: &str) -> Option<String> {
    if filename.contains('-') {
        Some(filename.replacen('-', ":", 1))
    } else {
        None
    }
}

/// A decoded bundle: manifest + whatever member bytes the form carried.
pub struct Decoded {
    /// The manifest.
    pub manifest: BundleManifest,
    /// `address → bytes` for every member the form carried.
    pub members: MemberBytes,
}

/// `encode(dir)` — write `bundle.json` + `members/*`.
pub fn encode_dir(
    dir: &Path,
    manifest: &BundleManifest,
    members: &MemberBytes,
) -> Result<(), BundleError> {
    let io = |e: std::io::Error| BundleError::Io {
        detail: e.to_string(),
    };
    std::fs::create_dir_all(dir.join(MEMBERS_DIR)).map_err(io)?;
    std::fs::write(
        dir.join(MANIFEST_NAME),
        manifest.to_json().to_canonical_string(),
    )
    .map_err(io)?;
    for (addr, bytes) in members {
        std::fs::write(dir.join(MEMBERS_DIR).join(member_filename(addr)), bytes).map_err(io)?;
    }
    Ok(())
}

/// `decode(dir)`.
pub fn decode_dir(dir: &Path) -> Result<Decoded, BundleError> {
    let io = |e: std::io::Error| BundleError::Io {
        detail: format!("{}: {e}", dir.display()),
    };
    let text = std::fs::read_to_string(dir.join(MANIFEST_NAME)).map_err(io)?;
    let doc = hh_wire::json::parse(&text).map_err(|e| BundleError::Malformed {
        detail: format!("{MANIFEST_NAME}: {e}"),
    })?;
    let manifest = BundleManifest::from_json(&doc)?;
    let mut members = MemberBytes::new();
    let mdir = dir.join(MEMBERS_DIR);
    if mdir.is_dir() {
        for entry in std::fs::read_dir(&mdir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(addr) = member_address(&name) {
                members.insert(addr, std::fs::read(entry.path()).map_err(io)?);
            }
        }
    }
    Ok(Decoded { manifest, members })
}

/// `encode(HHB1)` — the deterministic container.
pub fn encode_container(manifest: &BundleManifest, members: &MemberBytes) -> Vec<u8> {
    let hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
    let member_map: BTreeMap<String, Json> = members
        .iter()
        .map(|(a, b)| (a.clone(), Json::str(hex(b))))
        .collect();
    let doc = Json::obj([
        ("manifest", manifest.to_json()),
        ("members", Json::Obj(member_map)),
    ]);
    let mut out = CONTAINER_MAGIC.to_vec();
    out.extend_from_slice(doc.to_canonical_string().as_bytes());
    out
}

/// `decode(HHB1)` — `FormatUnknown` when the magic is absent.
pub fn decode_container(bytes: &[u8]) -> Result<Decoded, BundleError> {
    if !bytes.starts_with(CONTAINER_MAGIC) {
        return Err(BundleError::FormatUnknown {
            detail: "not an HHB1 container".into(),
        });
    }
    let text = std::str::from_utf8(&bytes[CONTAINER_MAGIC.len()..]).map_err(|_| {
        BundleError::Malformed {
            detail: "HHB1 body is not utf-8".into(),
        }
    })?;
    let doc = hh_wire::json::parse(text).map_err(|e| BundleError::Malformed {
        detail: format!("HHB1 body: {e}"),
    })?;
    let manifest =
        BundleManifest::from_json(doc.get("manifest").ok_or_else(|| BundleError::Malformed {
            detail: "HHB1 body has no manifest".into(),
        })?)?;
    let unhex = |s: &str| -> Result<Vec<u8>, BundleError> {
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| BundleError::Malformed {
                    detail: "HHB1 member hex".into(),
                })
            })
            .collect()
    };
    let mut members = MemberBytes::new();
    if let Some(Json::Obj(ms)) = doc.get("members") {
        for (addr, hex) in ms {
            let s = hex.as_str().ok_or_else(|| BundleError::Malformed {
                detail: "HHB1 member is not a hex string".into(),
            })?;
            members.insert(addr.clone(), unhex(s)?);
        }
    }
    Ok(Decoded { manifest, members })
}

/// `decode` — dispatch on the input: a directory, an `HHB1` byte string,
/// or `FormatUnknown`.
pub fn decode(path: &Path) -> Result<Decoded, BundleError> {
    if path.is_dir() {
        return decode_dir(path);
    }
    let bytes = std::fs::read(path).map_err(|e| BundleError::Io {
        detail: format!("{}: {e}", path.display()),
    })?;
    decode_container(&bytes)
}

/// The default member path for `bundle.json` discovery inside a decoded
/// tree (import reads either form through `decode`).
pub fn default_bundle_path(root: &Path) -> PathBuf {
    root.join(MANIFEST_NAME)
}
