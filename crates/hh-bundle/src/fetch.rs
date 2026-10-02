//! `fetch(bundle, addresses[], locations?) → {materialized[], failed[]}`
//! — resolves `fetch[]` entries through a caller-supplied fetcher
//! (§5h.3 §2; S4.2). Rules the contract fixes: the fetched bytes are
//! verified on arrival (S3 — the address *is* the digest; a location's
//! claim is never trusted), fetched bytes are never executed, and the
//! manifest is never mutated — the caller materializes the bytes into
//! its own pool. OQ-332 (location trust/expiry semantics) stays
//! DEFERRED (ADR-0210); what lands here is the deterministic core.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::export::MemberBytes;
use crate::manifest::MemberStatus;

/// The `fetch` outcome — `{materialized[], failed[]}` (the operation's
/// own record; refusals land per-address, never panic).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FetchOutcome {
    /// Addresses materialized + verified (`address → bytes` are handed to
    /// the caller for deposit).
    pub materialized: Vec<String>,
    /// `{address, reason}` per failure (`not_fetchable`, `no_fetcher`,
    /// `digest_mismatch`, `fetch_error`).
    pub failed: Vec<Json>,
    /// The verified bytes per materialized address.
    pub bytes: MemberBytes,
}

impl FetchOutcome {
    /// Canonical JSON (the boundary result).
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "materialized",
                Json::Arr(self.materialized.iter().map(Json::str).collect()),
            ),
            ("failed", Json::Arr(self.failed.clone())),
        ])
    }
}

/// `fetch(decoded, addresses, fetcher)` — for each requested address:
/// the manifest must list the member with `status = fetch` (a `present`
/// member needs no fetch; `redacted`/`gc`/unknown fail `not_fetchable`),
/// a `fetch[]` entry must exist, and the fetcher's returned bytes must
/// recompute to the address (`digest_mismatch` — the location's claim is
/// never trusted).
/// The fetch seam: `FetchEntry → bytes`. `None` in a store with no fetch
/// mechanism — every request fails `no_fetcher`, never fabricates.
pub type Fetcher<'a> = dyn Fn(&crate::manifest::FetchEntry) -> Result<Vec<u8>, String> + 'a;

pub fn fetch(
    decoded: &Decoded,
    addresses: &[String],
    fetcher: Option<&Fetcher<'_>>,
) -> FetchOutcome {
    let m = &decoded.manifest;
    let mut out = FetchOutcome::default();
    for addr in addresses {
        let fail = |out: &mut FetchOutcome, reason: &str| {
            out.failed.push(Json::obj([
                ("address", Json::str(addr.clone())),
                ("reason", Json::str(reason)),
            ]));
        };
        let member = match m.members.iter().find(|mm| mm.address == *addr) {
            Some(mm) => mm,
            None => {
                fail(&mut out, "unknown_member");
                continue;
            }
        };
        match member.status {
            MemberStatus::Fetch => {}
            MemberStatus::Present => {
                fail(&mut out, "not_fetchable:member_is_present");
                continue;
            }
            other => {
                fail(&mut out, &format!("not_fetchable:{}", other.name()));
                continue;
            }
        }
        let entry = match m.fetch.iter().find(|f| f.address == *addr) {
            Some(f) => f,
            None => {
                fail(&mut out, "not_fetchable:no_fetch_entry");
                continue;
            }
        };
        let Some(fetcher) = fetcher else {
            fail(&mut out, "no_fetcher");
            continue;
        };
        match fetcher(entry) {
            Ok(bytes) => {
                let recomputed = hh_identity::idp_id("blob", &bytes);
                if recomputed != *addr {
                    fail(&mut out, "digest_mismatch");
                } else {
                    out.materialized.push(addr.clone());
                    out.bytes.insert(addr.clone(), bytes);
                }
            }
            Err(e) => fail(&mut out, &format!("fetch_error:{e}")),
        }
    }
    out
}

/// The `fetch` refusal for a member read — a store that cannot fetch
/// refuses `FetchRequired{address}`, never fabricates payload bytes
/// (AC-R-2.9.3-8/-10: a withheld member reports `MemberUnavailable`/
/// `FetchRequired`, never a false verdict).
pub fn member_bytes_or_refused<'a>(
    decoded: &'a Decoded,
    address: &str,
) -> Result<&'a [u8], BundleError> {
    match decoded.members.get(address) {
        Some(b) => Ok(b.as_slice()),
        None => match decoded
            .manifest
            .members
            .iter()
            .find(|m| m.address == address)
        {
            Some(_) => Err(BundleError::FetchRequired {
                address: address.to_string(),
            }),
            None => Err(BundleError::MemberUnavailable {
                address: address.to_string(),
            }),
        },
    }
}

/// `readers` helper — the L2 content-class member roles a
/// `PublicationPolicy`'s `content_classes` gate (§5h.3 §6). `content`
/// covers ledger pages/traces, results rows, model payloads, task data,
/// checkpoints and `model_io`; `accounting` covers budget docs;
/// `diagnostic` covers reports; `structural` is everything else.
pub fn content_class(role: &str) -> &'static str {
    if role.starts_with("ledger_page:")
        || role.starts_with("ledger_tree:")
        || role.starts_with("row:")
        || role.starts_with("contains:")
    {
        return "content";
    }
    match role {
        "traces" | "results" | "model" | "model_io" | "task_data" | "checkpoint" => "content",
        "budget" | "accounting" => "accounting",
        "nondeterminism" | "lcd_report" | "opacity_report" | "reproducibility" => "diagnostic",
        _ => "structural",
    }
}

/// The `ForeignIntegrityMismatch` claim-conflict row for an import
/// mapping report (§5h.3 §2 `import` — warning-level, never coerced).
pub fn integrity_conflict(name: &str, declared: &str, recomputed: &str) -> Json {
    Json::obj([
        ("code", Json::str("ForeignIntegrityMismatch")),
        ("member", Json::str(name)),
        ("declared", Json::str(declared)),
        ("recomputed", Json::str(recomputed)),
    ])
}

/// Convenience: a `BTreeMap` of `name → bytes` → `name → digest`.
pub fn digest_index(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(n, b)| (n.clone(), hh_identity::idp_id("blob", b)))
        .collect()
}
