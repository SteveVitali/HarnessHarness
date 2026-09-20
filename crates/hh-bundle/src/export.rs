//! `LedgerExport` assembly — the chunked canonical event tree over a
//! run's durable prefix (§5h.3 §3 layer P; CF-299). Pages are canonical
//! `Json` documents (`{page, seq_lo, seq_hi, events[]}`), each content-
//! addressed; the `tree` is `idp/1` over the ordered page-address list —
//! the one tree rule (R-ID-3), so Merkle partial verification and
//! header-first reads stay possible.

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::error::BundleError;
use crate::manifest::{BlobIndexEntry, LedgerExport};

/// Events per `LedgerExport` page (64 — header-first reads stay cheap;
/// the constant is part of the exporter's contract, not the schema's).
pub const LEDGER_PAGE_SIZE: u64 = 64;

/// A member the assembler produced: content-addressed bytes keyed by
/// their `idp/1` address.
pub type MemberBytes = BTreeMap<String, Vec<u8>>;

/// Content-address `bytes` and record them in `members`.
pub fn add_member(
    members: &mut MemberBytes,
    bytes: Vec<u8>,
    media_type: &str,
) -> (String, u64) {
    let addr = hh_identity::address(&bytes, media_type);
    let size = bytes.len() as u64;
    members.insert(addr.id(), bytes);
    (addr.id(), size)
}

/// `envelope → canonical page-document bytes`.
fn page_bytes(page_no: u64, events: &[EventEnvelope]) -> Vec<u8> {
    let doc = Json::obj([
        ("page", Json::Int(page_no as i64)),
        (
            "seq_lo",
            Json::Int(events.first().map(|e| e.seq).unwrap_or(0) as i64),
        ),
        (
            "seq_hi",
            Json::Int(events.last().map(|e| e.seq).unwrap_or(0) as i64),
        ),
        (
            "events",
            Json::Arr(events.iter().map(|e| e.to_json()).collect()),
        ),
    ]);
    doc.to_canonical_string().into_bytes()
}

/// Decode a page document back to envelopes (validation/import).
pub fn decode_page(bytes: &[u8]) -> Result<Vec<EventEnvelope>, BundleError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| BundleError::Malformed {
        detail: "ledger page is not utf-8".into(),
    })?;
    let doc = hh_wire::json::parse(&text).map_err(|e| BundleError::Malformed {
        detail: format!("ledger page parse: {e}"),
    })?;
    match doc.get("events") {
        Some(Json::Arr(evs)) => evs
            .iter()
            .map(EventEnvelope::from_json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| BundleError::Malformed {
                detail: format!("ledger page envelope: {e}"),
            }),
        _ => Err(BundleError::Malformed {
            detail: "ledger page has no events[]".into(),
        }),
    }
}

/// Build the `LedgerExport` for `run_id` — appending the page members to
/// `members` and returning the export record plus the page `MemberRef`
/// roles (the caller adds them to the manifest's member index).
pub fn build_ledger_export(
    store: &Store,
    run_id: &str,
    members: &mut MemberBytes,
) -> Result<(LedgerExport, Vec<(String, String, u64)>), BundleError> {
    let events = store
        .envelopes(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;
    let mut pages = Vec::new();
    let mut roles = Vec::new();
    for (i, chunk) in events.chunks(LEDGER_PAGE_SIZE as usize).enumerate() {
        let bytes = page_bytes(i as u64, chunk);
        let (addr, size) = add_member(members, bytes, "application/vnd.hh.ledger-page+json");
        pages.push(addr.clone());
        roles.push((format!("ledger_page:{run_id}:{i}"), addr, size));
    }
    let tree = LedgerExport::tree_address(&pages);
    // The tree itself is a member — header-first readers pull it before
    // any page.
    let tree_doc = Json::obj([
        ("run_id", Json::str(run_id)),
        (
            "pages",
            Json::Arr(pages.iter().map(|p| Json::str(p.clone())).collect()),
        ),
    ]);
    let (tree_addr, tree_size) = add_member(
        members,
        tree_doc.to_canonical_string().into_bytes(),
        "application/vnd.hh.ledger-tree+json",
    );
    debug_assert_eq!(tree_addr, tree);
    roles.push((format!("ledger_tree:{run_id}"), tree_addr, tree_size));

    // Blob index — every `refs[]` address with its store status.
    let mut blob_index = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for ev in events {
        for r in &ev.refs {
            if seen.insert(r.id()) {
                let (status, size) = match store.get_blob(r) {
                    Ok(b) => ("present".to_string(), b.len() as u64),
                    Err(e) => {
                        let s = format!("{e:?}").to_lowercase();
                        (
                            if s.contains("redact") {
                                "redacted"
                            } else {
                                "gc"
                            }
                            .to_string(),
                            0u64,
                        )
                    }
                };
                blob_index.push(BlobIndexEntry {
                    address: r.id(),
                    status,
                    size,
                });
            }
        }
    }

    // Checkpoints — `security.audit.checkpoint` durable rows, as their
    // EventRef coordinates.
    let checkpoints: Vec<Json> = events
        .iter()
        .filter(|e| e.class == "security.audit.checkpoint")
        .map(|e| {
            Json::obj([
                ("run_id", Json::str(run_id)),
                ("event_id", Json::str(e.event_id.clone())),
                ("seq", Json::Int(e.seq as i64)),
            ])
        })
        .collect();

    let head = store.head(run_id).map_err(|_| BundleError::RunNotFound {
        run_id: run_id.to_string(),
    })?;
    let head_json = Json::obj([
        ("seq", Json::Int(head.seq as i64)),
        ("event_id", Json::str(head.event_id.clone())),
        ("hash", Json::str(head.hash.clone())),
    ]);

    // Lineage prefixes — the manifest's fork/continuation anchors.
    let manifest = store
        .manifest(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;
    let mut lineage_prefixes = Vec::new();
    for link in [&manifest.forked_from, &manifest.continued_from]
        .into_iter()
        .flatten()
    {
        lineage_prefixes.push(Json::obj([
            ("run_id", Json::str(link.run_id.clone())),
            ("up_to_seq", Json::Int(link.at_seq as i64)),
            ("head_hash", Json::str(link.head_hash.clone())),
        ]));
    }

    Ok((
        LedgerExport {
            run_id: run_id.to_string(),
            page_size: LEDGER_PAGE_SIZE,
            pages,
            tree,
            checkpoints,
            blob_index,
            head: head_json,
            lineage_prefixes,
        },
        roles,
    ))
}
