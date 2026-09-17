//! The `security.credential.*` / `security.secret.*` payload builders (§5g.3 §3).
//! Every payload is **content-free** — refs, revisions, modes, destinations,
//! closed codes and `provided: yes/no`; a secret value, placeholder body
//! fragment or mask byte never appears (SV-2). The classes are registered in
//! `hh-ledger::classes` (audit-grade, kernel-origin, provenance-mandatory).
//!
//! Member names are the spec's own (`bound{binding_id, env_handle, channel_id,
//! mode, expires_at}`, `used{binding_id, destination, effect_id, decision,
//! monitor_decision_ref}`, `denied{binding_id?, destination, reason}`,
//! `revoked{binding_id | channel_id, reason}`, `rotated{channel_id,
//! from_revision, to_revision}`, `redacted{target_ref, hits[]}`,
//! `leak_detected{location, hit, detector}`); the extra members the rows carry
//! (`holder`, `channel_revision`, `provided`, `binding_ids[]`, …) are ADR-0058
//! D4's audit fields — ids, revisions and closed enums, never content.

use hh_wire::json::Json;

use crate::errors::RefusedCode;
use crate::redact::{Leak, RedactionTombstone};
use crate::types::ProvidedSecret;

/// `security.credential.bound{binding_id, env_handle, channel_id, mode,
/// expires_at, holder, channel_revision, decision_ref}` — the PDP decision the
/// bind relied on is pinned by `decision_ref` (the `security.permission.
/// decided` event id; SV-8's audit trail).
#[allow(clippy::too_many_arguments)]
pub fn bound_payload(
    binding_id: &str,
    channel_id: &str,
    channel_revision: u64,
    holder: &str,
    env_handle: &str,
    mode: &str,
    decision_ref: &str,
    expires_at: &str,
) -> Json {
    Json::obj([
        ("binding_id", Json::str(binding_id)),
        ("env_handle", Json::str(env_handle)),
        ("channel_id", Json::str(channel_id)),
        ("mode", Json::str(mode)),
        ("expires_at", Json::str(expires_at)),
        ("holder", Json::str(holder)),
        ("channel_revision", Json::Int(channel_revision as i64)),
        ("decision_ref", Json::str(decision_ref)),
    ])
}

/// `security.credential.used{binding_id, destination, effect_id, decision,
/// monitor_decision_ref, channel_id, channel_revision, provided}` — every use
/// references an `effect_id` and the monitor decision it rode (SV-5/SV-8) and
/// records `provided: yes|no`, never the value (§5g.3 §4).
#[allow(clippy::too_many_arguments)]
pub fn used_payload(
    binding_id: &str,
    channel_id: &str,
    channel_revision: u64,
    destination: &str,
    effect_id: &str,
    decision: &str,
    monitor_decision_ref: Option<&str>,
    provided: ProvidedSecret,
) -> Json {
    Json::obj([
        ("binding_id", Json::str(binding_id)),
        ("destination", Json::str(destination)),
        ("effect_id", Json::str(effect_id)),
        ("decision", Json::str(decision)),
        (
            "monitor_decision_ref",
            monitor_decision_ref.map(Json::str).unwrap_or(Json::Null),
        ),
        ("channel_id", Json::str(channel_id)),
        ("channel_revision", Json::Int(channel_revision as i64)),
        ("provided", provided.to_json()),
    ])
}

/// `security.credential.denied{binding_id?, destination, reason, channel_id?,
/// effect_id?}` — an attempt denied, recorded *before* the refusal is visible
/// (ADR-0058 D6). `reason` is the closed `Refused.code` tag.
pub fn denied_payload(
    binding_id: Option<&str>,
    destination: &str,
    reason: RefusedCode,
    channel_id: Option<&str>,
    effect_id: Option<&str>,
) -> Json {
    Json::obj([
        (
            "binding_id",
            binding_id.map(Json::str).unwrap_or(Json::Null),
        ),
        ("destination", Json::str(destination)),
        ("reason", Json::str(reason.as_str())),
        (
            "channel_id",
            channel_id.map(Json::str).unwrap_or(Json::Null),
        ),
        ("effect_id", effect_id.map(Json::str).unwrap_or(Json::Null)),
    ])
}

/// `security.credential.revoked{binding_id | channel_id, reason,
/// binding_ids[]}` — a binding-level revoke carries `binding_id`, a
/// channel-level revoke `channel_id` plus `binding_ids[]` listing every
/// binding the revoke dropped (idempotent — an already-dead binding is not
/// re-listed).
pub fn revoked_payload(
    binding_id: Option<&str>,
    channel_id: Option<&str>,
    dropped: &[String],
    reason: &str,
) -> Json {
    Json::obj([
        (
            "binding_id",
            binding_id.map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "channel_id",
            channel_id.map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "binding_ids",
            Json::Arr(dropped.iter().map(|b| Json::str(b.clone())).collect()),
        ),
        ("reason", Json::str(reason)),
    ])
}

/// `security.credential.rotated{channel_id, from_revision, to_revision}` —
/// revisions only; the values never appear (the old value survives only in the
/// kernel-side mask set — SV-1).
pub fn rotated_payload(channel_id: &str, from_revision: u64, to_revision: u64) -> Json {
    Json::obj([
        ("channel_id", Json::str(channel_id)),
        ("from_revision", Json::Int(from_revision as i64)),
        ("to_revision", Json::Int(to_revision as i64)),
    ])
}

/// `security.secret.redacted{target_ref, hits[]}` — the at-source redaction
/// tombstone row (§5g.3 §3): `hits[]` are the tombstone summaries
/// (`{detector, fingerprint?, label}`), never the masked bytes.
pub fn redacted_payload(target_ref: &str, hits: &[RedactionTombstone]) -> Json {
    Json::obj([
        ("target_ref", Json::str(target_ref)),
        (
            "hits",
            Json::Arr(hits.iter().map(|h| h.to_json()).collect()),
        ),
    ])
}

/// `security.secret.leak_detected{location, hit, detector}` — the
/// test-battery's audit+triage row (ADR-0059 D2: a scan verdict, never a
/// live-path interceptor firing). `location` is the `leak_scan` target
/// (`event:<id>`, `blob:<addr>`, `request_view`, `definition:<id>`,
/// `sink_delivery:<sink>`, or a live-path tag like `mediate:<channel>` for a
/// canary attempt — ADR-0059 D3).
pub fn leak_detected_payload(leak: &Leak) -> Json {
    Json::obj([
        ("location", Json::str(leak.location_string())),
        (
            "hit",
            Json::obj([
                (
                    "fingerprint",
                    leak.fingerprint
                        .as_ref()
                        .map(|f| Json::str(f.clone()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "label",
                    leak.label
                        .as_ref()
                        .map(|l| Json::str(l.clone()))
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        ("detector", Json::str(leak.detector.as_str())),
    ])
}
