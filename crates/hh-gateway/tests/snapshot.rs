//! Executable coverage for the S2.12 model-snapshot slice:
//! `ModelSnapshotRecord` (WS-L4) identity, the fingerprint probe, and the
//! `SnapshotClaim` drift signal (AC-R-2.3.1 ADR-0120 d.4/d.7; ADR-0203 D5).
//! Every test fails if the behaviour it names is removed.

use hh_gateway::snapshot::*;
use hh_provenance::origin::Origin;
use hh_provenance::{PersistenceScope, ProvenanceRecord};

fn prov() -> ProvenanceRecord {
    // `provenance = reported` — the provider's claim lands as an imported
    // origin (CC2: asserted-by-third-party, never self-certifying).
    ProvenanceRecord::minted(
        Origin::Import {
            source_system: "provider:anthropic".into(),
            mapping_version: "served-fields/v1".into(),
        },
        PersistenceScope::Run,
        1_700_000_000,
    )
}

fn pinned_snapshot() -> ModelSnapshotRecord {
    ModelSnapshotRecord {
        provider: "anthropic".into(),
        model_id: "claude-opus-4".into(),
        snapshot_id: Some("20250514".into()),
        pinned: true,
        observed_fingerprint: Some(fingerprint_response(b"2,3,5,7,11,13,17,19;4")),
        provenance: prov(),
    }
}

// AC-R-2.3.1 ADR-0120 d.4 — the record carries provider/model_id/
// snapshot_id/pinned/reported-provenance/observed_fingerprint and
// round-trips through the strict codec.
#[test]
fn record_shape_round_trip() {
    let r = pinned_snapshot();
    let j = r.to_json();
    let back = ModelSnapshotRecord::from_json(&j).expect("round-trip");
    assert_eq!(back, r);
    // Strict codec: an unknown member refuses, never coerces.
    let mut m = match j {
        hh_wire::json::Json::Obj(m) => m,
        _ => unreachable!(),
    };
    m.insert("surprise".into(), hh_wire::json::Json::Bool(true));
    let err = ModelSnapshotRecord::from_json(&hh_wire::json::Json::Obj(m)).unwrap_err();
    assert!(matches!(err, GatewaySnapshotError::BadShape { .. }));
}

// Identity (CC1): version_id covers the full record; semantic_id is the
// {provider, model_id} projection — a snapshot roll or a new fingerprint
// observation is a new *version* of the same semantic line.
#[test]
fn semantic_id_is_model_coordinate() {
    let a = pinned_snapshot();
    let mut rolled = a.clone();
    rolled.snapshot_id = Some("20250601".into());
    rolled.observed_fingerprint = Some(fingerprint_response(b"other"));
    assert_eq!(a.semantic_id(), rolled.semantic_id());
    assert_ne!(a.version_id(), rolled.version_id());
    // The dependency stamp is the version id — any observed change is a new
    // stamp, which is exactly what `stale_by_dependency` must see.
    assert_eq!(a.dependency_stamp(), a.version_id());
    assert_ne!(a.dependency_stamp(), rolled.dependency_stamp());
}

// ADR-0120 (e) — the probe is a declared canonical request; the fingerprint
// is the idp/1 digest of the canonical response under its own domain.
#[test]
fn fingerprint_probe_is_canonical() {
    let req = probe_request();
    let canonical = req.to_canonical_string();
    assert!(canonical.contains(FINGERPRINT_PROBE_ID));
    assert_eq!(fingerprint_response(b"resp"), fingerprint_response(b"resp"));
    assert_ne!(
        fingerprint_response(b"resp"),
        fingerprint_response(b"resp2")
    );
}

// The closed verdict: match iff byte-equal; drift on any mismatch; unknown
// on absent evidence — absence is never drift (ADR-0118 d.2).
#[test]
fn fingerprint_verdict_is_closed() {
    let f = fingerprint_response(b"x");
    assert_eq!(
        fingerprint_verdict(Some(&f), Some(&f)),
        FingerprintVerdict::Match
    );
    let g = fingerprint_response(b"y");
    assert_eq!(
        fingerprint_verdict(Some(&f), Some(&g)),
        FingerprintVerdict::Drift
    );
    assert_eq!(
        fingerprint_verdict(Some(&f), None),
        FingerprintVerdict::Unknown
    );
    assert_eq!(
        fingerprint_verdict(None, Some(&g)),
        FingerprintVerdict::Unknown
    );
    assert_eq!(fingerprint_verdict(None, None), FingerprintVerdict::Unknown);
}

// ADR-0203 D5 / ADR-0126 `model_version_change` (i) — a served-model
// substitution raises the synthetic SnapshotClaim with `snapshot_id =
// observed`; the observed record honestly records what the wire said.
#[test]
fn observe_raises_claim_on_served_model_substitution() {
    let pinned = pinned_snapshot();
    let (obs, claim) = observe(
        &pinned,
        Some("claude-haiku-3"),
        Some("20250601"),
        None,
        prov(),
    );
    let claim = claim.expect("substitution raises a claim");
    assert_eq!(claim.claim_kind, SnapshotClaimKind::ServedModelSubstitution);
    assert_eq!(claim.snapshot_id, "20250601");
    assert_eq!(claim.pinned_model_id, "claude-opus-4");
    assert_eq!(claim.observed, "claude-haiku-3");
    assert_eq!(obs.snapshot_id.as_deref(), Some("20250601"));
}

// A fingerprint `DRIFT` raises the claim even when the served model id is
// unchanged — the snapshot rolled under a stable name.
#[test]
fn observe_raises_claim_on_fingerprint_drift() {
    let pinned = pinned_snapshot();
    let drifted = fingerprint_response(b"different-response");
    let (_obs, claim) = observe(
        &pinned,
        Some("claude-opus-4"),
        Some("20250601"),
        Some(&drifted),
        prov(),
    );
    let claim = claim.expect("drift raises a claim");
    assert_eq!(claim.claim_kind, SnapshotClaimKind::FingerprintDrift);
    assert_eq!(claim.observed, drifted);
}

// No contradiction → no claim; `unknown` coverage never fabricates one.
#[test]
fn observe_silent_when_consistent_or_unobserved() {
    let pinned = pinned_snapshot();
    let f = pinned.observed_fingerprint.clone().unwrap();
    let (_obs, claim) = observe(
        &pinned,
        Some("claude-opus-4"),
        pinned.snapshot_id.as_deref(),
        Some(&f),
        prov(),
    );
    assert!(claim.is_none());
    // Provider reports nothing: no claim, and the observation inherits the
    // pin's coverage rather than fabricating.
    let (obs, claim) = observe(&pinned, None, None, None, prov());
    assert!(claim.is_none());
    assert_eq!(obs.observed_fingerprint, pinned.observed_fingerprint);
}
