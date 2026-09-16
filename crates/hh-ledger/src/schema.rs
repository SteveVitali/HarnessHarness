//! The envelope's canonical form and the schema gate (§5a.1 §3/§5; ADR-0029 §5; AC-8):
//!
//! - `SCHEMA_VERSION` — the current envelope schema (`1`); stamped on every event.
//! - [`EventEnvelope::to_json`] / [`EventEnvelope::from_json`] — the canonical encoding;
//!   decode accepts every version `≤ SCHEMA_VERSION` (the v1 loader is the identity),
//!   refuses a newer dialect with `SchemaMismatch`, refuses unknown members with
//!   `SchemaViolation`.
//! - [`DEPRECATIONS`] — the recorded-deprecation registry: a field removal requires a
//!   row here first (AC-8's "recorded deprecation" hook); empty at v1.
//! - [`event_hash`] — `hash = H(leaf_tag ∥ canonical(envelope − {hash}) ∥ prev_hash)`
//!   under `idp/1` domain `ledger.event` (the one hasher — CC1; leaf byte `0x00` is the
//!   RFC-6962 leaf/domain separator; the node side arrives with the audit tree heads).

use hh_identity::idp::idp_id;
use hh_identity::idp::ContentAddress;
use hh_identity::rowkeys::IrRef;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::classes::Durability;
use crate::errors::LedgerError;
use crate::event::{EphemeralRecord, EventEnvelope, EventPlane, Producer, Scope};
use crate::manifest::{EventRef, ObservabilityLevel, ParticipantClass};

/// The current envelope schema version.
pub const SCHEMA_VERSION: u64 = 1;

/// The idp/1 domain tag for the per-run hash chain.
pub const EVENT_HASH_DOMAIN: &str = "ledger.event";

/// The idp/1 domain tag for view hashes.
pub const VIEW_HASH_DOMAIN: &str = "ledger.view";

/// The RFC-6962 leaf separator byte inside the event-hash preimage.
pub const LEAF_TAG: u8 = 0x00;

/// A recorded field deprecation — `field`, the version it was deprecated at, the
/// version it may be removed at. Removing a field without a row here is a
/// `SchemaViolation` by construction (the decoder refuses unknown members, so a
/// removed-but-undeprecated field can never load).
pub struct FieldDeprecation {
    /// The envelope member.
    pub field: &'static str,
    /// The schema version that deprecated it.
    pub deprecated_in: u64,
    /// The earliest version that may drop it.
    pub removable_in: u64,
}

/// The deprecation registry — empty at schema v1 (nothing predates it).
pub const DEPRECATIONS: &[FieldDeprecation] = &[];

/// `hash = H(leaf_tag ∥ canonical(envelope − {hash}) ∥ prev_hash)` — rendered as an
/// idp/1 id (`sha256:<hex>`) under the `ledger.event` domain.
pub fn event_hash(envelope_without_hash: &Json, prev_hash: &str) -> String {
    let mut preimage = Vec::with_capacity(256);
    preimage.push(LEAF_TAG);
    preimage.extend_from_slice(envelope_without_hash.to_canonical_string().as_bytes());
    preimage.extend_from_slice(prev_hash.as_bytes());
    idp_id(EVENT_HASH_DOMAIN, &preimage)
}

fn envelope_json(e: &EventEnvelope, with_hash: bool) -> Json {
    let mut m: BTreeMap<String, Json> = BTreeMap::new();
    let mut put = |k: &str, v: Json| {
        m.insert(k.to_string(), v);
    };
    put("event_id", Json::str(&e.event_id));
    put("run_id", Json::str(&e.run_id));
    put("seq", Json::Int(e.seq as i64));
    put("ts", Json::str(&e.ts));
    if let Some(h) = &e.hlc {
        put("hlc", Json::str(h));
    }
    put("plane", Json::str(e.plane.as_str()));
    put("class", Json::str(&e.class));
    put("schema_version", Json::Int(e.schema_version as i64));
    put(
        "producer",
        Json::Obj(BTreeMap::from([
            (
                "component_class".to_string(),
                Json::str(&e.producer.component_class),
            ),
            (
                "component_variant_ref".to_string(),
                Json::str(&e.producer.component_variant_ref),
            ),
            (
                "participant_ref".to_string(),
                Json::str(&e.producer.participant_ref),
            ),
        ])),
    );
    put("participant_class", Json::str(e.participant_class.as_str()));
    put(
        "observability_level",
        Json::Arr(
            e.observability_level
                .iter()
                .map(|l| Json::str(l.as_str()))
                .collect(),
        ),
    );
    put("durability", Json::str(e.durability.as_str()));
    put("scope", scope_json(&e.scope));
    put("lease_generation", Json::Int(e.lease_generation as i64));
    put("parent_event_id", Json::str(&e.parent_event_id));
    put(
        "causes",
        Json::Arr(e.causes.iter().map(event_ref_json).collect()),
    );
    put(
        "refs",
        Json::Arr(
            e.refs
                .iter()
                .map(|a| {
                    Json::obj([
                        ("id", Json::str(a.id())),
                        ("media_type", Json::str(&a.media_type)),
                        ("size", Json::Int(a.size as i64)),
                    ])
                })
                .collect(),
        ),
    );
    put(
        "ir_refs",
        Json::Arr(
            e.ir_refs
                .iter()
                .map(|r| {
                    let mut m = BTreeMap::new();
                    m.insert("version_id".to_string(), Json::str(&r.version_id));
                    if let Some(s) = &r.semantic_id {
                        m.insert("semantic_id".to_string(), Json::str(s));
                    }
                    Json::Obj(m)
                })
                .collect(),
        ),
    );
    if !e.surface_ids.is_empty() {
        put(
            "surface_ids",
            Json::Obj(
                e.surface_ids
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v)))
                    .collect(),
            ),
        );
    }
    if let Some(p) = &e.provenance {
        put("provenance", p.to_json());
    }
    put("prev_hash", Json::str(&e.prev_hash));
    put("payload", e.payload.clone());
    if with_hash {
        put("hash", Json::str(&e.hash));
    }
    Json::Obj(m)
}

impl EventEnvelope {
    /// The canonical encoding (with `hash`).
    pub fn to_json(&self) -> Json {
        envelope_json(self, true)
    }

    /// The hash preimage — canonical bytes of `envelope − {hash}`.
    pub fn preimage_json(&self) -> Json {
        envelope_json(self, false)
    }

    /// Recompute this envelope's `hash` (the `verify` check).
    pub fn recompute_hash(&self) -> String {
        event_hash(&self.preimage_json(), &self.prev_hash)
    }

    /// The stored canonical bytes.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }

    /// Decode an envelope from its canonical form (the `read`/replay/`verify` path).
    ///
    /// The schema gate (AC-8): `schema_version > SCHEMA_VERSION` ⇒ `SchemaMismatch`;
    /// an unknown member ⇒ `SchemaViolation`; a missing required member ⇒
    /// `SchemaViolation`.
    pub fn from_json(j: &Json) -> Result<EventEnvelope, LedgerError> {
        let bad = |d: String| LedgerError::SchemaViolation { detail: d };
        if let Json::Obj(m) = j {
            for k in m.keys() {
                if !ENVELOPE_FIELDS.contains(&k.as_str()) {
                    return Err(bad(format!("unknown envelope member {k}")));
                }
            }
        } else {
            return Err(bad("envelope must be an object".into()));
        }
        let schema_version = match j.get("schema_version").and_then(Json::as_int) {
            Some(v) if v >= 0 => v as u64,
            _ => return Err(bad("schema_version missing/not an int".into())),
        };
        if schema_version > SCHEMA_VERSION {
            return Err(LedgerError::SchemaMismatch {
                found: schema_version,
                known_max: SCHEMA_VERSION,
            });
        }
        let req_str = |k: &str| -> Result<String, LedgerError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad(format!("{k} missing/not a string")))
        };
        let req_int = |k: &str| -> Result<u64, LedgerError> {
            match j.get(k).and_then(Json::as_int) {
                Some(i) if i >= 0 => Ok(i as u64),
                _ => Err(bad(format!("{k} missing/not an int ≥ 0"))),
            }
        };
        let producer_j = j
            .get("producer")
            .ok_or_else(|| bad("producer missing".into()))?;
        let producer = Producer {
            component_class: producer_j
                .get("component_class")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad("producer.component_class missing".into()))?,
            component_variant_ref: producer_j
                .get("component_variant_ref")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad("producer.component_variant_ref missing".into()))?,
            participant_ref: producer_j
                .get("participant_ref")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad("producer.participant_ref missing".into()))?,
        };
        let participant_class = ParticipantClass::parse(&req_str("participant_class")?)
            .ok_or_else(|| bad("participant_class unknown".into()))?;
        let observability_level = match j.get("observability_level") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .and_then(ObservabilityLevel::parse)
                        .ok_or_else(|| bad("observability_level member unknown".into()))
                })
                .collect::<Result<BTreeSet<_>, _>>()?,
            _ => return Err(bad("observability_level missing/not an array".into())),
        };
        let plane =
            EventPlane::parse(&req_str("plane")?).ok_or_else(|| bad("plane unknown".into()))?;
        let durability = Durability::parse(&req_str("durability")?)
            .ok_or_else(|| bad("durability unknown".into()))?;
        let scope = scope_from_json(j.get("scope").ok_or_else(|| bad("scope missing".into()))?)?;
        let causes = match j.get("causes") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(event_ref_from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(bad("causes missing/not an array".into())),
        };
        let refs = match j.get("refs") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| {
                    let id = i
                        .get("id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("refs.id missing/not a string".into()))?;
                    let media_type = i
                        .get("media_type")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("refs.media_type missing/not a string".into()))?;
                    let size = match i.get("size").and_then(Json::as_int) {
                        Some(n) if n >= 0 => n as u64,
                        _ => return Err(bad("refs.size missing/not an int >= 0".into())),
                    };
                    let parsed = hh_identity::idp::parse_id(id)
                        .map_err(|_| bad(format!("refs.id {id} is not an idp/1 id")))?;
                    Ok::<ContentAddress, LedgerError>(ContentAddress {
                        idp: "idp/1",
                        algorithm: "sha256",
                        digest: parsed.digest_hex,
                        media_type: media_type.to_string(),
                        size,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(bad("refs missing/not an array".into())),
        };
        let ir_refs = match j.get("ir_refs") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| {
                    Ok(IrRef {
                        semantic_id: i
                            .get("semantic_id")
                            .and_then(Json::as_str)
                            .map(str::to_string),
                        version_id: i
                            .get("version_id")
                            .and_then(Json::as_str)
                            .map(str::to_string)
                            .ok_or_else(|| bad("ir_refs.version_id missing".into()))?,
                    })
                })
                .collect::<Result<Vec<_>, LedgerError>>()?,
            _ => return Err(bad("ir_refs missing/not an array".into())),
        };
        let surface_ids = match j.get("surface_ids") {
            Some(Json::Obj(m)) => m
                .iter()
                .map(|(k, v)| {
                    v.as_str()
                        .map(|s| (k.clone(), s.to_string()))
                        .ok_or_else(|| bad("surface_ids member not a string".into()))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            Some(_) => return Err(bad("surface_ids not an object".into())),
            None => BTreeMap::new(),
        };
        let provenance = match j.get("provenance") {
            Some(p) => Some(ProvenanceRecord::from_json(p)?),
            None => None,
        };
        Ok(EventEnvelope {
            event_id: req_str("event_id")?,
            run_id: req_str("run_id")?,
            seq: req_int("seq")?,
            ts: req_str("ts")?,
            hlc: j.get("hlc").and_then(Json::as_str).map(str::to_string),
            plane,
            class: req_str("class")?,
            schema_version,
            producer,
            participant_class,
            observability_level,
            durability,
            scope,
            lease_generation: req_int("lease_generation")?,
            parent_event_id: req_str("parent_event_id")?,
            causes,
            refs,
            ir_refs,
            surface_ids,
            provenance,
            prev_hash: req_str("prev_hash")?,
            payload: j
                .get("payload")
                .cloned()
                .ok_or_else(|| bad("payload missing".into()))?,
            hash: req_str("hash")?,
        })
    }
}

/// The closed envelope member set at schema v1 — anything else is a typed refusal.
const ENVELOPE_FIELDS: &[&str] = &[
    "causes",
    "class",
    "durability",
    "event_id",
    "hash",
    "hlc",
    "ir_refs",
    "lease_generation",
    "observability_level",
    "parent_event_id",
    "participant_class",
    "payload",
    "plane",
    "prev_hash",
    "producer",
    "provenance",
    "refs",
    "run_id",
    "schema_version",
    "scope",
    "seq",
    "surface_ids",
    "ts",
];

fn scope_json(s: &Scope) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("turn_id", &s.turn_id),
        ("model_call_id", &s.model_call_id),
        ("tool_call_id", &s.tool_call_id),
        ("effect_id", &s.effect_id),
        ("child_run_id", &s.child_run_id),
        ("branch_id", &s.branch_id),
    ] {
        if let Some(v) = v {
            m.insert(k.to_string(), Json::str(v));
        }
    }
    Json::Obj(m)
}

fn scope_from_json(j: &Json) -> Result<Scope, LedgerError> {
    let bad = |d: String| LedgerError::SchemaViolation { detail: d };
    let get = |k: &str| -> Result<Option<String>, LedgerError> {
        match j.get(k) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| bad(format!("scope.{k} not a string"))),
        }
    };
    if let Json::Obj(m) = j {
        for k in m.keys() {
            if ![
                "turn_id",
                "model_call_id",
                "tool_call_id",
                "effect_id",
                "child_run_id",
                "branch_id",
            ]
            .contains(&k.as_str())
            {
                return Err(bad(format!("unknown scope member {k}")));
            }
        }
    } else {
        return Err(bad("scope must be an object".into()));
    }
    Ok(Scope {
        turn_id: get("turn_id")?,
        model_call_id: get("model_call_id")?,
        tool_call_id: get("tool_call_id")?,
        effect_id: get("effect_id")?,
        child_run_id: get("child_run_id")?,
        branch_id: get("branch_id")?,
    })
}

fn event_ref_json(e: &EventRef) -> Json {
    Json::Obj(BTreeMap::from([
        ("run_id".to_string(), Json::str(&e.run_id)),
        ("event_id".to_string(), Json::str(&e.event_id)),
    ]))
}

fn event_ref_from_json(j: &Json) -> Result<EventRef, LedgerError> {
    let bad = |d: &str| LedgerError::SchemaViolation {
        detail: d.to_string(),
    };
    Ok(EventRef {
        run_id: j
            .get("run_id")
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| bad("causes.run_id missing"))?,
        event_id: j
            .get("event_id")
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| bad("causes.event_id missing"))?,
    })
}

impl EphemeralRecord {
    /// The canonical encoding of an ephemeral frame record (subscribe-side; never
    /// persisted).
    pub fn to_json(&self) -> Json {
        Json::Obj(BTreeMap::from([
            ("event_id".to_string(), Json::str(&self.event_id)),
            ("run_id".to_string(), Json::str(&self.run_id)),
            ("ts".to_string(), Json::str(&self.ts)),
            ("plane".to_string(), Json::str(self.plane.as_str())),
            ("class".to_string(), Json::str(&self.class)),
            (
                "schema_version".to_string(),
                Json::Int(self.schema_version as i64),
            ),
            ("payload".to_string(), self.payload.clone()),
            (
                "durability".to_string(),
                Json::str(Durability::Ephemeral.as_str()),
            ),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::GENESIS_HASH;

    fn sample() -> EventEnvelope {
        EventEnvelope {
            event_id: "evt-0000000001000-000000-st".into(),
            run_id: "run-0000000001000-000000-st".into(),
            seq: 0,
            ts: "2026-09-15T00:00:00.000Z".into(),
            hlc: None,
            plane: EventPlane::Lifecycle,
            class: "lifecycle.run.created".into(),
            schema_version: SCHEMA_VERSION,
            producer: Producer::kernel("kernel:ledger"),
            participant_class: ParticipantClass::Native,
            observability_level: BTreeSet::from([ObservabilityLevel::Ledger]),
            durability: Durability::Ledger,
            scope: Scope::default(),
            lease_generation: 1,
            parent_event_id: crate::ids::ROOT_EVENT.into(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: BTreeMap::new(),
            provenance: None,
            prev_hash: GENESIS_HASH.into(),
            payload: Json::obj([("run_kind", Json::str("agent"))]),
            hash: String::new(),
        }
    }

    #[test]
    fn hash_is_over_canonical_minus_hash_plus_prev() {
        let mut e = sample();
        e.hash = e.recompute_hash();
        let again = e.recompute_hash();
        assert_eq!(e.hash, again);
        assert!(e.hash.starts_with("sha256:"));
        // A byte change anywhere flips the hash.
        e.ts = "2026-09-15T00:00:00.124Z".into();
        assert_ne!(e.recompute_hash(), again);
    }

    #[test]
    fn decode_rejects_newer_schema_version() {
        let mut e = sample();
        e.hash = e.recompute_hash();
        let mut j = e.to_json();
        if let Json::Obj(ref mut m) = j {
            m.insert("schema_version".into(), Json::Int(2));
        }
        assert!(matches!(
            EventEnvelope::from_json(&j),
            Err(LedgerError::SchemaMismatch {
                found: 2,
                known_max: 1
            })
        ));
    }

    #[test]
    fn decode_rejects_unknown_member() {
        let mut e = sample();
        e.hash = e.recompute_hash();
        let mut j = e.to_json();
        if let Json::Obj(ref mut m) = j {
            m.insert("mystery".into(), Json::Int(1));
        }
        assert!(matches!(
            EventEnvelope::from_json(&j),
            Err(LedgerError::SchemaViolation { .. })
        ));
    }

    #[test]
    fn decode_rejects_missing_required_member() {
        let mut e = sample();
        e.hash = e.recompute_hash();
        let mut j = e.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("seq");
        }
        assert!(matches!(
            EventEnvelope::from_json(&j),
            Err(LedgerError::SchemaViolation { .. })
        ));
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let mut e = sample();
        e.hash = e.recompute_hash();
        let bytes = e.canonical_bytes();
        let parsed = hh_wire::json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let back = EventEnvelope::from_json(&parsed).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.canonical_bytes(), bytes);
    }

    #[test]
    fn deprecations_registry_is_checked_in() {
        // v1 has no deprecations — the registry exists so a future removal is a
        // recorded row first (AC-8).
        assert!(DEPRECATIONS.is_empty());
    }
}
