//! `fetch(bundle, addresses[], locations?) → {materialized[], failed[]}`
//! — resolves `fetch[]` entries through a caller-supplied fetcher
//! (§5h.3 §2; S4.2). Rules the contract fixes: the fetched bytes are
//! verified on arrival (S3 — the address *is* the digest; a location's
//! claim is never trusted), fetched bytes are never executed, and the
//! manifest is never mutated — the caller materializes the bytes into
//! its own pool.
//!
//! R4.15d (OQ-332; DF-S4.2-1; the GATE-G7-adopted §trust-fields set):
//! `locations[]` remote members materialise through the declared
//! [`FetchTrustPolicy`] — every location is classified + admitted
//! ([`trust::admit`]) before any wire is touched, cleartext `http:` is
//! `transport_refused`, `expires` is enforced, `ipfs:`/CID-class
//! locations admit `content_addressed` then answer typed
//! `transport_unsupported` (no IPFS wire in the ADR-0365 scope), and a
//! remote location with no declared policy is `policy_absent` — an
//! unmediated fetch is never attempted. Every remote *attempt* yields a
//! [`trust::member_fetched_payload`] row in [`FetchOutcome::records`] —
//! the caller lands those `bundle.member.fetched` events durable before
//! the member reads `present` (`require_ledgered_arrival`).
//!
//! The `transport_pending` interim is gone: remote locations answer the
//! typed arms (`transport_refused` / `location_expired` /
//! `origin_not_admitted` / `class_not_admitted` / `policy_absent` /
//! `transport_unsupported`), never a silent miss.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::export::MemberBytes;
use crate::manifest::MemberStatus;
use crate::trust::{self, FetchTrustPolicy, LocationLeg};

/// The `fetch` outcome — `{materialized[], failed[]}` (the operation's
/// own record; refusals land per-address, never panic).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FetchOutcome {
    /// Addresses materialized + verified (`address → bytes` are handed to
    /// the caller for deposit).
    pub materialized: Vec<String>,
    /// `{address, reason}` per failure (`not_fetchable`, `no_fetcher`,
    /// `digest_mismatch`, `fetch_error`, `refused:<arm>`).
    pub failed: Vec<Json>,
    /// The verified bytes per materialized address.
    pub bytes: MemberBytes,
    /// The `bundle.member.fetched` payload per remote location attempt
    /// (admissions + refusals alike — the caller appends them before the
    /// member reads `present`; R4.15d, `require_ledgered_arrival`).
    pub records: Vec<Json>,
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
            ("fetched", Json::Arr(self.records.clone())),
        ])
    }
}

/// `fetch(decoded, addresses, pool, trust, remote, now_ms)` — for each
/// requested address: the manifest must list the member with
/// `status = fetch` (a `present` member needs no fetch;
/// `redacted`/`gc`/unknown fail `not_fetchable`) and a `fetch[]` entry
/// must exist. Each `locations[]` entry is then tried in order:
///
/// - pool/local locations (or an empty `locations[]`) resolve through
///   the pool fetcher by address — the landed S4.2 path;
/// - remote locations admit under `trust` ([`trust::admit`]) and ride
///   the `remote` fetcher — an admitted `https:` leg with no remote
///   fetcher reports `transport_unavailable`;
/// - the returned bytes recompute to the address on every arm
///   (`digest_mismatch` — the location's claim is never trusted).
///
/// The first verified location wins; a member whose locations all fail
/// lands `failed` with the last attempt's typed reason (every attempt
/// is in `records`).
///
/// The seams: `pool` — `FetchEntry → bytes` (the blob-pool path; `None`
/// fails `no_fetcher`); `remote` — `location URI → bytes` (the TLS GET
/// transport; `None` fails remote legs `transport_unavailable`, never
/// fabricates).
pub type Fetcher<'a> = dyn Fn(&crate::manifest::FetchEntry) -> Result<Vec<u8>, String> + 'a;

/// The remote-location seam — `location URI → bytes` (the `https:` TLS
/// GET; `hh-embed`'s `fetch_transport` drives it over the `hh_env`
/// egress wire). `FnMut` — a mediated wire is stateful (each attempt
/// appends its own durable rows).
pub type LocationFetcher<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

pub fn fetch(
    decoded: &Decoded,
    addresses: &[String],
    pool: Option<&Fetcher<'_>>,
    trust: Option<&FetchTrustPolicy>,
    mut remote: Option<&mut LocationFetcher<'_>>,
    now_ms: u64,
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
        // `locations[]` in declared order — the first verified arm wins;
        // every remote attempt lands a `bundle.member.fetched` payload.
        let locations: Vec<String> = if entry.locations.is_empty() {
            vec![String::new()]
        } else {
            entry.locations.clone()
        };
        let mut last_reason = String::new();
        for loc in &locations {
            match trust::admit(trust, entry, loc, now_ms) {
                Ok(LocationLeg::Pool) => match pool {
                    Some(p) => match p(entry) {
                        Ok(bytes) => {
                            let recomputed = hh_identity::idp_id("blob", &bytes);
                            if recomputed != *addr {
                                last_reason = "digest_mismatch".to_string();
                            } else {
                                out.materialized.push(addr.clone());
                                out.bytes.insert(addr.clone(), bytes);
                                last_reason.clear();
                                break;
                            }
                        }
                        Err(e) => {
                            last_reason = format!("fetch_error:{e}");
                        }
                    },
                    None => {
                        last_reason = "no_fetcher".to_string();
                    }
                },
                Ok(LocationLeg::HttpsGet { url, .. }) => {
                    let transport = "https";
                    match remote.as_deref_mut() {
                        Some(r) => match r(&url) {
                            Ok(bytes) => {
                                let recomputed = hh_identity::idp_id("blob", &bytes);
                                if recomputed != *addr {
                                    out.records.push(trust::member_fetched_payload(
                                        addr,
                                        loc,
                                        transport,
                                        "digest_mismatch",
                                        now_ms,
                                    ));
                                    last_reason = "digest_mismatch".to_string();
                                } else {
                                    out.records.push(trust::member_fetched_payload(
                                        addr, loc, transport, "verified", now_ms,
                                    ));
                                    out.materialized.push(addr.clone());
                                    out.bytes.insert(addr.clone(), bytes);
                                    last_reason.clear();
                                    break;
                                }
                            }
                            Err(e) => {
                                out.records.push(trust::member_fetched_payload(
                                    addr,
                                    loc,
                                    transport,
                                    &format!("error:{e}"),
                                    now_ms,
                                ));
                                last_reason = format!("fetch_error:{e}");
                            }
                        },
                        None => {
                            out.records.push(trust::member_fetched_payload(
                                addr,
                                loc,
                                transport,
                                "refused:transport_unavailable",
                                now_ms,
                            ));
                            last_reason = "refused:transport_unavailable".to_string();
                        }
                    }
                }
                Ok(LocationLeg::Unsupported { scheme, .. }) => {
                    out.records.push(trust::member_fetched_payload(
                        addr,
                        loc,
                        &scheme,
                        "refused:transport_unsupported",
                        now_ms,
                    ));
                    last_reason = "refused:transport_unsupported".to_string();
                }
                Err(r) => {
                    let transport =
                        trust::location_scheme(loc).unwrap_or_else(|| "local".to_string());
                    out.records.push(trust::member_fetched_payload(
                        addr,
                        loc,
                        &transport,
                        &format!("refused:{}", r.as_str()),
                        now_ms,
                    ));
                    last_reason = format!("refused:{}", r.as_str());
                }
            }
        }
        if !last_reason.is_empty() {
            fail(&mut out, &last_reason);
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
        "traces"
        | "results"
        | "model"
        | "model_io"
        | "task_data"
        | "checkpoint"
        | "hosted_native_records" => "content",
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
