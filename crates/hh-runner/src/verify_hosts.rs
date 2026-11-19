//! `hh-verify-host-derivation/1` — the transitive verify-host
//! derivation from captured runtime evidence (R6.51; DF-R6.45l-1's
//! discharge machinery).
//!
//! The `fp-w1-q-x55` freeze's verify arm derives over agent base ∪
//! `tests/*` scan ∪ oracle-observed hosts — but nothing in committed
//! text names a host that appears only at *runtime*: redirect targets
//! behind a granted index host (smoke1's `download-r2.pytorch.org`
//! behind the granted `download.pytorch.org`) and dataset URLs fetched
//! by verifier code paths (`ossci-datasets.s3.amazonaws.com`,
//! `yann.lecun.com`). This module is the runtime arm: it folds
//! **verify-phase-attributed** deny evidence — durable
//! `security.egress.decided` deny rows decided under the verify
//! layer's `policy_version_id` (the `verdict::denied_hosts` fold) plus
//! mediator/proxy observations captured in cell reports — into a typed
//! candidate transitive verify-host set with a provenance record per
//! host (which cell and which evidence kind supplied it).
//!
//! Honesty (AC-4): a deny decided under a non-verify policy layer —
//! the `pytorch-model-cli` `allowlist_gap{download.pytorch.org}`
//! agent-phase class (X-06) — is **recorded and never derived**;
//! a deny the evidence cannot phase-attribute lands `unattributed`,
//! likewise never a candidate. An empty evidence input emits an empty
//! set with `n/a{no_runtime_evidence}`, not a guess.
//!
//! The emitted record is canonical JSON (`Json::to_canonical_string`)
//! — same inputs, byte-identical output.

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::json::Json;

use crate::verdict;

/// The emitted record's schema spelling.
pub const DERIVATION_SCHEMA: &str = "hh-verify-host-derivation/1";
/// The typed n/a for an empty evidence input (AC-4 — never a guess).
pub const NA_NO_RUNTIME_EVIDENCE: &str = "n/a{no_runtime_evidence}";
/// The provenance kind of a durable WAL deny row.
pub const KIND_DECIDED: &str = "security.egress.decided";

/// A mediator/proxy observation carried by a captured record — the
/// record's own verify-phase-attributed `denied_hosts` fold plus its
/// all-phase deny set (`store.deny_hosts`), when the record carries
/// one. The `denied_hosts` member is verify-phase-attributed by
/// construction (the runtime's own `collect_denied`/`denied_hosts`
/// fold minted it); `observed_deny_hosts` spans every phase, so the
/// set difference names non-verify denies.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReportObservation {
    /// The provenance kind label (`hh-smoke/1.cells.denied_hosts`,
    /// `cell-report.denied_hosts`, …).
    pub kind: String,
    /// The verify-phase-attributed denied hosts the record carries.
    pub denied_hosts: Vec<String>,
    /// The record's all-phase mediator deny observation — empty when
    /// the record carries none.
    pub observed_deny_hosts: Vec<String>,
}

/// One captured cell's runtime evidence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellEvidence {
    /// The cell ref — `task/profile/attempt`
    /// (`pytorch-model-cli/H-allow/r0`).
    pub cell: String,
    /// The verify layer's policy `version_id` — the value a
    /// verify-phase `security.egress.decided` row stamps as
    /// `policy_version_id`. `None` → the cell's durable rows cannot be
    /// phase-attributed and land `unattributed`, never candidates.
    pub verify_version_id: Option<String>,
    /// The mediator-store `events.wal` bytes (`None` for a
    /// non-mediated cell or an absent store).
    pub wal: Option<Vec<u8>>,
    /// The captured mediator/proxy observations (cell reports, the
    /// committed smoke record's cell members).
    pub reports: Vec<ReportObservation>,
}

/// `wal_events(wal)` — the committed mediator-store `events.wal`
/// envelope: `{"k":"e","v":{event}}` rows and `{"k":"c","n":N}`
/// checkpoints (skipped); a bare event doc (no `k` member) is accepted
/// verbatim so a projection export folds identically.
fn wal_events(wal: &[u8]) -> Vec<Json> {
    let text = String::from_utf8_lossy(wal);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(env) = hharness_wire::json::parse(line) else {
            continue;
        };
        match env.get("k").and_then(Json::as_str) {
            Some("e") => {
                if let Some(v) = env.get("v") {
                    out.push(v.clone());
                }
            }
            Some(_) => {}
            None => out.push(env),
        }
    }
    out
}

/// `decided_deny_rows(events)` — `(payload, policy_version_id)` for
/// every `security.egress.decided` row whose decision is `deny`.
fn decided_deny_rows(events: &[Json]) -> Vec<(Json, Option<String>)> {
    let mut out = Vec::new();
    for e in events {
        if e.get("class").and_then(Json::as_str) != Some("security.egress.decided") {
            continue;
        }
        let Some(payload) = e.get("payload") else {
            continue;
        };
        if payload.get("decision").and_then(Json::as_str) != Some("deny") {
            continue;
        }
        let vid = payload
            .get("policy_version_id")
            .and_then(Json::as_str)
            .map(str::to_string);
        out.push((payload.clone(), vid));
    }
    out
}

/// `derive_verify_hosts(window, cells, sources)` — the fold. `cells`
/// order is normalized (sorted by ref); `sources` cites every input by
/// `sha256:` (the emitter records names, never paths).
pub fn derive_verify_hosts(
    window: &str,
    cells: &[CellEvidence],
    sources: &BTreeMap<String, String>,
) -> Json {
    // host → provenance entries (candidates) and host → observations
    // (excluded denies).
    let mut candidates: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    let mut non_verify: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    let mut unattributed: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    let mut saw_evidence = false;

    let mut sorted: Vec<&CellEvidence> = cells.iter().collect();
    sorted.sort_by(|a, b| a.cell.cmp(&b.cell));

    for cell in sorted {
        // ── durable `security.egress.decided` deny rows ────────────
        let mut wal_verify: BTreeSet<String> = BTreeSet::new();
        if let Some(wal) = &cell.wal {
            saw_evidence = true;
            let events = wal_events(wal);
            let pairs: Vec<(&str, &Json)> = events
                .iter()
                .filter_map(|e| {
                    let class = e.get("class").and_then(Json::as_str)?;
                    let payload = e.get("payload")?;
                    Some((class, payload))
                })
                .collect();
            // The committed fold (CC1 — never a second attribution
            // predicate): the scoped call is the verify-phase set.
            let all: BTreeSet<String> = verdict::denied_hosts(pairs.iter().copied(), None)
                .into_iter()
                .collect();
            wal_verify = match &cell.verify_version_id {
                Some(vid) => verdict::denied_hosts(pairs.iter().copied(), Some(vid))
                    .into_iter()
                    .collect(),
                None => BTreeSet::new(),
            };
            // Per-(host, layer) deny-row counts for provenance — the
            // row's own member table, same `collect_host_names` the
            // fold walks.
            let mut rows: BTreeMap<(String, Option<String>), i64> = BTreeMap::new();
            for (payload, vid) in decided_deny_rows(&events) {
                let mut hosts = BTreeSet::new();
                verdict::collect_host_names(&payload, &mut hosts);
                for h in hosts {
                    *rows.entry((h, vid.clone())).or_insert(0) += 1;
                }
            }
            for host in &all {
                if wal_verify.contains(host) {
                    let vid = cell.verify_version_id.clone().unwrap_or_default();
                    candidates.entry(host.clone()).or_default().push(Json::obj([
                        ("cell", Json::str(&cell.cell)),
                        ("kind", Json::str(KIND_DECIDED)),
                        ("policy_version_id", Json::str(&vid)),
                        (
                            "deny_rows",
                            Json::Int(*rows.get(&(host.clone(), Some(vid))).unwrap_or(&0)),
                        ),
                    ]));
                } else {
                    // A deny decided under a non-verify layer — agent /
                    // setup class (X-06): recorded, never derived. When
                    // the cell names no verify layer or the row carries
                    // no layer stamp, the deny is unattributed.
                    for (vid, n) in all_row_vids(&rows, host) {
                        let attributed_other_layer =
                            cell.verify_version_id.is_some() && vid.is_some();
                        let obs = Json::obj([
                            ("cell", Json::str(&cell.cell)),
                            ("kind", Json::str(KIND_DECIDED)),
                            (
                                "policy_version_id",
                                vid.map(Json::str).unwrap_or(Json::Null),
                            ),
                            ("deny_rows", Json::Int(n)),
                        ]);
                        if attributed_other_layer {
                            non_verify.entry(host.clone()).or_default().push(obs);
                        } else {
                            unattributed.entry(host.clone()).or_default().push(obs);
                        }
                    }
                }
            }
        }

        // ── captured mediator/proxy observations ───────────────────
        let mut reports: Vec<&ReportObservation> = cell.reports.iter().collect();
        reports.sort_by(|a, b| a.kind.cmp(&b.kind));
        for rep in reports {
            if !rep.denied_hosts.is_empty() || !rep.observed_deny_hosts.is_empty() {
                saw_evidence = true;
            }
            for host in &rep.denied_hosts {
                candidates.entry(host.clone()).or_default().push(Json::obj([
                    ("cell", Json::str(&cell.cell)),
                    ("kind", Json::str(&rep.kind)),
                ]));
            }
            // The record's all-phase deny set minus every verify-
            // attributed host = the non-verify leg of the observation.
            for host in &rep.observed_deny_hosts {
                if rep.denied_hosts.contains(host) || wal_verify.contains(host) {
                    continue;
                }
                non_verify.entry(host.clone()).or_default().push(Json::obj([
                    ("cell", Json::str(&cell.cell)),
                    ("kind", Json::str(format!("{}.observed", rep.kind))),
                ]));
            }
        }
    }

    let hosts: Vec<String> = candidates.keys().cloned().collect();
    let na = if saw_evidence {
        Json::Null
    } else {
        Json::str(NA_NO_RUNTIME_EVIDENCE)
    };
    Json::obj([
        ("schema", Json::str(DERIVATION_SCHEMA)),
        ("window", Json::str(window)),
        (
            "derivation",
            Json::str(
                "verify-phase-attributed security.egress.decided deny rows + \
                 captured denied_hosts observations → candidate transitive \
                 verify hosts; non-verify denies recorded, never derived",
            ),
        ),
        (
            "hosts",
            Json::Arr(hosts.iter().map(|h| Json::str(h.clone())).collect()),
        ),
        (
            "candidates",
            Json::Arr(
                candidates
                    .iter()
                    .map(|(h, prov)| {
                        Json::obj([
                            ("host", Json::str(h.clone())),
                            ("provenance", Json::Arr(prov.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "non_verify_denies",
            excluded(
                &non_verify,
                "excluded — decided under a non-verify policy layer",
            ),
        ),
        (
            "unattributed_denies",
            excluded(
                &unattributed,
                "excluded — no verify-phase attribution possible",
            ),
        ),
        ("na", na),
        (
            "sources",
            Json::Obj(
                sources
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
    ])
}

/// The distinct (layer, count) pairs a host's deny rows carry, sorted.
fn all_row_vids(
    rows: &BTreeMap<(String, Option<String>), i64>,
    host: &str,
) -> Vec<(Option<String>, i64)> {
    let mut out: Vec<(Option<String>, i64)> = rows
        .iter()
        .filter(|((h, _), _)| h == host)
        .map(|((_, vid), n)| (vid.clone(), *n))
        .collect();
    out.sort();
    out
}

/// An excluded-deny member set: per host, its observations plus the
/// typed disposition (never derived, never dropped silently).
fn excluded(map: &BTreeMap<String, Vec<Json>>, disposition: &str) -> Json {
    Json::Arr(
        map.iter()
            .map(|(h, obs)| {
                Json::obj([
                    ("host", Json::str(h.clone())),
                    ("observations", Json::Arr(obs.clone())),
                    ("disposition", Json::str(disposition)),
                ])
            })
            .collect(),
    )
}
