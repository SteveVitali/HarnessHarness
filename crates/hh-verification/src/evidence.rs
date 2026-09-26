//! `evidence` — `EvidenceHandle`, `EvidenceItem`, `EvidenceBundle`,
//! `OmissionRecord` and the bundle rules (spec §5f.1 §3, §5f.4 §3; ADR-0110 D2,
//! ADR-0115 D2/D4, ADR-0116 D4). The kernel resolves handles and stamps
//! authority from the records' provenance — a validator/critic body never
//! fetches evidence and never reads the transcript directly.

use std::collections::BTreeMap;

use hh_identity::idp::idp_id;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::Json;

use crate::vocab::{AdmissionMode, EvidenceClass, EvidenceKind, Freshness, Grounding, Integrity};

/// `EvidenceHandle` — the R-2.7.1 evidence pointer (ADR-0110 D2):
/// `{kind, ref, authority, provenance, produced_at_seq}`. `authority` is
/// stamped by the kernel from the record's provenance — never read from the
/// handle's own content (CC2).
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceHandle {
    /// The evidence kind.
    pub kind: EvidenceKind,
    /// The bound ref (`EventRef` / `ContentAddress` / effect_id /
    /// `(run_id, from, to)` rendering).
    pub handle_ref: String,
    /// The kernel-stamped authority (≥ `environment` for closed-schema
    /// evidence — enforced at `collect`).
    pub authority: AuthorityClass,
    /// The record's provenance.
    pub provenance: ProvenanceRecord,
    /// The seq the evidence was produced at.
    pub produced_at_seq: u64,
    /// The integrity form the handle satisfies.
    pub integrity: Integrity,
}

/// `EvidenceItem` — one bundle item (the critic form; ADR-0115 D2):
/// `{ref, evidence_class, provenance, label, bytes_or_view_hash, truncated}`.
/// `label` comes from the ledger, never the critic.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceItem {
    /// The item ref.
    pub item_ref: String,
    /// The item's evidence class.
    pub evidence_class: EvidenceClass,
    /// The item's authority (from provenance — the admission label join).
    pub authority: AuthorityClass,
    /// The item's provenance record.
    pub provenance: ProvenanceRecord,
    /// The item's label join rendering (`role_map(authority)` at render).
    pub label: String,
    /// The item's `bytes_hash` or `view_hash` (the reproducibility anchor).
    pub bytes_or_view_hash: String,
    /// Whether the item was truncated into the bundle.
    pub truncated: bool,
}

/// `OmissionRecord` — an item above the admission mode is *omitted and
/// listed*, never silently dropped (ADR-0116 D4; CF-245 — a critic that needed
/// an omitted item returns `inconclusive{missing_evidence}`, never `pass`).
#[derive(Debug, Clone, PartialEq)]
pub struct OmissionRecord {
    /// The omitted item's ref.
    pub item_ref: String,
    /// Why it was omitted.
    pub reason: OmissionReason,
}

/// The closed omission-reason sum (`{budget, admission, offload}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmissionReason {
    /// `budget` — the bundle budget could not carry it.
    Budget,
    /// `admission` — above the declared admission mode.
    Admission,
    /// `offload` — offloaded beyond the inline threshold.
    Offload,
}

impl OmissionReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OmissionReason::Budget => "budget",
            OmissionReason::Admission => "admission",
            OmissionReason::Offload => "offload",
        }
    }
}

/// `EvidenceBundle` — the content-addressed evidence set a check runs over
/// (R-2.7.1 form: `{handles[], inputs_digest}`; critic form adds `items`,
/// `omitted`, `task_contract_ref`, `reference_ref`). `bundle_id` +
/// `validator_ref`/`critic_ref` make a verdict reproducible at R1.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceBundle {
    /// The bundle id (`H(canonical(items))` — content-addressed).
    pub bundle_id: String,
    /// The R-2.7.1 handles (kernel-resolved, authority-stamped).
    pub handles: Vec<EvidenceHandle>,
    /// The critic-form items.
    pub items: Vec<EvidenceItem>,
    /// The omitted items (always listed).
    pub omitted: Vec<OmissionRecord>,
    /// The task-contract ref, when the check is contract-bound.
    pub task_contract_ref: Option<String>,
    /// The reference ref (comparative/calibration uses).
    pub reference_ref: Option<String>,
    /// `inputs_digest = H(canonical(sorted resolved addresses ∥ target ∥
    /// validator version_id))` — the R1 replay anchor (ADR-0110 D2).
    pub inputs_digest: String,
}

/// Bundle/collect failures (ADR-0110 D2; typed — never a warning).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// A declared evidence kind resolved to nothing (`EvidenceMissing`).
    EvidenceMissing {
        /// The missing kind.
        kind: &'static str,
    },
    /// An evidence requirement's freshness arm failed (`EvidenceStale`).
    EvidenceStale {
        /// The stale handle.
        handle_ref: String,
    },
    /// A handle's authority is below `environment` (`EvidenceUnauthoritative`).
    EvidenceUnauthoritative {
        /// The unauthoritative handle.
        handle_ref: String,
    },
    /// The bundle contains only `model_claim`/`claimed` observations
    /// (`ClaimOnlyEvidence` — I-V2; AC-R-2.7.1-3).
    ClaimOnlyEvidence,
    /// A pinned fixture/validator's workspace copy mismatches its
    /// `ContentAddress` (`EvidenceTampered` — a veto invariant, ADR-0109 D4).
    EvidenceTampered {
        /// The tampered handle.
        handle_ref: String,
    },
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::EvidenceMissing { kind } => write!(f, "EvidenceMissing({kind})"),
            BundleError::EvidenceStale { handle_ref } => {
                write!(f, "EvidenceStale({handle_ref})")
            }
            BundleError::EvidenceUnauthoritative { handle_ref } => {
                write!(f, "EvidenceUnauthoritative({handle_ref})")
            }
            BundleError::ClaimOnlyEvidence => write!(f, "ClaimOnlyEvidence"),
            BundleError::EvidenceTampered { handle_ref } => {
                write!(f, "EvidenceTampered({handle_ref})")
            }
        }
    }
}

impl std::error::Error for BundleError {}

/// The `inputs_digest` construction — `H(canonical(sorted resolved addresses ∥
/// target ∥ validator version_id))` (ADR-0110 D2). Deterministic: equal inputs
/// give equal digests (R1); changing one evidence byte changes the digest
/// (AC-R-2.7.1-4).
pub fn inputs_digest(
    resolved_addresses: &[String],
    target: &str,
    validator_version_id: &str,
) -> String {
    let mut sorted = resolved_addresses.to_vec();
    sorted.sort();
    let canonical = Json::obj([
        (
            "addresses",
            Json::Arr(sorted.iter().map(Json::str).collect()),
        ),
        ("target", Json::str(target)),
        ("validator_version_id", Json::str(validator_version_id)),
    ])
    .to_canonical_string();
    idp_id("verification.inputs", canonical.as_bytes())
}

/// Build a bundle over resolved handles and items (ADR-0110 D2's collect
/// contract at C0 — pure projection, content-addressed). Every handle is
/// validated for authority; a bundle of only `claimed` items is refused
/// (`ClaimOnlyEvidence`).
pub fn build_bundle(
    handles: Vec<EvidenceHandle>,
    items: Vec<EvidenceItem>,
    omitted: Vec<OmissionRecord>,
    task_contract_ref: Option<String>,
    target: &str,
    validator_version_id: &str,
) -> Result<EvidenceBundle, BundleError> {
    for h in &handles {
        // `delegate` orders above `environment` in the command lattice but is
        // model-claimed text — never authoritative evidence (same rule as
        // `AuthoritativeHandle::validate`).
        if h.authority < AuthorityClass::Environment || h.authority == AuthorityClass::Delegate {
            return Err(BundleError::EvidenceUnauthoritative {
                handle_ref: h.handle_ref.clone(),
            });
        }
    }
    // I-V2: a bundle of only model_claim observations is refused. `model_io`
    // handles and `claimed` items are claim-side evidence; a bundle needs at
    // least one measured/reconciled member.
    let only_claims = handles.iter().all(|h| h.kind == EvidenceKind::ModelIo)
        && items
            .iter()
            .all(|i| i.evidence_class == EvidenceClass::Claimed);
    if (!handles.is_empty() || !items.is_empty()) && only_claims {
        return Err(BundleError::ClaimOnlyEvidence);
    }
    let addresses: Vec<String> = handles
        .iter()
        .map(|h| h.handle_ref.clone())
        .chain(items.iter().map(|i| i.bytes_or_view_hash.clone()))
        .collect();
    let digest = inputs_digest(&addresses, target, validator_version_id);
    let bundle_id = idp_id(
        "verification.bundle",
        Json::obj([
            (
                "handles",
                Json::Arr(addresses.iter().map(Json::str).collect()),
            ),
            (
                "omitted",
                Json::Arr(
                    omitted
                        .iter()
                        .map(|o| Json::str(o.item_ref.clone()))
                        .collect(),
                ),
            ),
            ("target", Json::str(target)),
        ])
        .to_canonical_string()
        .as_bytes(),
    );
    Ok(EvidenceBundle {
        bundle_id,
        handles,
        items,
        omitted,
        task_contract_ref,
        reference_ref: None,
        inputs_digest: digest,
    })
}

/// Check a freshness requirement against a handle produced at `seq` given the
/// last effect-terminal seq on the scope (ADR-0109 D2).
pub fn check_freshness(
    freshness: &Freshness,
    produced_at_seq: u64,
    last_effect_seq: Option<u64>,
) -> bool {
    freshness.satisfied_by(produced_at_seq, last_effect_seq)
}

/// Admission — the declared-mode projection (ADR-0116 D4). Items above the
/// mode are omitted *and listed*; the caller decides whether an omission is
/// `inconclusive{missing_evidence}`.
pub fn admit_items(
    mode: AdmissionMode,
    items: Vec<EvidenceItem>,
) -> (Vec<EvidenceItem>, Vec<OmissionRecord>) {
    let mut kept = Vec::new();
    let mut omitted = Vec::new();
    for item in items {
        if mode.admits(item.authority) {
            kept.push(item);
        } else {
            omitted.push(OmissionRecord {
                item_ref: item.item_ref.clone(),
                reason: OmissionReason::Admission,
            });
        }
    }
    (kept, omitted)
}

/// The derived grounding — `strongest evidence_class among cited items`
/// (ADR-0115 D4). No cited items ⇒ `ungrounded` ⇒ `oracle_failure` upstream.
pub fn grounding_of(items: &[EvidenceItem]) -> Grounding {
    items
        .iter()
        .map(|i| Grounding::of(i.evidence_class))
        .max()
        .unwrap_or(Grounding::Ungrounded)
}

/// A pinned-fixture pin check (ADR-0109 D4): compare a workspace copy's
/// `ContentAddress` against the pinned address — mismatch is `EvidenceTampered`
/// (a veto invariant, not a warning).
pub fn pin_check(handle_ref: &str, expected: &str, observed: &str) -> Result<(), BundleError> {
    if expected == observed {
        Ok(())
    } else {
        Err(BundleError::EvidenceTampered {
            handle_ref: handle_ref.to_string(),
        })
    }
}

/// The bundle's canonical JSON (the `evidence` member of a verdict payload).
pub fn bundle_json(bundle: &EvidenceBundle) -> Json {
    let mut m = BTreeMap::new();
    m.insert("bundle_id".to_string(), Json::str(bundle.bundle_id.clone()));
    m.insert(
        "handles".to_string(),
        Json::Arr(
            bundle
                .handles
                .iter()
                .map(|h| {
                    Json::obj([
                        ("kind", Json::str(h.kind.as_str())),
                        ("ref", Json::str(h.handle_ref.clone())),
                    ])
                })
                .collect(),
        ),
    );
    m.insert(
        "items".to_string(),
        Json::Arr(
            bundle
                .items
                .iter()
                .map(|i| {
                    Json::obj([
                        ("ref", Json::str(i.item_ref.clone())),
                        ("evidence_class", Json::str(i.evidence_class.as_str())),
                        ("truncated", Json::Bool(i.truncated)),
                    ])
                })
                .collect(),
        ),
    );
    m.insert(
        "omitted".to_string(),
        Json::Arr(
            bundle
                .omitted
                .iter()
                .map(|o| {
                    Json::obj([
                        ("ref", Json::str(o.item_ref.clone())),
                        ("reason", Json::str(o.reason.as_str())),
                    ])
                })
                .collect(),
        ),
    );
    m.insert(
        "inputs_digest".to_string(),
        Json::str(bundle.inputs_digest.clone()),
    );
    Json::Obj(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_prov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("kernel:verification", 3)
    }

    fn handle(kind: EvidenceKind, authority: AuthorityClass) -> EvidenceHandle {
        EvidenceHandle {
            kind,
            handle_ref: "sha256:abc".into(),
            authority,
            provenance: kernel_prov(),
            produced_at_seq: 5,
            integrity: Integrity::ContentAddressed,
        }
    }

    fn item(class: EvidenceClass, authority: AuthorityClass) -> EvidenceItem {
        EvidenceItem {
            item_ref: "evt:1".into(),
            evidence_class: class,
            authority,
            provenance: kernel_prov(),
            label: "fact".into(),
            bytes_or_view_hash: "sha256:def".into(),
            truncated: false,
        }
    }

    #[test]
    fn claim_only_bundle_is_refused() {
        // I-V2 / AC-R-2.7.1-3: transcript-only evidence yields no verdict.
        let r = build_bundle(
            vec![handle(EvidenceKind::ModelIo, AuthorityClass::Kernel)],
            vec![],
            vec![],
            None,
            "t",
            "v:0",
        );
        assert_eq!(r, Err(BundleError::ClaimOnlyEvidence));
        let r = build_bundle(
            vec![],
            vec![item(EvidenceClass::Claimed, AuthorityClass::Delegate)],
            vec![],
            None,
            "t",
            "v:0",
        );
        assert_eq!(r, Err(BundleError::ClaimOnlyEvidence));
        // One measured item makes it admissible.
        assert!(build_bundle(
            vec![],
            vec![
                item(EvidenceClass::Claimed, AuthorityClass::Delegate),
                item(EvidenceClass::Measured, AuthorityClass::Kernel),
            ],
            vec![],
            None,
            "t",
            "v:0",
        )
        .is_ok());
    }

    #[test]
    fn unauthoritative_handle_is_refused() {
        let r = build_bundle(
            vec![handle(EvidenceKind::EndState, AuthorityClass::Delegate)],
            vec![],
            vec![],
            None,
            "t",
            "v:0",
        );
        assert_eq!(
            r,
            Err(BundleError::EvidenceUnauthoritative {
                handle_ref: "sha256:abc".into()
            })
        );
    }

    #[test]
    fn inputs_digest_is_deterministic_and_sensitive() {
        // AC-R-2.7.1-4: same inputs ⇒ same digest; one byte differs ⇒ differs.
        let a = inputs_digest(&["sha256:1".into(), "sha256:2".into()], "t", "v:0");
        let b = inputs_digest(&["sha256:2".into(), "sha256:1".into()], "t", "v:0");
        assert_eq!(a, b); // sorted — order-insensitive
        let c = inputs_digest(&["sha256:1".into(), "sha256:3".into()], "t", "v:0");
        assert_ne!(a, c);
        let d = inputs_digest(&["sha256:1".into(), "sha256:2".into()], "t2", "v:0");
        assert_ne!(a, d);
    }

    #[test]
    fn admission_omits_and_lists() {
        let (kept, omitted) = admit_items(
            AdmissionMode::PrincipalOnly,
            vec![
                item(EvidenceClass::Measured, AuthorityClass::Kernel),
                item(EvidenceClass::Measured, AuthorityClass::External),
            ],
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(omitted.len(), 1);
        assert_eq!(omitted[0].reason, OmissionReason::Admission);
    }

    #[test]
    fn grounding_is_derived_from_cited_items() {
        assert_eq!(grounding_of(&[]), Grounding::Ungrounded);
        assert_eq!(
            grounding_of(&[item(EvidenceClass::Claimed, AuthorityClass::Delegate)]),
            Grounding::Claimed
        );
        assert_eq!(
            grounding_of(&[
                item(EvidenceClass::Claimed, AuthorityClass::Delegate),
                item(EvidenceClass::Measured, AuthorityClass::Kernel),
            ]),
            Grounding::Measured
        );
        assert_eq!(
            grounding_of(&[item(EvidenceClass::Reconciled, AuthorityClass::Kernel)]),
            Grounding::Reconciled
        );
    }

    #[test]
    fn freshness_checks() {
        assert!(check_freshness(&Freshness::Any, 1, Some(5)));
        assert!(check_freshness(
            &Freshness::AfterLastEffectOnScope,
            10,
            Some(5)
        ));
        assert!(!check_freshness(
            &Freshness::AfterLastEffectOnScope,
            4,
            Some(5)
        ));
        assert!(check_freshness(&Freshness::AfterLastEffectOnScope, 4, None));
        assert!(!check_freshness(&Freshness::AfterSeq(7), 7, None));
        assert!(check_freshness(&Freshness::AfterSeq(7), 8, None));
    }

    #[test]
    fn pin_check_trips_evidence_tampered() {
        assert!(pin_check("fixture:1", "sha256:aa", "sha256:aa").is_ok());
        assert_eq!(
            pin_check("fixture:1", "sha256:aa", "sha256:bb"),
            Err(BundleError::EvidenceTampered {
                handle_ref: "fixture:1".into()
            })
        );
    }
}
