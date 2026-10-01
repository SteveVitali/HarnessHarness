//! `audit_ref` — the citation coordinate every cell carries (§6.5 §6:
//! "every cell cites an `audit_ref` … citation verification goes through
//! inclusion proofs"; ADR-0067/0161 D4). The record is an address, never
//! bytes: `{run_id, seq, hash}` pins the event; `content_refs[]` names blob
//! addresses the cited evidence depends on (a redacted blob resolves
//! `Missing{redacted}`, never silently dropped — AC-R-2.10.5-12).

use hh_ledger::audit::BlobStatus;
use hh_ledger::errors::MissingReason;
use hh_ledger::store::Store;
use hh_wire::json::Json;

/// `audit_ref{run_id, seq, hash, checkpoint_ref?, content_refs[]}` — a chain
/// citation plus the blob addresses the cited evidence names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRef {
    /// The cited run.
    pub run_id: String,
    /// The cited seq.
    pub seq: u64,
    /// The cited event's `hash` (the idp/1 digest — the bytes the citation
    /// binds to).
    pub hash: String,
    /// A signed checkpoint covering the citation, when one exists.
    pub checkpoint_ref: Option<String>,
    /// Blob addresses the cited evidence carries (`refs[]`/evidence refs) —
    /// verified through the tombstone-aware blob lookup.
    pub content_refs: Vec<String>,
}

impl AuditRef {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("run_id".into(), Json::str(&self.run_id));
        m.insert("seq".into(), Json::Int(self.seq as i64));
        m.insert("hash".into(), Json::str(&self.hash));
        if let Some(c) = &self.checkpoint_ref {
            m.insert("checkpoint_ref".into(), Json::str(c));
        }
        if !self.content_refs.is_empty() {
            m.insert(
                "content_refs".into(),
                Json::Arr(self.content_refs.iter().map(Json::str).collect()),
            );
        }
        Json::Obj(m)
    }

    /// Strict decode; `None` on any violation.
    pub fn from_json(j: &Json) -> Option<AuditRef> {
        let Json::Obj(m) = j else {
            return None;
        };
        let run_id = m.get("run_id")?.as_str()?.to_string();
        let seq = m.get("seq")?.as_int()? as u64;
        let hash = m.get("hash")?.as_str()?.to_string();
        let checkpoint_ref = m
            .get("checkpoint_ref")
            .and_then(Json::as_str)
            .map(String::from);
        let content_refs = match m.get("content_refs") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| i.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()?,
            _ => Vec::new(),
        };
        Some(AuditRef {
            run_id,
            seq,
            hash,
            checkpoint_ref,
            content_refs,
        })
    }
}

/// `verify_citation`'s verdict — `ok | Tampered | Missing{reason}` (§6.5
/// §2.2 errors row; `ProofInvalid` is the inclusion-proof failure arm).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationVerdict {
    /// The cited event hash matches the committed envelope and the inclusion
    /// proof verifies against the run's tree head; every `content_refs`
    /// address resolves `present`.
    Verified,
    /// The cited seq carries a different `hash` — the citation names bytes
    /// that differ from the committed record (`Tampered(at_seq)`).
    Tampered {
        /// The cited seq.
        at_seq: u64,
    },
    /// The cited run/seq/blob is absent — the recorded reason (`gc`,
    /// `redacted`, `key_unavailable`), never `Tampered` (ADR-0068 R3).
    Missing {
        /// The missing reason.
        reason: MissingReason,
    },
    /// The event hash matched but the Merkle inclusion proof failed against
    /// the run's tree head — the chain does not carry the citation.
    ProofInvalid,
}

impl CitationVerdict {
    /// The canonical spelling for JSON surfaces.
    pub fn as_str(&self) -> String {
        match self {
            CitationVerdict::Verified => "ok".to_string(),
            CitationVerdict::Tampered { at_seq } => format!("tampered:{at_seq}"),
            CitationVerdict::Missing { reason } => format!("missing:{}", reason.as_str()),
            CitationVerdict::ProofInvalid => "proof_invalid".to_string(),
        }
    }

    /// `true` iff the citation resolves clean.
    pub fn is_verified(&self) -> bool {
        matches!(self, CitationVerdict::Verified)
    }
}

/// `verify_citation(audit_ref)` — resolve a citation against the store
/// (§6.5 §2.2): the committed envelope at `seq` must carry the cited `hash`
/// (else `Tampered{at_seq}`); the Merkle inclusion proof must verify against
/// the run's tree head (else `ProofInvalid`); every `content_refs` blob must
/// resolve `Present` (else `Missing{reason}` — tombstoned addresses report
/// their recorded reason). A run the store does not hold, or a seq beyond
/// the committed prefix, resolves `Missing{gc}` — the bytes are absent, the
/// chain is not thereby tampered.
pub fn verify_citation(store: &Store, audit_ref: &AuditRef) -> CitationVerdict {
    let Ok(events) = store.envelopes(&audit_ref.run_id) else {
        return CitationVerdict::Missing {
            reason: MissingReason::Gc,
        };
    };
    let Some(env) = events.get(audit_ref.seq as usize) else {
        return CitationVerdict::Missing {
            reason: MissingReason::Gc,
        };
    };
    if env.seq != audit_ref.seq || env.hash != audit_ref.hash {
        return CitationVerdict::Tampered {
            at_seq: audit_ref.seq,
        };
    }
    // The ADR-0067 inclusion-proof arm — the citation verifies against the
    // run's Merkle tree head (recomputed from the committed prefix), not
    // just the envelope scan.
    let leaves: Vec<String> = events.iter().map(|e| e.hash.clone()).collect();
    let tree_head = hh_ledger::tree::mth(&leaves);
    match store.prove_inclusion(&audit_ref.run_id, audit_ref.seq, None) {
        Ok(proof) => {
            if !hh_ledger::tree::verify_inclusion(&proof, &audit_ref.hash, &tree_head) {
                return CitationVerdict::ProofInvalid;
            }
        }
        Err(_) => return CitationVerdict::ProofInvalid,
    }
    for addr in &audit_ref.content_refs {
        match store.blob_status(addr) {
            BlobStatus::Present => {}
            BlobStatus::Tombstoned(reason) => {
                return CitationVerdict::Missing { reason };
            }
            BlobStatus::Missing => {
                return CitationVerdict::Missing {
                    reason: MissingReason::Gc,
                };
            }
        }
    }
    CitationVerdict::Verified
}
