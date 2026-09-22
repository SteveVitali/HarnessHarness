//! The two **opaque leaf kinds** (§3.1.2; ADR-0015 decision 3): `Text` and
//! `CompiledPayload`. These are the *only* opaque constructs in HIR/1 — a leaf is legal only
//! as (O1) the body of something whose interface is declared, (O2) never a decider of
//! authority/budget/validity, (O3) individually addressable. Anything else string-shaped is
//! `OpaqueWithoutInterface`/`UnexpressibleSurface`, not merely opaque.
//!
//! Both leaves are **content-addressed**: the canonical form carries the content hash, never
//! the bytes (the WS-A5 pointer rule at type level). `Text.content` is carried in memory for
//! rendering but is absent from the canonical form — which is what makes `ReplaceLeaf`
//! (hash→hash) sufficient for `apply` (§3.1.5).

use hh_identity::idp::identify_text;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

use crate::errors::HirError;

/// A `Text` leaf — the **only** construct for model-facing prose (instructions, tool
/// `purpose`, rubrics, memory content; §3.1.2). `content` is the raw prose held in memory;
/// the canonical/semantic form carries only `content_hash` (hash-addressed — the runtime
/// resolves content from the content-addressed store by hash). `provenance` is mandatory
/// (§3.1.4: "provenance on every `Text` leaf"); `owner`/`authority` are semantic fields that
/// feed the role-placement invariant (§3.1.11; ADR-0034).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    /// The raw prose — `None` for a hash-addressed leaf parsed from canonical form.
    pub content: Option<String>,
    /// `identify_text(content)` — `sha256:<hex>` under the `hir.text` domain. Always present.
    pub content_hash: String,
    /// The owner (an identity coordinate — who owns this prose).
    pub owner: String,
    /// The text's authority (R-TEXT: minted ≤ `external` unless kernel/definition/principal).
    pub authority: AuthorityClass,
    /// The leaf's own provenance record (mandatory — §3.1.4).
    pub provenance: ProvenanceRecord,
    /// The language tag, when the prose has one.
    pub language: Option<String>,
}

impl Text {
    /// A text leaf from raw content: the content hash is computed by the one idp/1 scheme
    /// (`identify_text` — CC1); `provenance` is supplied by the author (never defaulted).
    pub fn new(
        content: impl Into<String>,
        owner: impl Into<String>,
        provenance: ProvenanceRecord,
    ) -> Text {
        let content = content.into();
        Text {
            content_hash: identify_text(content.as_bytes()),
            content: Some(content),
            owner: owner.into(),
            authority: provenance.authority,
            provenance,
            language: None,
        }
    }

    /// A hash-addressed leaf (no raw content — the form `apply`/`from_json` produce).
    pub fn addressed(
        content_hash: impl Into<String>,
        owner: impl Into<String>,
        authority: AuthorityClass,
        provenance: ProvenanceRecord,
    ) -> Text {
        Text {
            content: None,
            content_hash: content_hash.into(),
            owner: owner.into(),
            authority,
            provenance,
            language: None,
        }
    }

    /// The canonical JSON — `content_hash`, never raw `content`.
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("content_hash", Json::str(self.content_hash.clone())),
            ("owner", Json::str(self.owner.clone())),
            ("authority", Json::str(self.authority.as_str())),
            ("provenance", self.provenance.to_json()),
        ];
        if let Some(l) = &self.language {
            pairs.push(("language", Json::str(l.clone())));
        }
        Json::obj(pairs)
    }

    /// The semantic-projection form: the content hash plus the semantic fields, provenance
    /// excluded (§3.1.2 — surface, provenance, ext, version records are out of `semantic_id`).
    pub fn semantic_json(&self) -> Json {
        let mut pairs = vec![
            ("content_hash", Json::str(self.content_hash.clone())),
            ("owner", Json::str(self.owner.clone())),
            ("authority", Json::str(self.authority.as_str())),
        ];
        if let Some(l) = &self.language {
            pairs.push(("language", Json::str(l.clone())));
        }
        Json::obj(pairs)
    }

    /// Parse a `Text` leaf from its canonical form. Missing `provenance` is
    /// [`HirError::MissingProvenance`] (DF-S1.3-1; §3.1.4).
    pub fn from_json(j: &Json, path: &str) -> Result<Text, HirError> {
        let get = |k: &str| -> Result<&Json, HirError> {
            j.get(k).ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.{k} missing"),
            })
        };
        let content_hash = get("content_hash")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.content_hash must be a string"),
            })?
            .to_string();
        let owner = get("owner")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.owner must be a string"),
            })?
            .to_string();
        let authority = get("authority")?
            .as_str()
            .and_then(hh_provenance::AuthorityClass::parse)
            .ok_or_else(|| HirError::UnknownKind {
                kind: format!("{path}.authority: bad AuthorityClass"),
            })?;
        let prov = j
            .get("provenance")
            .ok_or_else(|| HirError::MissingProvenance {
                what: format!("{path} (Text leaf)"),
            })?;
        let provenance = crate::schema::provenance_from_json(prov, path)?;
        let language = j.get("language").and_then(Json::as_str).map(str::to_string);
        Ok(Text {
            content: None,
            content_hash,
            owner,
            authority,
            provenance,
            language,
        })
    }

    /// The R-TEXT check (§3.1.4): a `Text` leaf claiming `authority > external` whose origin
    /// cannot confer it is `TextAboveExternal`. Kernel and human origins pass through
    /// (`kernel`, `principal`); `definition` text is reached only via `seal` (a `seal`/`pin`
    /// attestation in the record).
    pub fn check_authority(&self, path: &str) -> Result<(), HirError> {
        if self.authority <= AuthorityClass::External {
            return Ok(());
        }
        let ok = match &self.provenance.origin {
            Origin::Kernel { .. } | Origin::Human { .. } => true,
            _ => self.provenance.attestation.as_ref().is_some_and(|a| {
                a.self_consistent()
                    && matches!(
                        a.kind,
                        hh_provenance::AttestationKind::Seal | hh_provenance::AttestationKind::Pin
                    )
            }),
        };
        if ok {
            Ok(())
        } else {
            Err(HirError::TextAboveExternal {
                authority: format!("{path}: {}", self.authority.as_str()),
            })
        }
    }

    /// Consistency: a leaf carrying raw content must hash to `content_hash`.
    pub fn content_consistent(&self) -> bool {
        match &self.content {
            Some(c) => identify_text(c.as_bytes()) == self.content_hash,
            None => true,
        }
    }
}

/// The declared interface every `CompiledPayload` carries (O1: opacity is legal only as the
/// body of a declared interface): `{inputs, outputs, effects: set<EffectClass>,
/// deterministic, target}` (§3.1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredInterface {
    /// The input schema (a structured schema value — owned by §3.3/§5b; carried as a
    /// structured record).
    pub inputs: Json,
    /// The output schema.
    pub outputs: Json,
    /// The effects the payload may produce (empty = pure).
    pub effects: std::collections::BTreeSet<crate::kinds::EffectClass>,
    /// Whether the payload is deterministic.
    pub deterministic: bool,
    /// The lowering target the payload is written for.
    pub target: String,
}

/// A `CompiledPayload` leaf — the **only** construct for executable bodies (variant
/// implementations, `Opaque` steps, executable validators, `parse` grammars; §3.1.2).
/// `bytes_hash` is a content-addressed pointer, never bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPayload {
    /// The payload format tag.
    pub format_tag: String,
    /// The content address of the payload bytes (`sha256:<hex>` under `blob`).
    pub bytes_hash: String,
    /// The declared interface (mandatory — `OpaqueWithoutInterface` otherwise).
    pub declared_interface: Option<DeclaredInterface>,
    /// The owner (identity coordinate).
    pub owner: String,
    /// The leaf's provenance record (mandatory).
    pub provenance: ProvenanceRecord,
}

impl CompiledPayload {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("format_tag", Json::str(self.format_tag.clone())),
            ("bytes_hash", Json::str(self.bytes_hash.clone())),
            ("owner", Json::str(self.owner.clone())),
            ("provenance", self.provenance.to_json()),
        ];
        if let Some(i) = &self.declared_interface {
            pairs.push(("declared_interface", interface_json(i)));
        }
        Json::obj(pairs)
    }

    /// The semantic-projection form (provenance excluded — §3.1.2).
    pub fn semantic_json(&self) -> Json {
        let mut pairs = vec![
            ("format_tag", Json::str(self.format_tag.clone())),
            ("bytes_hash", Json::str(self.bytes_hash.clone())),
            ("owner", Json::str(self.owner.clone())),
        ];
        if let Some(i) = &self.declared_interface {
            pairs.push(("declared_interface", interface_json(i)));
        }
        Json::obj(pairs)
    }

    /// Parse from canonical form. Missing `provenance` → [`HirError::MissingProvenance`].
    pub fn from_json(j: &Json, path: &str) -> Result<CompiledPayload, HirError> {
        let get = |k: &str| -> Result<&Json, HirError> {
            j.get(k).ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.{k} missing"),
            })
        };
        let format_tag = get("format_tag")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.format_tag must be a string"),
            })?
            .to_string();
        let bytes_hash = get("bytes_hash")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.bytes_hash must be a string"),
            })?
            .to_string();
        let owner = get("owner")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.owner must be a string"),
            })?
            .to_string();
        let prov = j
            .get("provenance")
            .ok_or_else(|| HirError::MissingProvenance {
                what: format!("{path} (CompiledPayload leaf)"),
            })?;
        let provenance = crate::schema::provenance_from_json(prov, path)?;
        let declared_interface = match j.get("declared_interface") {
            Some(i) => Some(interface_from_json(
                i,
                &format!("{path}.declared_interface"),
            )?),
            None => None,
        };
        Ok(CompiledPayload {
            format_tag,
            bytes_hash,
            declared_interface,
            owner,
            provenance,
        })
    }
}

fn interface_json(i: &DeclaredInterface) -> Json {
    Json::obj([
        ("inputs", i.inputs.clone()),
        ("outputs", i.outputs.clone()),
        (
            "effects",
            Json::Arr(
                i.effects
                    .iter()
                    .map(crate::kinds::EffectClass::to_json)
                    .collect(),
            ),
        ),
        ("deterministic", Json::Bool(i.deterministic)),
        ("target", Json::str(i.target.clone())),
    ])
}

fn interface_from_json(j: &Json, path: &str) -> Result<DeclaredInterface, HirError> {
    let get = |k: &str| -> Result<&Json, HirError> {
        j.get(k).ok_or_else(|| HirError::SchemaViolation {
            detail: format!("{path}.{k} missing"),
        })
    };
    let effects = match get("effects")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(n, e)| crate::kinds::EffectClass::from_json(e, &format!("{path}.effects[{n}]")))
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.effects must be an array"),
            })
        }
    };
    Ok(DeclaredInterface {
        inputs: get("inputs")?.clone(),
        outputs: get("outputs")?.clone(),
        effects,
        deterministic: matches!(get("deterministic")?, Json::Bool(true)),
        target: get("target")?
            .as_str()
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.target must be a string"),
            })?
            .to_string(),
    })
}

/// The minting helper for `Text` leaves (R-TEXT): mint the leaf's record through
/// `default_text_authority`, never bare `default_authority` — free text is capped at
/// `external` regardless of a closed-world declaration (DF-S1.3-3).
pub fn mint_text_provenance(origin: Origin, created_at: u64) -> ProvenanceRecord {
    let authority = hh_provenance::default_text_authority(&origin, None);
    let mut rec = ProvenanceRecord::minted(origin, PersistenceScope::Run, created_at);
    rec.authority = authority;
    rec
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human_prov() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::human("author:a", hh_provenance::HumanRole::Author),
            PersistenceScope::Definition,
            0,
        )
    }

    #[test]
    fn text_canonical_form_carries_hash_not_content() {
        // The pointer rule at type level: canonical form holds `content_hash`, never bytes.
        let t = Text::new("do the thing", "owner:o", human_prov());
        let j = t.to_json().to_canonical_string();
        assert!(j.contains("content_hash"));
        assert!(!j.contains("do the thing"));
    }

    #[test]
    fn text_above_external_is_refused_for_low_origins() {
        // R-TEXT / §3.1.4: a tool-origin Text claiming `definition` fails TextAboveExternal.
        let mut t = Text::new(
            "x",
            "o",
            ProvenanceRecord::minted(Origin::tool("tool:t", "inv:1"), PersistenceScope::Run, 0),
        );
        assert!(t.check_authority("t").is_ok()); // external is fine
        t.authority = AuthorityClass::Principal;
        assert!(matches!(
            t.check_authority("t"),
            Err(HirError::TextAboveExternal { .. })
        ));
    }

    #[test]
    fn compiled_payload_round_trips_canonical() {
        let p = CompiledPayload {
            format_tag: "e1/tool".into(),
            bytes_hash: "sha256:".to_string() + &"a".repeat(64),
            declared_interface: Some(DeclaredInterface {
                inputs: Json::obj([("args", Json::str("schema"))]),
                outputs: Json::obj([("result", Json::str("schema"))]),
                effects: Default::default(),
                deterministic: true,
                target: "kernel".into(),
            }),
            owner: "owner:o".into(),
            provenance: human_prov(),
        };
        let parsed = CompiledPayload::from_json(&p.to_json(), "p").unwrap();
        assert_eq!(parsed.to_json(), p.to_json());
    }
}
