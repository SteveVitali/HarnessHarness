//! `R-2.3.4` — the K5 exact-match response cache (ADR-0129 d.2/d.4;
//! ticket S3.7): key shape, admissibility (`PreRegistrationInvalid`), served
//! terminal stamps (`served_from_cache`, `timing = n/a{not_run}`), and the
//! IR-2/IR-4 withholding rules (`stale_withheld` under `execute`, annotated
//! under `audit`, explicit request under `reproduce`).

use hh_context::{
    admit_k5, definition_dep_ref, k5_dependencies, profile_dep_ref, snapshot_dep_ref, K5Cache,
    K5Context, K5Key, K5Resolution, PreRegistrationInvalid,
};
use hh_identity::names::ResolveMode;
use hh_wire::json::Json;

fn key(replicate: u64) -> K5Key {
    K5Key {
        plan_hash: "sha256:plan".to_string(),
        provider_model_id: "model-a".to_string(),
        served_model: Some("model-a-2025-01".to_string()),
        snapshot_id: Some("snap-1".to_string()),
        replicate,
        configuration_version_id: "cfg-1".to_string(),
    }
}

fn deps() -> Vec<hh_context::DependencyStamp> {
    k5_dependencies("snap-1", "snaprec-1", "prof@1", "pv-1", "def-v1")
}

fn write(cache: &mut K5Cache, replicate: u64) -> String {
    cache.write(
        key(replicate),
        Json::obj([("role", Json::str("assistant"))]),
        Json::obj([("input_total", Json::Int(10))]),
        Json::obj([("ms", Json::Int(42))]),
        deps(),
        7,
    )
}

// AC-R-2.3.4-11: admissibility is a closed check — reproduce ≥ R1, the
// recording gateway, declared replay, or a declared `cache` factor.
#[test]
fn k5_admissibility() {
    assert!(admit_k5(&K5Context::Reproduce { level: 1 }).is_ok());
    assert!(admit_k5(&K5Context::RecordingGateway).is_ok());
    assert!(admit_k5(&K5Context::DeclaredDeterministicReplay).is_ok());
    assert!(admit_k5(&K5Context::ExperimentArm {
        declared_factor: true
    })
    .is_ok());

    let err = admit_k5(&K5Context::ExperimentArm {
        declared_factor: false,
    })
    .unwrap_err();
    assert!(err.reason.contains("factor"));
    let err = admit_k5(&K5Context::Reproduce { level: 0 }).unwrap_err();
    assert!(err.reason.contains("R1"));
    // The refusal is a typed value, never a warning string.
    let _: PreRegistrationInvalid = err;
}

// AC-R-2.3.4-11: a hit under reproduce stamps `served_from_cache` and
// `timing = n/a{not_run}`; the stored timing is never re-read.
#[test]
fn k5_hit_serves_terminal_stamp() {
    let mut cache = K5Cache::new();
    let entry_ref = write(&mut cache, 0);
    match cache.resolve(&key(0), ResolveMode::Reproduce, false) {
        K5Resolution::Hit { entry } => {
            assert_eq!(entry.entry_ref, entry_ref);
            let stamp = K5Cache::served_terminal_stamp(&entry_ref);
            let Json::Obj(m) = &stamp else { panic!("obj") };
            assert_eq!(
                m.get("timing"),
                Some(&Json::Str("n/a{not_run}".to_string()))
            );
            assert_eq!(
                m.get("served_from_cache"),
                Some(&Json::Str(format!("cache({entry_ref})")))
            );
        }
        other => panic!("expected hit, got {other:?}"),
    }
    // A miss under a different replicate is an honest `miss` — cached
    // responses never fake replicate variance.
    assert_eq!(
        cache.resolve(&key(1), ResolveMode::Reproduce, false),
        K5Resolution::Miss
    );
}

// AC-R-2.3.4-7: revoking the profile version withholds entries —
// `stale_withheld` under `execute`, annotated under `audit`.
#[test]
fn k5_revocation_withholds() {
    let mut cache = K5Cache::new();
    let entry_ref = write(&mut cache, 0);
    cache.record_revocation(&profile_dep_ref("prof@1"), "pv-1");

    match cache.resolve(&key(0), ResolveMode::Execute, false) {
        K5Resolution::StaleWithheld {
            entry_ref: r,
            reason,
        } => {
            assert_eq!(r, entry_ref);
            assert!(reason.contains("stale_withheld"));
            assert!(reason.contains("profile_version:prof@1"));
        }
        other => panic!("expected stale_withheld, got {other:?}"),
    }
    match cache.resolve(&key(0), ResolveMode::Audit, false) {
        K5Resolution::Annotated { entry, reason } => {
            assert_eq!(entry.entry_ref, entry_ref);
            assert!(reason.contains("stale_withheld"));
        }
        other => panic!("expected annotated, got {other:?}"),
    }
    // A non-matching revocation does not withhold.
    let mut cache2 = K5Cache::new();
    let r2 = write(&mut cache2, 0);
    cache2.record_revocation(&profile_dep_ref("prof@1"), "pv-OTHER");
    assert!(matches!(
        cache2.resolve(&key(0), ResolveMode::Execute, false),
        K5Resolution::Hit { entry } if entry.entry_ref == r2
    ));
}

// IR-4: served-model/fingerprint drift marks the snapshot's entries
// `stale_by_dependency`; reproduce may request them explicitly (annotated).
#[test]
fn k5_drift_marks_stale_by_dependency() {
    let mut cache = K5Cache::new();
    let entry_ref = write(&mut cache, 0);
    let marked = cache.note_drift(&snapshot_dep_ref("snap-1"), "fingerprint_drift");
    assert_eq!(marked, vec![entry_ref.clone()]);

    match cache.resolve(&key(0), ResolveMode::Execute, false) {
        K5Resolution::StaleWithheld { reason, .. } => {
            assert!(reason.contains("stale_by_dependency"));
            assert!(reason.contains("fingerprint_drift"));
        }
        other => panic!("expected stale_withheld, got {other:?}"),
    }
    // reproduce without an explicit request still withholds…
    assert!(matches!(
        cache.resolve(&key(0), ResolveMode::Reproduce, false),
        K5Resolution::StaleWithheld { .. }
    ));
    // …but an explicit reproduce request returns the entry annotated.
    match cache.resolve(&key(0), ResolveMode::Reproduce, true) {
        K5Resolution::Annotated { entry, reason } => {
            assert_eq!(entry.entry_ref, entry_ref);
            assert!(reason.contains("fingerprint_drift"));
        }
        other => panic!("expected annotated, got {other:?}"),
    }
}

// AC-R-2.3.4-6: every lookup emits exactly one `model.cache.resolved` row —
// hit, miss, and withheld alike.
#[test]
fn k5_resolved_payload_one_row_per_lookup() {
    let mut cache = K5Cache::new();
    let entry_ref = write(&mut cache, 0);
    let hit = cache.resolve(&key(0), ResolveMode::Execute, false);
    let miss = cache.resolve(&key(9), ResolveMode::Execute, false);
    let dep = definition_dep_ref("def-v1");
    cache.record_revocation(&dep, "def-v1");
    let withheld = cache.resolve(&key(0), ResolveMode::Execute, false);

    for (res, outcome) in [
        (&hit, "hit"),
        (&miss, "miss"),
        (&withheld, "stale_withheld"),
    ] {
        let row = cache.resolved_payload(&key(0), res, ResolveMode::Execute, 11);
        let Json::Obj(m) = &row else { panic!("obj") };
        assert_eq!(
            m.get("kind"),
            Some(&Json::Str("model.cache.resolved".into()))
        );
        assert_eq!(m.get("cache_kind"), Some(&Json::Str("response".into())));
        assert_eq!(m.get("outcome"), Some(&Json::Str(outcome.into())));
    }
    let Json::Obj(m) = cache.resolved_payload(&key(0), &hit, ResolveMode::Execute, 11) else {
        panic!("obj")
    };
    assert_eq!(m.get("entry_ref"), Some(&Json::Str(entry_ref)));
}

// The key preimage includes every spec member — `served_model`/`snapshot_id`
// absence spells `none`, never collides with a present value, and the
// replicate is in the key.
#[test]
fn k5_key_shape() {
    let a = key(0);
    let mut no_served = a.clone();
    no_served.served_model = None;
    assert!(no_served.preimage().contains("∥none∥"));
    assert_ne!(a.entry_key(), no_served.entry_key());
    assert_ne!(a.entry_key(), key(1).entry_key());
}
