//! The canonical **decoders** — the `from_json` half of the one provenance schema
//! (CC7: the schema source owns both directions). These mirror the `to_json` renderings
//! in [`crate::record`], [`crate::label`] and [`crate::endorse`] exactly: a decoded value
//! re-encodes byte-identically under the same canonicalizer (`hh-wire`).
//!
//! Added for S1.5 (the run ledger needs typed provenance out of stored envelopes, and the
//! monitor's append-time endorsement check — `check_endorsement` — needs a decoded
//! `security.label.endorsed` payload). `Derivation.deriver` is not on the wire (the
//! canonical derivation is `{kind, inputs, deterministic}`); the decoder restores a kernel
//! placeholder, which round-trips byte-identically because the field is never emitted —
//! the same convention `hh-hir`'s parser used.

use std::collections::BTreeSet;
use std::fmt;

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, PersistenceScope, ReaderSet, TaintTag};
use crate::endorse::{ContentKind, EndorsementBasis, LabelEndorsed};
use crate::label::Label;
use crate::origin::{HumanRole, Origin};
use crate::record::{
    Attestation, AttestationAnchor, AttestationKind, Derivation, DerivationKind, ProvenanceRecord,
};

/// A canonical-decode failure — a *shape* error, distinct from a [`crate::record::ProvenanceError`]
/// rule violation: the bytes could not be read as the record at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// The offending path/reason.
    pub detail: String,
}

impl DecodeError {
    fn new(detail: impl Into<String>) -> DecodeError {
        DecodeError {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provenance decode error: {}", self.detail)
    }
}

impl std::error::Error for DecodeError {}

fn req<'a>(j: &'a Json, key: &str, path: &str) -> Result<&'a Json, DecodeError> {
    j.get(key)
        .ok_or_else(|| DecodeError::new(format!("{path}.{key} missing")))
}

fn req_str(j: &Json, key: &str, path: &str) -> Result<String, DecodeError> {
    req(j, key, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| DecodeError::new(format!("{path}.{key} must be a string")))
}

fn opt_str(j: &Json, key: &str) -> Option<String> {
    j.get(key).and_then(Json::as_str).map(str::to_string)
}

fn req_int(j: &Json, key: &str, path: &str) -> Result<u64, DecodeError> {
    match req(j, key, path)?.as_int() {
        Some(i) if i >= 0 => Ok(i as u64),
        _ => Err(DecodeError::new(format!("{path}.{key} must be an int ≥ 0"))),
    }
}

fn req_bool(j: &Json, key: &str, path: &str) -> Result<bool, DecodeError> {
    match req(j, key, path)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(DecodeError::new(format!("{path}.{key} must be a bool"))),
    }
}

fn req_arr<'a>(j: &'a Json, key: &str, path: &str) -> Result<&'a [Json], DecodeError> {
    match req(j, key, path)? {
        Json::Arr(items) => Ok(items),
        _ => Err(DecodeError::new(format!("{path}.{key} must be an array"))),
    }
}

fn str_arr(items: &[Json], path: &str) -> Result<Vec<String>, DecodeError> {
    items
        .iter()
        .map(|i| {
            i.as_str()
                .map(str::to_string)
                .ok_or_else(|| DecodeError::new(format!("{path} members must be strings")))
        })
        .collect()
}

impl HumanRole {
    /// Parse the canonical spelling (`author` / `principal` / `reviewer`).
    pub fn parse(s: &str) -> Option<HumanRole> {
        match s {
            "author" => Some(HumanRole::Author),
            "principal" => Some(HumanRole::Principal),
            "reviewer" => Some(HumanRole::Reviewer),
            _ => None,
        }
    }
}

impl PersistenceScope {
    /// Parse the canonical spelling (`definition` / `user` / `project` / `session` /
    /// `run` / `turn`).
    pub fn parse(s: &str) -> Option<PersistenceScope> {
        match s {
            "definition" => Some(PersistenceScope::Definition),
            "user" => Some(PersistenceScope::User),
            "project" => Some(PersistenceScope::Project),
            "session" => Some(PersistenceScope::Session),
            "run" => Some(PersistenceScope::Run),
            "turn" => Some(PersistenceScope::Turn),
            _ => None,
        }
    }
}

impl DerivationKind {
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DerivationKind> {
        Some(match s {
            "summary" => DerivationKind::Summary,
            "compaction" => DerivationKind::Compaction,
            "extraction" => DerivationKind::Extraction,
            "quotation" => DerivationKind::Quotation,
            "revision" => DerivationKind::Revision,
            "translation" => DerivationKind::Translation,
            "subagent_result" => DerivationKind::SubagentResult,
            "projection" => DerivationKind::Projection,
            _ => return None,
        })
    }
}

impl AttestationKind {
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<AttestationKind> {
        Some(match s {
            "seal" => AttestationKind::Seal,
            "hash_chain" => AttestationKind::HashChain,
            "signature" => AttestationKind::Signature,
            "pin" => AttestationKind::Pin,
            _ => return None,
        })
    }
}

impl EndorsementBasis {
    /// Parse the canonical spelling (the closed list — a new basis is a dialect bump).
    pub fn parse(s: &str) -> Option<EndorsementBasis> {
        Some(match s {
            "seal" => EndorsementBasis::Seal,
            "approval" => EndorsementBasis::Approval,
            "validator" => EndorsementBasis::Validator,
            "promotion" => EndorsementBasis::Promotion,
            "pin" => EndorsementBasis::Pin,
            "policy_rule" => EndorsementBasis::PolicyRule,
            _ => return None,
        })
    }
}

impl ContentKind {
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ContentKind> {
        Some(match s {
            "free_text" => ContentKind::FreeText,
            "closed_schema_value" => ContentKind::ClosedSchemaValue,
            "effect_intent" => ContentKind::EffectIntent,
            "other" => ContentKind::Other,
            _ => return None,
        })
    }

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContentKind::FreeText => "free_text",
            ContentKind::ClosedSchemaValue => "closed_schema_value",
            ContentKind::EffectIntent => "effect_intent",
            ContentKind::Other => "other",
        }
    }
}

impl TaintTag {
    /// Parse the `as_string` form (`tool:<cap>[+<inner>]` / `participant:<p>` /
    /// `import:<sys>` / `extension:<id>`).
    pub fn parse(s: &str) -> Option<TaintTag> {
        if let Some(rest) = s.strip_prefix("tool:") {
            let (cap, inner) = match rest.split_once('+') {
                Some((c, i)) => (c.to_string(), Some(i.to_string())),
                None => (rest.to_string(), None),
            };
            Some(TaintTag::Tool {
                capability: cap,
                inner_source: inner,
            })
        } else if let Some(p) = s.strip_prefix("participant:") {
            Some(TaintTag::Participant {
                participant: p.to_string(),
            })
        } else if let Some(sys) = s.strip_prefix("import:") {
            Some(TaintTag::Import {
                source_system: sys.to_string(),
            })
        } else {
            s.strip_prefix("extension:").map(|e| TaintTag::Extension {
                extension_id: e.to_string(),
            })
        }
    }
}

impl Origin {
    /// Decode the canonical `{kind, …}` rendering emitted by `ProvenanceRecord::to_json`.
    pub fn from_json(j: &Json) -> Result<Origin, DecodeError> {
        let kind = req_str(j, "kind", "origin")?;
        let s = |k: &str| req_str(j, k, "origin");
        match kind.as_str() {
            "kernel" => Ok(Origin::kernel(s("component_ref")?)),
            "human" => {
                let role = HumanRole::parse(&s("role")?)
                    .ok_or_else(|| DecodeError::new("origin.role unknown"))?;
                Ok(Origin::human(s("author_ref")?, role))
            }
            "model" => Ok(Origin::model(
                s("model_ref")?,
                s("run_ref")?,
                s("response_id")?,
            )),
            "tool" => {
                let inner = opt_str(j, "inner_source")
                    .map(|i| {
                        TaintTag::parse(&i)
                            .map(Box::new)
                            .ok_or_else(|| DecodeError::new("origin.inner_source unparseable"))
                    })
                    .transpose()?;
                Ok(Origin::Tool {
                    capability: s("capability")?,
                    invocation_ref: s("invocation_ref")?,
                    inner_source: inner,
                })
            }
            "evolution" => Ok(Origin::evolution(s("candidate_id")?, s("hypothesis_ref")?)),
            "import" => Ok(Origin::import(s("source_system")?, s("mapping_version")?)),
            "migration" => Ok(Origin::Migration {
                from_dialect: s("from_dialect")?,
            }),
            "participant" => Ok(Origin::participant(
                s("participant_ref")?,
                s("hosting_mechanism")?,
            )),
            other => Err(DecodeError::new(format!("origin.kind {other} unknown"))),
        }
    }
}

impl Derivation {
    /// Decode `{kind, inputs, deterministic}`; `deriver` is not on the wire — a kernel
    /// placeholder is restored (round-trips byte-identically; never emitted).
    pub fn from_json(j: &Json) -> Result<Derivation, DecodeError> {
        Ok(Derivation {
            kind: DerivationKind::parse(&req_str(j, "kind", "derived_from")?)
                .ok_or_else(|| DecodeError::new("derived_from.kind unknown"))?,
            inputs: str_arr(req_arr(j, "inputs", "derived_from")?, "derived_from.inputs")?,
            deriver: Origin::kernel("hh-provenance:decode"),
            deterministic: req_bool(j, "deterministic", "derived_from")?,
        })
    }
}

impl Attestation {
    /// Decode `{kind, subject_hash, anchor: {signer|chain}, verified_by, verified_at}`.
    pub fn from_json(j: &Json) -> Result<Attestation, DecodeError> {
        let kind = AttestationKind::parse(&req_str(j, "kind", "attestation")?)
            .ok_or_else(|| DecodeError::new("attestation.kind unknown"))?;
        let anchor_j = req(j, "anchor", "attestation")?;
        let anchor = if let Some(s) = anchor_j.get("signer").and_then(Json::as_str) {
            AttestationAnchor::Signer(s.to_string())
        } else if let Some(c) = anchor_j.get("chain").and_then(Json::as_str) {
            AttestationAnchor::Chain(c.to_string())
        } else {
            return Err(DecodeError::new("attestation.anchor must be signer|chain"));
        };
        Ok(Attestation {
            kind,
            subject_hash: req_str(j, "subject_hash", "attestation")?,
            anchor,
            verified_by: req_str(j, "verified_by", "attestation")?,
            verified_at: req_int(j, "verified_at", "attestation")?,
        })
    }
}

fn readers_from_json(j: Option<&Json>, path: &str) -> Result<ReaderSet, DecodeError> {
    match j {
        None => Ok(ReaderSet::Public),
        Some(Json::Arr(items)) => Ok(ReaderSet::Restricted(
            str_arr(items, path)?.into_iter().collect(),
        )),
        Some(_) => Err(DecodeError::new(format!("{path} must be an array"))),
    }
}

impl ProvenanceRecord {
    /// Decode the canonical record emitted by [`ProvenanceRecord::to_json`]. Defaults
    /// absent from the wire restore their defaults (`taint`/`derived_from` = ∅,
    /// `readers` = Public, `attestation` = none).
    pub fn from_json(j: &Json) -> Result<ProvenanceRecord, DecodeError> {
        let origin = Origin::from_json(req(j, "origin", "record")?)?;
        let authority = AuthorityClass::parse(&req_str(j, "authority", "record")?)
            .ok_or_else(|| DecodeError::new("record.authority unknown"))?;
        let scope = PersistenceScope::parse(&req_str(j, "scope", "record")?)
            .ok_or_else(|| DecodeError::new("record.scope unknown"))?;
        let created_at = req_int(j, "created_at", "record")?;
        let taint = match j.get("taint") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|t| {
                    t.as_str()
                        .and_then(TaintTag::parse)
                        .ok_or_else(|| DecodeError::new("record.taint member unparseable"))
                })
                .collect::<Result<BTreeSet<_>, _>>()?,
            Some(_) => return Err(DecodeError::new("record.taint must be an array")),
            None => BTreeSet::new(),
        };
        let readers = readers_from_json(j.get("readers"), "record.readers")?;
        let derived_from = match j.get("derived_from") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(Derivation::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(DecodeError::new("record.derived_from must be an array")),
            None => Vec::new(),
        };
        let attestation = match j.get("attestation") {
            Some(a) => Some(Attestation::from_json(a)?),
            None => None,
        };
        Ok(ProvenanceRecord {
            origin,
            authority,
            taint,
            readers,
            scope,
            derived_from,
            created_at,
            attestation,
        })
    }
}

impl Label {
    /// Decode `{authority, taint?, readers?}` — the label rendering `endorse` payloads use.
    pub fn from_json(j: &Json) -> Result<Label, DecodeError> {
        let authority = AuthorityClass::parse(&req_str(j, "authority", "label")?)
            .ok_or_else(|| DecodeError::new("label.authority unknown"))?;
        let taint = match j.get("taint") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|t| {
                    t.as_str()
                        .and_then(TaintTag::parse)
                        .ok_or_else(|| DecodeError::new("label.taint member unparseable"))
                })
                .collect::<Result<BTreeSet<_>, _>>()?,
            Some(_) => return Err(DecodeError::new("label.taint must be an array")),
            None => BTreeSet::new(),
        };
        let readers = readers_from_json(j.get("readers"), "label.readers")?;
        Ok(Label {
            authority,
            taint,
            readers,
        })
    }
}

impl LabelEndorsed {
    /// Decode the documented `security.label.endorsed` payload `{subject_ref, from, to,
    /// endorser, basis, basis_ref?}` (§8.1 #3). The ledger's append-time monitor check-5
    /// ([`crate::endorse::check_endorsement`]) consumes this.
    pub fn from_json(j: &Json) -> Result<LabelEndorsed, DecodeError> {
        Ok(LabelEndorsed {
            subject_ref: req_str(j, "subject_ref", "label_endorsed")?,
            from: Label::from_json(req(j, "from", "label_endorsed")?)?,
            to: Label::from_json(req(j, "to", "label_endorsed")?)?,
            endorser: ProvenanceRecord::from_json(req(j, "endorser", "label_endorsed")?)?,
            basis: EndorsementBasis::parse(&req_str(j, "basis", "label_endorsed")?)
                .ok_or_else(|| DecodeError::new("label_endorsed.basis unknown"))?,
            basis_ref: opt_str(j, "basis_ref"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endorse::EndorsementBasis;

    #[test]
    fn record_round_trips_byte_identically() {
        let mut r = ProvenanceRecord::minted(
            Origin::model("m1", "run-1", "resp-9"),
            PersistenceScope::Run,
            7,
        );
        r.taint.insert(TaintTag::Import {
            source_system: "sys".into(),
        });
        r.authority = AuthorityClass::External;
        r.derived_from.push(Derivation {
            kind: DerivationKind::Summary,
            inputs: vec!["sha256:in".into()],
            deriver: Origin::model("m1", "run-1", "resp-9"),
            deterministic: false,
        });
        let j = r.to_json();
        let back = ProvenanceRecord::from_json(&j).unwrap();
        // The deriver placeholder is never on the wire — the re-encode is byte-identical.
        assert_eq!(j, back.to_json());
        assert_eq!(
            j.to_canonical_string(),
            back.to_json().to_canonical_string()
        );
    }

    #[test]
    fn attested_record_round_trips() {
        let r = ProvenanceRecord::minted_attested(
            Origin::human("alice", HumanRole::Author),
            PersistenceScope::Definition,
            3,
            Attestation {
                kind: AttestationKind::Seal,
                subject_hash: "sha256:s".into(),
                anchor: AttestationAnchor::Chain("sha256:head".into()),
                verified_by: "kernel:sealer".into(),
                verified_at: 3,
            },
        );
        let j = r.to_json();
        let back = ProvenanceRecord::from_json(&j).unwrap();
        assert_eq!(back, r);
        assert_eq!(j, back.to_json());
    }

    #[test]
    fn bad_kind_is_a_typed_decode_error() {
        let j = Json::obj([("kind", Json::str("nope"))]);
        assert!(Origin::from_json(&j).is_err());
    }

    #[test]
    fn label_endorsed_payload_decodes() {
        let endorser = ProvenanceRecord::kernel("kernel:monitor", 4);
        let ev = LabelEndorsed {
            subject_ref: "sha256:s".into(),
            from: Label::at(AuthorityClass::External),
            to: Label::at(AuthorityClass::Definition),
            endorser: endorser.clone(),
            basis: EndorsementBasis::Seal,
            basis_ref: Some("sha256:perm".into()),
        };
        let j = Json::obj([
            ("subject_ref", Json::str(ev.subject_ref.clone())),
            (
                "from",
                Json::Obj({
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("authority".to_string(), Json::str("external"));
                    m
                }),
            ),
            (
                "to",
                Json::Obj({
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("authority".to_string(), Json::str("definition"));
                    m
                }),
            ),
            ("endorser", endorser.to_json()),
            ("basis", Json::str("seal")),
            ("basis_ref", Json::str("sha256:perm")),
        ]);
        let back = LabelEndorsed::from_json(&j).unwrap();
        assert_eq!(back.basis, EndorsementBasis::Seal);
        assert_eq!(back.endorser.authority, AuthorityClass::Kernel);
    }
}
