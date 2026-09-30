//! `ModelSnapshotRecord` + the fingerprint probe (spec §5b.4 WS-L4 row;
//! R-2.3.1, ADR-0120 d.4/d.5/d.7; ADR-0203 D5; S2.12).
//!
//! The record — `{provider, model_id, snapshot_id?, pinned,
//! provenance = reported, observed_fingerprint?}` — is what the gateway
//! *fills* from a response's served-model/system-fingerprint fields
//! (`served_model`, `snapshot_id` on `message.completed`; the dialect's
//! `served_model_exposed`/`snapshot_id_exposed` members say where each wire
//! carries them). `provenance` is `reported` — the provider's claim, never
//! coerced to probed (CC2: `publisher_claim` semantics — a reported field is
//! evidence of what the provider said, not of what ran).
//!
//! The fingerprint probe (ADR-0120 (e)): a *declared, canonical* probe
//! request (`probe_request()`) whose canonical response body is digested
//! under the `model_snapshot.fingerprint` domain → `observed_fingerprint`.
//! Matching is closed: [`FingerprintVerdict::Match`] iff the pinned and
//! observed digests are byte-equal; anything else is `Drift` (absent
//! observed evidence is `Unknown` — never coerced, ADR-0118 d.2 "unknown
//! never coerced").
//!
//! A fingerprint `DRIFT`, a `served_model ≠ model_id` substitution or a
//! snapshot-id change raises the synthetic [`SnapshotClaim`] signal
//! (`{snapshot_id = observed}` — ADR-0203 D5): the caller surfaces it as a
//! discovery/expiry observable, the gateway never decides the verdict's
//! consequence (I-NOAUTH — the router/registry owns the gate).

use hh_identity::idp::{identify_bytes, idp_digest};
use hh_identity::kinds::RecordKind;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

/// The canonical probe request (ADR-0120 (e) — the fingerprint probe is a
/// declared probe, instrument-charged, never a hidden side-channel). A fixed
/// prompt set + pinned sampling so the response distribution is a function
/// of the served model alone.
pub const FINGERPRINT_PROBE_ID: &str = "hh.fingerprint/v1";

/// The canonical probe request body — deterministic sampling
/// (`temperature = 0`, bounded `max_tokens`) over the declared probe prompts.
pub fn probe_request() -> Json {
    Json::obj([
        ("kind", Json::str("fingerprint_probe")),
        ("probe_id", Json::str(FINGERPRINT_PROBE_ID)),
        (
            "prompts",
            Json::Arr(vec![
                Json::str("Respond with the first twenty prime numbers, comma-separated."),
                Json::str("State the JSON value of 2+2 as a bare number."),
            ]),
        ),
        ("temperature", Json::Int(0)),
        ("max_tokens", Json::Int(128)),
    ])
}

/// `observed_fingerprint` — the `idp/1` digest of the canonical response
/// body under the `model_snapshot.fingerprint` domain (distinct from every
/// record/blob domain, N3).
pub fn fingerprint_response(canonical_response: &[u8]) -> String {
    idp_digest("model_snapshot.fingerprint", canonical_response)
}

/// The closed match verdict — `unknown` is a member, never a coercion of
/// absent evidence (ADR-0118 d.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerprintVerdict {
    /// The observed fingerprint equals the pinned one.
    Match,
    /// The observed fingerprint differs — a `DRIFT` observable.
    Drift,
    /// No observed fingerprint was produced this observation.
    Unknown,
}

impl FingerprintVerdict {
    /// The canonical spelling (`model.surface.*`/discovery rows read it).
    pub fn as_str(self) -> &'static str {
        match self {
            FingerprintVerdict::Match => "match",
            FingerprintVerdict::Drift => "drift",
            FingerprintVerdict::Unknown => "unknown",
        }
    }
}

/// `match` iff pinned == observed byte-for-byte; `drift` on any mismatch;
/// `unknown` when either side is absent (absence is *not* drift — a
/// provider that never reports fingerprints yields `unknown`, and the
/// record's `pinned` stays the only claim).
pub fn fingerprint_verdict(pinned: Option<&str>, observed: Option<&str>) -> FingerprintVerdict {
    match (pinned, observed) {
        (Some(p), Some(o)) if p == o => FingerprintVerdict::Match,
        (Some(_), Some(_)) => FingerprintVerdict::Drift,
        _ => FingerprintVerdict::Unknown,
    }
}

/// `ModelSnapshotRecord` (§5b.4 WS-L4): the observed model coordinate —
/// `{provider, model_id, snapshot_id?, pinned, provenance = reported,
/// observed_fingerprint?}`.
///
/// Identities (`idp/1`, CC1): `version_id` is the full-record digest under
/// the `model_snapshot` domain; `semantic_id` is the semantic projection
/// `{provider, model_id}` — the coordinate `configuration_id.model_ref`-adjacent
/// comparisons use. A provider-side snapshot roll or a fingerprint
/// observation mints a new *version* of the same semantic line.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSnapshotRecord {
    /// The provider id (`provider:*` — N7: a claim, keyed by the registry's
    /// provider record, never self-certifying).
    pub provider: String,
    /// The declared model coordinate (`model_ref.provider_model_id`).
    pub model_id: String,
    /// The provider-reported snapshot id (`snapshot_id_exposed`), when the
    /// wire carries one.
    pub snapshot_id: Option<String>,
    /// `pinned` — the operator pinned this snapshot (R2 reachability;
    /// `repro::check_claim` refuses R2 without it).
    pub pinned: bool,
    /// The fingerprint probe's observation, when one ran.
    pub observed_fingerprint: Option<String>,
    /// `provenance = reported` — the record is the provider's claim +
    /// the probe's observation; its authority is read from the provenance
    /// origin, never widened by the content (CC2).
    pub provenance: ProvenanceRecord,
}

impl ModelSnapshotRecord {
    /// `version_id` — `idp/1` over the canonical record body.
    pub fn version_id(&self) -> String {
        identify_bytes(
            RecordKind::ModelSnapshot,
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// `semantic_id` — the `{provider, model_id}` projection (the coordinate;
    /// observed members excluded — N6).
    pub fn semantic_id(&self) -> String {
        let proj = Json::obj([
            ("model_id", Json::str(self.model_id.clone())),
            ("provider", Json::str(self.provider.clone())),
        ]);
        idp_digest(
            "model_snapshot.semantic",
            proj.to_canonical_string().as_bytes(),
        )
    }

    /// The model-snapshot dependency stamp (OQ-206 — the gateway publishes
    /// this; K5/K1 stamps and `stale_by_dependency` reads key off it).
    /// `version_id` *is* the stamp: any observed change (snapshot roll,
    /// fingerprint drift) yields a new stamp by construction.
    pub fn dependency_stamp(&self) -> String {
        self.version_id()
    }

    /// The canonical record body (sorted-key `idp/1` JSON).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("model_id", Json::str(self.model_id.clone())),
            (
                "observed_fingerprint",
                self.observed_fingerprint
                    .clone()
                    .map_or(Json::Null, Json::Str),
            ),
            ("pinned", Json::Bool(self.pinned)),
            ("provider", Json::str(self.provider.clone())),
            ("provenance", self.provenance.to_json()),
            (
                "snapshot_id",
                self.snapshot_id.clone().map_or(Json::Null, Json::Str),
            ),
        ])
    }

    /// The strict codec — unknown members and wrong types refuse
    /// (`{path, code}`), never coerced.
    pub fn from_json(j: &Json) -> Result<ModelSnapshotRecord, GatewaySnapshotError> {
        let m = match j {
            Json::Obj(m) => m,
            _ => {
                return Err(GatewaySnapshotError::BadShape {
                    path: "".to_string(),
                    code: "expected_object".to_string(),
                })
            }
        };
        for k in m.keys() {
            if !matches!(
                k.as_str(),
                "model_id"
                    | "observed_fingerprint"
                    | "pinned"
                    | "provider"
                    | "provenance"
                    | "snapshot_id"
            ) {
                return Err(GatewaySnapshotError::BadShape {
                    path: format!("/{k}"),
                    code: "unknown_member".to_string(),
                });
            }
        }
        let req_str = |k: &str| -> Result<String, GatewaySnapshotError> {
            m.get(k)
                .and_then(Json::as_str)
                .map(String::from)
                .ok_or_else(|| GatewaySnapshotError::BadShape {
                    path: format!("/{k}"),
                    code: "missing_or_not_string".to_string(),
                })
        };
        let opt_str = |k: &str| -> Result<Option<String>, GatewaySnapshotError> {
            match m.get(k) {
                None | Some(Json::Null) => Ok(None),
                Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                    GatewaySnapshotError::BadShape {
                        path: format!("/{k}"),
                        code: "type_mismatch".to_string(),
                    }
                }),
            }
        };
        let pinned = match m.get("pinned") {
            Some(Json::Bool(b)) => *b,
            _ => {
                return Err(GatewaySnapshotError::BadShape {
                    path: "/pinned".to_string(),
                    code: "missing_or_not_bool".to_string(),
                })
            }
        };
        let provenance = m
            .get("provenance")
            .ok_or_else(|| GatewaySnapshotError::BadShape {
                path: "/provenance".to_string(),
                code: "missing".to_string(),
            })
            .and_then(|p| {
                ProvenanceRecord::from_json(p).map_err(|e| GatewaySnapshotError::BadShape {
                    path: "/provenance".to_string(),
                    code: format!("{e:?}"),
                })
            })?;
        Ok(ModelSnapshotRecord {
            provider: req_str("provider")?,
            model_id: req_str("model_id")?,
            snapshot_id: opt_str("snapshot_id")?,
            pinned,
            observed_fingerprint: opt_str("observed_fingerprint")?,
            provenance,
        })
    }
}

/// The synthetic `SnapshotClaim` signal (ADR-0203 D5) — raised when an
/// observation contradicts the pinned record: `snapshot_id = observed`
/// carries the *observed* coordinate, never the claim's.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotClaim {
    /// `served_model` — the served-model substitution (`model_version_change`
    /// observable (i), ADR-0126).
    pub claim_kind: SnapshotClaimKind,
    /// The observed snapshot id (or served model when no snapshot id rode
    /// the wire).
    pub snapshot_id: String,
    /// The pinned coordinate the observation contradicts.
    pub pinned_model_id: String,
    /// The observed coordinate (`served_model` or the drifted fingerprint).
    pub observed: String,
}

/// The closed claim-kind sum — the two DRIFT raisers ADR-0203 D5 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotClaimKind {
    /// `served_model ≠ model_ref.provider_model_id` on a call of the binding.
    ServedModelSubstitution,
    /// The fingerprint probe returned `drift` (or a compatibility-token
    /// change, which lands as the same claim shape).
    FingerprintDrift,
}

impl SnapshotClaimKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotClaimKind::ServedModelSubstitution => "served_model_substitution",
            SnapshotClaimKind::FingerprintDrift => "fingerprint_drift",
        }
    }
}

/// `observe(snapshot, served_model, snapshot_id, fingerprint)` — fold one
/// response's served fields against the pinned record. Returns the
/// observation's `ModelSnapshotRecord` (always — the honest record of what
/// the wire said) plus a `SnapshotClaim` when the observation contradicts
/// the pin (served-model substitution, snapshot-id change, fingerprint
/// drift — the ADR-0126 `model_version_change` observables (i)/(iii)).
///
/// `snapshot_id`/`fingerprint` are `Option`s — a provider that reports
/// neither yields an observation with `unknown` coverage, never a
/// fabricated claim.
pub fn observe(
    pinned: &ModelSnapshotRecord,
    served_model: Option<&str>,
    snapshot_id: Option<&str>,
    observed_fingerprint: Option<&str>,
    provenance: ProvenanceRecord,
) -> (ModelSnapshotRecord, Option<SnapshotClaim>) {
    let observed = ModelSnapshotRecord {
        provider: pinned.provider.clone(),
        model_id: pinned.model_id.clone(),
        snapshot_id: snapshot_id.map(String::from).or(pinned.snapshot_id.clone()),
        pinned: pinned.pinned,
        observed_fingerprint: observed_fingerprint
            .map(String::from)
            .or(pinned.observed_fingerprint.clone()),
        provenance,
    };
    let served_substituted = matches!(served_model, Some(s) if s != pinned.model_id);
    let claim = if served_substituted {
        let served = served_model.unwrap_or_default();
        Some(SnapshotClaim {
            claim_kind: SnapshotClaimKind::ServedModelSubstitution,
            snapshot_id: snapshot_id.unwrap_or(served).to_string(),
            pinned_model_id: pinned.model_id.clone(),
            observed: served.to_string(),
        })
    } else if fingerprint_verdict(pinned.observed_fingerprint.as_deref(), observed_fingerprint)
        == FingerprintVerdict::Drift
    {
        Some(SnapshotClaim {
            claim_kind: SnapshotClaimKind::FingerprintDrift,
            snapshot_id: snapshot_id.unwrap_or("observed").to_string(),
            pinned_model_id: pinned.model_id.clone(),
            observed: observed_fingerprint.unwrap_or("").to_string(),
        })
    } else {
        None
    };
    (observed, claim)
}

/// The strict-codec refusal (typed, never a warning — the crate's own
/// `GatewayError` stays the transport taxonomy).
#[derive(Debug, Clone, PartialEq)]
pub enum GatewaySnapshotError {
    /// A member was missing, mistyped or unknown.
    BadShape {
        /// The JSON pointer the failure names.
        path: String,
        /// The machine-readable code.
        code: String,
    },
}

impl std::fmt::Display for GatewaySnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewaySnapshotError::BadShape { path, code } => {
                write!(f, "model_snapshot{path}: {code}")
            }
        }
    }
}

impl std::error::Error for GatewaySnapshotError {}
