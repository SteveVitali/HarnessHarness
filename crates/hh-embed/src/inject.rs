//! Injection refusals at the schema (I7/I8/I9; spec §7.4 §2.4, §5).
//!
//! Three refusals the boundary applies *before* a params record reaches a
//! session:
//!
//! 1. **Runtime handles are unreachable from the contract schema** (I7 —
//!    `EnvHandleId`/`HandleId`/lease ids never appear as fields). A payload
//!    member *named* like a runtime handle in an open/submit position is a
//!    hostile reach around the boundary — `SchemaViolation{code:
//!    "runtime_handle_injection"}`.
//! 2. **Secrets never travel the payload plane** (I9/SV-1) — every
//!    canonical payload string is scanned by `hh-secrets`'s registered
//!    detectors; a hit is `SecretInPayload` (the secret is never echoed —
//!    the error names no bytes).
//! 3. **Override widening is an authority violation** — an `overrides[]`
//!    pointer that names a member outside the declared coordinate space
//!    (`/model`, `/mode`, `/thought_level`, `/profile`) is
//!    `AuthorityViolation{layer: "override"}` (ADR-0024 D6: the boundary
//!    refuses what the client's layer cannot widen).

use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::Override;
use hh_secrets::mask::MaskSet;
use hh_secrets::redact::{leak_scan, DetectorSet, ScanTarget};
use hh_wire::json::Json;

/// Payload member names that spell runtime-handle / credential reaches.
/// Contract fields are *ids of records* (`permission_id`, `effect_id`,
/// `session_id`, `capability_id`, `run_id`, …) — a member that names a
/// live runtime object is refused.
const HANDLE_KEYS: &[&str] = &[
    "env_handle_id",
    "handle_id",
    "tool_handle_id",
    "lease_secret",
    "credential",
    "api_key",
    "secret",
    "secret_value",
    "token",
    "private_key",
];

/// The coordinate prefixes an `overrides[]` pointer may name (the
/// definition's tunable surface). Everything else is widening.
const OVERRIDE_ALLOWED: &[&str] = &["/model", "/mode", "/thought_level", "/profile"];

/// Refuse runtime-handle member names anywhere in `v`
/// (`SchemaViolation{path, code:"runtime_handle_injection"}` — the path
/// names the member, never its value).
pub fn refuse_handle_keys(v: &Json, path: &str) -> Result<(), EmbedError> {
    match v {
        Json::Obj(m) => {
            for (k, val) in m {
                let p = format!("{path}/{k}");
                if HANDLE_KEYS.contains(&k.as_str()) {
                    return Err(EmbedError::SchemaViolation {
                        path: p,
                        code: "runtime_handle_injection".to_string(),
                    });
                }
                refuse_handle_keys(val, &p)?;
            }
            Ok(())
        }
        Json::Arr(a) => {
            for (i, val) in a.iter().enumerate() {
                refuse_handle_keys(val, &format!("{path}/{i}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Scan every canonical payload string with the registered secret
/// detectors — a hit is `SecretInPayload` (SV-1: the leak is named by
/// class, never echoed).
pub fn refuse_secrets(v: &Json) -> Result<(), EmbedError> {
    let detectors = DetectorSet::standard(MaskSet::default());
    let canonical = v.to_canonical_string();
    let leaks = leak_scan(&[(ScanTarget::RequestView, canonical.as_str())], &detectors);
    if leaks.is_empty() {
        Ok(())
    } else {
        Err(EmbedError::SecretInPayload)
    }
}

/// Refuse override pointers that reach outside the declared coordinate
/// space — `AuthorityViolation{layer:"override"}` (ADR-0024 D6).
pub fn refuse_override_widening(overrides: &[Override]) -> Result<(), EmbedError> {
    for o in overrides {
        let ok = OVERRIDE_ALLOWED
            .iter()
            .any(|p| o.pointer == *p || o.pointer.starts_with(&format!("{p}/")));
        if !ok {
            return Err(EmbedError::AuthorityViolation {
                layer: "override".to_string(),
                detail: format!(
                    "override pointer {} names a member outside the tunable coordinate space",
                    o.pointer
                ),
            });
        }
    }
    Ok(())
}
