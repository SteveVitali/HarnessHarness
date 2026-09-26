//! [`ProvenanceRecord`] — the **one** provenance record (§8.1 #3; ADR-0033 D1/D7) — plus
//! [`Derivation`], [`Attestation`], the validation/append errors, and
//! [`verify_attestation`].
//!
//! Mandatory on every HIR node and edge and on every class in the mandatory-provenance table
//! ([`crate::mandatory`]); defaults are omitted from the canonical form when default. The
//! record is excluded from `semantic_id` and included in `version_id` (ADR-0033 D7;
//! ADR-0015) — identity composition is `hh-identity`'s concern, above this layer.
//!
//! **CC2 anchors here:** `authority` is *derived* — minted by
//! [`crate::origin::default_authority`] from `origin`, or raised only by a `security.label.
//! endorsed`/`declassified` event on the closed [`crate::endorse::EndorsementBasis`] list.
//! [`ProvenanceRecord::validate`] enforces `TaintedAboveExternal` and `AuthorityExceedsOrigin`
//! at validate/append (§8.1 #5).

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, PersistenceScope, ReaderSet, TaintTag};
use crate::label::Label;
use crate::origin::{default_authority, Origin};

/// `Derivation.kind` — the PROV `wasDerivedFrom` subtypes (§8.1 #3; ADR-0033 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivationKind {
    /// A model-produced summary.
    Summary,
    /// A compaction — `derived_from` lists the forgotten ids.
    Compaction,
    /// An extraction.
    Extraction,
    /// A quotation.
    Quotation,
    /// A revision.
    Revision,
    /// A translation.
    Translation,
    /// A subagent result entering the parent (P2 ingestion).
    SubagentResult,
    /// A deterministic projection (truncation, redaction, structured projection).
    Projection,
}

impl DerivationKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DerivationKind::Summary => "summary",
            DerivationKind::Compaction => "compaction",
            DerivationKind::Extraction => "extraction",
            DerivationKind::Quotation => "quotation",
            DerivationKind::Revision => "revision",
            DerivationKind::Translation => "translation",
            DerivationKind::SubagentResult => "subagent_result",
            DerivationKind::Projection => "projection",
        }
    }

    /// Model-produced kinds — `deriver = delegate` for these, and the derived label is capped
    /// at `min(⊔ inputs, delegate)` (§8.1 #2 `derive`; ADR-0034 P1 as amended).
    pub fn is_model_produced(self) -> bool {
        matches!(
            self,
            DerivationKind::Summary
                | DerivationKind::Compaction
                | DerivationKind::Extraction
                | DerivationKind::Translation
        )
    }
}

/// `Derivation` (§8.1 #3): `{kind, inputs: [Ref], deriver: Origin, deterministic: bool}`.
/// Lineage = `derived_from` inside the record plus the ledger's `causes[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    /// The derivation kind.
    pub kind: DerivationKind,
    /// The inputs' identity coordinates (`version_id`s / content addresses — for compaction,
    /// the forgotten ids).
    pub inputs: Vec<String>,
    /// Who performed the derivation.
    pub deriver: Origin,
    /// Whether the derivation is deterministic (a projection).
    pub deterministic: bool,
}

/// `Attestation.kind` (§8.1 #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttestationKind {
    /// A `seal` attestation (the sealed definition).
    Seal,
    /// The per-run hash chain (§05a, ADR-0029) — the `hash_chain` anchor for every stamped
    /// record, so the audit attestation over chain heads attests provenance without a second
    /// mechanism (ADR-0035 D4).
    HashChain,
    /// A signature under a trust root.
    Signature,
    /// A `pin` attestation (verified signature/hash under `TrustRootPolicy`).
    Pin,
}

impl AttestationKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AttestationKind::Seal => "seal",
            AttestationKind::HashChain => "hash_chain",
            AttestationKind::Signature => "signature",
            AttestationKind::Pin => "pin",
        }
    }
}

/// `Attestation.anchor: SignerRef | ChainRef` (§8.1 #3). Identity coordinates, as with
/// `Origin`'s ref fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationAnchor {
    /// Anchors on a signer / trust root.
    Signer(String),
    /// Anchors on a chain head (the per-run hash chain).
    Chain(String),
}

/// `Attestation` (§8.1 #3): `{kind, subject_hash, anchor, verified_by: kernel(component_ref),
/// verified_at}` — a *verified* fact, never self-declared: the record documents that a kernel
/// component verified the anchor against a trust set at `verified_at`
/// ([`verify_attestation`]); a payload's own claim is never trusted (in-toto/SLSA
/// signer–builder rule; ADR-0033 D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    /// The attestation kind.
    pub kind: AttestationKind,
    /// The hash of the attested subject.
    pub subject_hash: String,
    /// The verification anchor (signer / trust root, or a chain head).
    pub anchor: AttestationAnchor,
    /// The kernel component that performed the verification.
    pub verified_by: String,
    /// The logical time (seq) of the verification — never a wall clock.
    pub verified_at: u64,
}

impl Attestation {
    /// Structural self-consistency: a real verification record names its subject, anchor and
    /// kernel verifier. A record failing this is a self-claim — `default_authority` mints it
    /// `unverified` (the attestation-failed case).
    pub fn self_consistent(&self) -> bool {
        !self.subject_hash.is_empty()
            && !self.verified_by.is_empty()
            && match &self.anchor {
                AttestationAnchor::Signer(s) | AttestationAnchor::Chain(s) => !s.is_empty(),
            }
    }
}

/// The trust set [`verify_attestation`] anchors on: the signers/trust roots and chain heads the
/// caller knows good. Anchoring on the record's own claim is refused — verification is over
/// the *external* trust set (ADR-0033 D1).
#[derive(Debug, Clone, Default)]
pub struct TrustedAnchors {
    /// Trusted signer / trust-root refs.
    pub signers: BTreeSet<String>,
    /// Trusted chain heads.
    pub chain_heads: BTreeSet<String>,
}

/// `ProvenanceRecord` (§8.1 #3): `{origin, authority, taint = ∅, readers = Public, scope,
/// derived_from = [], created_at, attestation?}` — **the one record** every HIR node/edge and
/// every mandatory-table event carries (CC1: no second record anywhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// Who produced it — a fact, never an ordering.
    pub origin: Origin,
    /// How much it may command the harness — derived, never declared (CC2).
    pub authority: AuthorityClass,
    /// Taint carried at C0, enforced at C2 (`taint ≠ ∅ ⇒ authority ≤ external`).
    pub taint: BTreeSet<TaintTag>,
    /// Who may read it (carried at C0, enforced at C2).
    pub readers: ReaderSet,
    /// Where it lives — distinct from its authority.
    pub scope: PersistenceScope,
    /// The `wasDerivedFrom` lineage.
    pub derived_from: Vec<Derivation>,
    /// The transaction/logical time (`seq`) of minting — never a wall clock.
    pub created_at: u64,
    /// A verified attestation, when one exists.
    pub attestation: Option<Attestation>,
}

/// Validation/append failure modes (§8.1 #5; "every violation is a typed validation or append
/// error, never a warning" — ADR-0033 D7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    /// A mandatory-provenance class carried no `ProvenanceRecord`.
    MissingProvenance {
        /// What was missing the record (the event class / derivation kind).
        what: String,
    },
    /// `taint ≠ ∅` with `authority > external`.
    TaintedAboveExternal {
        /// The claimed authority that taint forbids.
        authority: AuthorityClass,
    },
    /// The stamped `authority` exceeds what `origin` + in-record evidence (a `seal`/`pin`
    /// attestation, or a supplied endorsement event) can legitimately confer.
    AuthorityExceedsOrigin {
        /// The ceiling the origin/evidence can confer.
        ceiling: AuthorityClass,
        /// The class the record claims.
        claimed: AuthorityClass,
    },
    /// An attestation that fails [`verify_attestation`].
    AttestationFailed {
        /// Why verification failed.
        reason: String,
    },
    /// A derivation whose inputs were empty (`derive` requires ≥1 input record).
    DerivationWithoutInputs,
    /// `lift_provenance` was asked to assign an origin outside `{import, participant, tool}`.
    LiftOriginNotAllowed,
}

impl ProvenanceRecord {
    /// Mint a record at `origin`'s derived class: `authority = default_authority(origin,
    /// scope, None)`, `taint = ∅`, `readers = Public` — the entry point for new content.
    pub fn minted(origin: Origin, scope: PersistenceScope, created_at: u64) -> ProvenanceRecord {
        ProvenanceRecord {
            authority: default_authority(&origin, scope, None),
            origin,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
            scope,
            derived_from: Vec::new(),
            created_at,
            attestation: None,
        }
    }

    /// Mint under a [`MintingContext`] — `authority = default_authority_in(
    /// origin, scope, None, ctx)` (CC8 additive: `minted` delegates with an
    /// empty context). The only behavioural difference a context makes for a
    /// `participant` origin is the vouched → `delegate` path (DF-S1.3-2;
    /// §6.6) — the vouch set is conferred by the Lab, never by the record.
    pub fn minted_in(
        origin: Origin,
        scope: PersistenceScope,
        created_at: u64,
        ctx: &crate::origin::MintingContext,
    ) -> ProvenanceRecord {
        ProvenanceRecord {
            authority: crate::origin::default_authority_in(&origin, scope, None, ctx),
            origin,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
            scope,
            derived_from: Vec::new(),
            created_at,
            attestation: None,
        }
    }

    /// Mint with an attestation already verified (`authority = default_authority(origin,
    /// scope, Some(att))`).
    pub fn minted_attested(
        origin: Origin,
        scope: PersistenceScope,
        created_at: u64,
        attestation: Attestation,
    ) -> ProvenanceRecord {
        let authority = default_authority(&origin, scope, Some(&attestation));
        ProvenanceRecord {
            authority,
            origin,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
            scope,
            derived_from: Vec::new(),
            created_at,
            attestation: Some(attestation),
        }
    }

    /// A kernel-origin fact (ledger facts, monitor decisions, validator verdicts).
    pub fn kernel(component_ref: impl Into<String>, created_at: u64) -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::kernel(component_ref),
            PersistenceScope::Run,
            created_at,
        )
    }

    /// A human-principal record carrying an attestation (e.g. for a widening publish — the
    /// §8.3 widening-successor rule reads `authority == principal` + attestation).
    pub fn human_attested(
        author_ref: impl Into<String>,
        role: crate::origin::HumanRole,
        scope: PersistenceScope,
        created_at: u64,
        attestation: Attestation,
    ) -> ProvenanceRecord {
        ProvenanceRecord::minted_attested(
            Origin::human(author_ref, role),
            scope,
            created_at,
            attestation,
        )
    }

    /// The record's `Label` projection `(authority, taint, readers)`.
    pub fn label(&self) -> Label {
        Label {
            authority: self.authority,
            taint: self.taint.clone(),
            readers: self.readers.clone(),
        }
    }

    /// The authority ceiling `origin` + in-record evidence can confer: the minted class, raised
    /// to `definition` only by a `seal`/`pin` attestation ("`definition` only at `seal` or by a
    /// `pin` endorsement" — §8.1 #3 minting).
    pub fn minted_ceiling(&self) -> AuthorityClass {
        let base = default_authority(&self.origin, self.scope, self.attestation.as_ref());
        match &self.attestation {
            Some(att)
                if att.self_consistent()
                    && matches!(att.kind, AttestationKind::Seal | AttestationKind::Pin) =>
            {
                base.max(AuthorityClass::Definition)
            }
            _ => base,
        }
    }

    /// `validate`/`append`-time check (§8.1 #5/#6): `MissingProvenance` is raised by the caller
    /// when no record exists at all; this checks a present record:
    /// - `taint ≠ ∅ ⇒ authority ≤ external` (`TaintedAboveExternal`);
    /// - `authority ≤ minted ceiling` unless a legitimate endorsement event covers the rise
    ///   (`AuthorityExceedsOrigin` — location/self-claim can never elevate).
    ///
    /// `endorsement_evidence` is the `security.label.endorsed`/`declassified` event that
    /// legitimized an authority above the minted ceiling (evidence lives in the ledger, not in
    /// the record — it is passed in by the validating context).
    pub fn validate(
        &self,
        endorsement_evidence: Option<&crate::endorse::LabelEndorsed>,
    ) -> Result<(), ProvenanceError> {
        if !self.taint.is_empty() && self.authority > AuthorityClass::External {
            return Err(ProvenanceError::TaintedAboveExternal {
                authority: self.authority,
            });
        }
        let ceiling = self.minted_ceiling();
        if self.authority > ceiling {
            let covered = endorsement_evidence
                .is_some_and(|e| e.to.authority == self.authority && e.from.authority <= ceiling);
            if !covered {
                return Err(ProvenanceError::AuthorityExceedsOrigin {
                    ceiling,
                    claimed: self.authority,
                });
            }
        }
        if let Some(att) = &self.attestation {
            if !att.self_consistent() {
                return Err(ProvenanceError::AttestationFailed {
                    reason:
                        "attestation is a bare self-claim (missing subject, anchor or verifier)"
                            .to_string(),
                });
            }
        }
        Ok(())
    }

    /// The canonical JSON rendering of the record (the one canonicalizer — CC1/CC7:
    /// `hh-wire` sorted-key compact JSON). Defaults are omitted (`taint`, `readers`,
    /// `derived_from`, `attestation` appear only when non-default — §8.1 #3).
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("origin", self.origin_json()),
            ("authority", Json::str(self.authority.as_str())),
            ("scope", Json::str(self.scope.as_str())),
            ("created_at", Json::Int(self.created_at as i64)),
        ];
        if !self.taint.is_empty() {
            pairs.push((
                "taint",
                Json::Arr(
                    self.taint
                        .iter()
                        .map(|t| Json::str(t.as_string()))
                        .collect(),
                ),
            ));
        }
        if let ReaderSet::Restricted(rs) = &self.readers {
            pairs.push((
                "readers",
                Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect()),
            ));
        }
        if !self.derived_from.is_empty() {
            pairs.push((
                "derived_from",
                Json::Arr(self.derived_from.iter().map(derivation_json).collect()),
            ));
        }
        if let Some(att) = &self.attestation {
            pairs.push(("attestation", attestation_json(att)));
        }
        Json::obj(pairs)
    }

    fn origin_json(&self) -> Json {
        let (kind, fields): (&'static str, Vec<(&'static str, Json)>) = match &self.origin {
            Origin::Human { author_ref, role } => (
                "human",
                vec![
                    ("author_ref", Json::str(author_ref.clone())),
                    ("role", Json::str(role.as_str())),
                ],
            ),
            Origin::Model {
                model_ref,
                run_ref,
                response_id,
            } => (
                "model",
                vec![
                    ("model_ref", Json::str(model_ref.clone())),
                    ("run_ref", Json::str(run_ref.clone())),
                    ("response_id", Json::str(response_id.clone())),
                ],
            ),
            Origin::Tool {
                capability,
                invocation_ref,
                inner_source,
            } => {
                let mut f = vec![
                    ("capability", Json::str(capability.clone())),
                    ("invocation_ref", Json::str(invocation_ref.clone())),
                ];
                if let Some(inner) = inner_source {
                    f.push(("inner_source", Json::str(inner.as_string())));
                }
                ("tool", f)
            }
            Origin::Evolution {
                candidate_id,
                hypothesis_ref,
            } => (
                "evolution",
                vec![
                    ("candidate_id", Json::str(candidate_id.clone())),
                    ("hypothesis_ref", Json::str(hypothesis_ref.clone())),
                ],
            ),
            Origin::Import {
                source_system,
                mapping_version,
            } => (
                "import",
                vec![
                    ("source_system", Json::str(source_system.clone())),
                    ("mapping_version", Json::str(mapping_version.clone())),
                ],
            ),
            Origin::Migration { from_dialect } => (
                "migration",
                vec![("from_dialect", Json::str(from_dialect.clone()))],
            ),
            Origin::Kernel { component_ref } => (
                "kernel",
                vec![("component_ref", Json::str(component_ref.clone()))],
            ),
            Origin::Participant {
                participant_ref,
                hosting_mechanism,
            } => (
                "participant",
                vec![
                    ("participant_ref", Json::str(participant_ref.clone())),
                    ("hosting_mechanism", Json::str(hosting_mechanism.clone())),
                ],
            ),
            Origin::Cache { entry_ref } => {
                ("cache", vec![("entry_ref", Json::str(entry_ref.clone()))])
            }
        };
        let mut fields = fields;
        fields.insert(0, ("kind", Json::str(kind)));
        Json::obj(fields)
    }
}

fn derivation_json(d: &Derivation) -> Json {
    Json::obj([
        ("kind", Json::str(d.kind.as_str())),
        (
            "inputs",
            Json::Arr(d.inputs.iter().map(|i| Json::str(i.clone())).collect()),
        ),
        ("deterministic", Json::Bool(d.deterministic)),
    ])
}

fn attestation_json(a: &Attestation) -> Json {
    let anchor = match &a.anchor {
        AttestationAnchor::Signer(s) => Json::obj([("signer", Json::str(s.clone()))]),
        AttestationAnchor::Chain(c) => Json::obj([("chain", Json::str(c.clone()))]),
    };
    Json::obj([
        ("kind", Json::str(a.kind.as_str())),
        ("subject_hash", Json::str(a.subject_hash.clone())),
        ("anchor", anchor),
        ("verified_by", Json::str(a.verified_by.clone())),
        ("verified_at", Json::Int(a.verified_at as i64)),
    ])
}

/// The `MissingProvenance` gate (§8.1 #5; AC-R-2.1.5-1): every HIR node/edge and every event
/// in the mandatory-provenance table ([`crate::mandatory`]) **carries** a `ProvenanceRecord`.
/// An append/validate path that finds none fails with `MissingProvenance` — never a warning,
/// never a default record minted silently.
pub fn require_provenance(
    record: Option<&ProvenanceRecord>,
    what: impl Into<String>,
) -> Result<&ProvenanceRecord, ProvenanceError> {
    match record {
        Some(r) => {
            r.validate(None)?;
            Ok(r)
        }
        None => Err(ProvenanceError::MissingProvenance { what: what.into() }),
    }
}

/// `verify_attestation(attestation, trusted) → verified | AttestationFailed` (§8.1 #2;
/// ADR-0033 D1; ADR-0035 D2): anchors on the signer/trust root or the chain head **supplied by
/// the caller's trust set** — never on the record's own claim (the in-toto/SLSA
/// signer–builder rule). `pin` requires `verified` under `TrustRootPolicy`; hash-only pins give
/// integrity and at most `hash_only_ceiling` (default `external`).
pub fn verify_attestation(
    attestation: &Attestation,
    trusted: &TrustedAnchors,
) -> Result<(), ProvenanceError> {
    if !attestation.self_consistent() {
        return Err(ProvenanceError::AttestationFailed {
            reason: "attestation is not a verification record".to_string(),
        });
    }
    let anchored = match &attestation.anchor {
        AttestationAnchor::Signer(s) => trusted.signers.contains(s),
        AttestationAnchor::Chain(c) => trusted.chain_heads.contains(c),
    };
    if !anchored {
        return Err(ProvenanceError::AttestationFailed {
            reason: "anchor is not in the trust set (no trust-root/chain-head match)".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::HumanRole;

    fn attested_seal() -> Attestation {
        Attestation {
            kind: AttestationKind::Seal,
            subject_hash: "sha256:x".into(),
            anchor: AttestationAnchor::Chain("sha256:head".into()),
            verified_by: "kernel:sealer".into(),
            verified_at: 7,
        }
    }

    #[test]
    fn minted_carries_the_derived_class() {
        let r =
            ProvenanceRecord::minted(Origin::model("m", "run", "resp"), PersistenceScope::Run, 3);
        assert_eq!(r.authority, AuthorityClass::Delegate);
        assert!(r.taint.is_empty());
        assert_eq!(r.readers, ReaderSet::Public);
        assert!(r.validate(None).is_ok());
    }

    #[test]
    fn tainted_above_external_is_refused() {
        let mut r = ProvenanceRecord::minted(Origin::kernel("k"), PersistenceScope::Run, 0);
        r.taint.insert(TaintTag::Import {
            source_system: "x".into(),
        });
        assert!(matches!(
            r.validate(None),
            Err(ProvenanceError::TaintedAboveExternal { .. })
        ));
        // At ≤ external taint is fine.
        r.authority = AuthorityClass::External;
        assert!(r.validate(None).is_ok());
    }

    #[test]
    fn authority_above_the_minted_ceiling_is_refused() {
        // A model-origin record claiming `definition` without endorsement evidence: refused.
        let mut r =
            ProvenanceRecord::minted(Origin::model("m", "run", "resp"), PersistenceScope::Run, 0);
        r.authority = AuthorityClass::Definition;
        assert!(matches!(
            r.validate(None),
            Err(ProvenanceError::AuthorityExceedsOrigin {
                ceiling: AuthorityClass::Delegate,
                claimed: AuthorityClass::Definition,
            })
        ));
        // A human-origin record sealed to `definition`: the seal attestation lifts the ceiling.
        let mut sealed = ProvenanceRecord::minted(
            Origin::human("author", HumanRole::Author),
            PersistenceScope::Definition,
            0,
        );
        sealed.attestation = Some(attested_seal());
        sealed.authority = AuthorityClass::Definition;
        assert!(sealed.validate(None).is_ok());
    }

    #[test]
    fn verify_attestation_anchors_on_the_trust_set_only() {
        let att = attested_seal();
        let mut trusted = TrustedAnchors::default();
        // No trust set membership → failed.
        assert!(verify_attestation(&att, &trusted).is_err());
        trusted.chain_heads.insert("sha256:head".to_string());
        assert!(verify_attestation(&att, &trusted).is_ok());
        // A self-claim never verifies.
        let bare = Attestation {
            verified_by: String::new(),
            ..attested_seal()
        };
        assert!(verify_attestation(&bare, &trusted).is_err());
    }

    #[test]
    fn require_provenance_fails_missing_and_validates_present() {
        // AC-R-2.1.5-1: an absent record is `MissingProvenance`; a present record is also
        // validated (a present-but-invalid record is not "carried").
        assert!(matches!(
            require_provenance(None, "security.permission.proposed"),
            Err(ProvenanceError::MissingProvenance { .. })
        ));
        let ok = ProvenanceRecord::kernel("kernel:ledger", 0);
        assert!(require_provenance(Some(&ok), "e").is_ok());
        let mut bad = ok.clone();
        bad.taint.insert(TaintTag::Import {
            source_system: "x".into(),
        });
        assert!(require_provenance(Some(&bad), "e").is_err());
    }

    #[test]
    fn canonical_json_omits_defaults() {
        let r = ProvenanceRecord::kernel("kernel:ledger", 5);
        let j = r.to_json();
        assert!(j.get("taint").is_none());
        assert!(j.get("readers").is_none());
        assert!(j.get("derived_from").is_none());
        assert!(j.get("attestation").is_none());
        assert_eq!(j.get("authority").and_then(Json::as_str), Some("kernel"));
        // Deterministic canonical bytes.
        assert_eq!(
            r.to_json().to_canonical_string(),
            r.to_json().to_canonical_string()
        );
    }
}
