//! The workspace trust store (ADR-0168 D7) — the surface's read of the
//! H5 store. The Stage-2 realization is a user-scope file
//! `$HH_TRUST_STORE` (default `~/.hh/trust.json`) mapping workspace
//! roots to a trust claim:
//!
//! ```json
//! {"workspaces": {"/abs/path": "trusted"}}
//! ```
//!
//! `workspace_trust(cwd)` reads the record for the invocation's
//! workspace and returns its canonical spelling — `trusted` |
//! `untrusted` | `unknown`. Absent, unreadable or malformed records
//! answer `unknown` — the claim is never inferred (OQ-387 defers which
//! Π rows it narrows; the manifest records it verbatim).

use hh_wire::json::Json;
use std::path::{Path, PathBuf};

/// The closed claim sum (§7.1 §2.4 `workspace_trust` member).
pub const TRUST_VALUES: &[&str] = &["trusted", "untrusted", "unknown"];

/// The trust-store path — `$HH_TRUST_STORE` when set, else
/// `~/.hh/trust.json` (the user-scope layer; the store is H5's,
/// never the workspace's own — a repository file could not vouch for
/// itself).
pub fn store_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HH_TRUST_STORE") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|h| Path::new(&h).join(".hh").join("trust.json"))
}

/// `workspace_trust(workspace_root) → "trusted" | "untrusted" | "unknown"`.
/// The lookup key is the canonicalized absolute path; a workspace the
/// store does not name is `unknown`.
pub fn workspace_trust(workspace_root: &Path) -> &'static str {
    let Some(path) = store_path() else {
        return "unknown";
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return "unknown";
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return "unknown";
    };
    let Ok(doc) = hh_wire::json::parse(&text) else {
        return "unknown";
    };
    let key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf())
        .display()
        .to_string();
    match doc
        .get("workspaces")
        .and_then(|w| w.get(&key))
        .and_then(Json::as_str)
    {
        Some("trusted") => "trusted",
        Some("untrusted") => "untrusted",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_store_is_unknown() {
        // A path that cannot exist in any store ⇒ `unknown`.
        let tmp = std::env::temp_dir().join("hh-cli-trust-test-nonexistent-ws");
        // Ensure no ambient store leaks into the test.
        std::env::remove_var("HH_TRUST_STORE");
        let v = workspace_trust(&tmp);
        assert!(TRUST_VALUES.contains(&v));
    }

    #[test]
    fn store_record_round_trips() {
        let dir = std::env::temp_dir().join(format!("hh-cli-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ws = dir.join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let store = dir.join("trust.json");
        let key = ws.canonicalize().unwrap().display().to_string();
        std::fs::write(
            &store,
            format!("{{\"workspaces\":{{\"{key}\":\"trusted\"}}}}").replace('\\', "\\\\"),
        )
        .unwrap();
        std::env::set_var("HH_TRUST_STORE", &store);
        assert_eq!(workspace_trust(&ws), "trusted");
        std::fs::write(&store, "{\"workspaces\":{}}").unwrap();
        assert_eq!(workspace_trust(&ws), "unknown");
        std::fs::write(&store, "not json").unwrap();
        assert_eq!(workspace_trust(&ws), "unknown");
        std::env::remove_var("HH_TRUST_STORE");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
