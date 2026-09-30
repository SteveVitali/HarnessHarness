//! `R-2.3.4²` — the K6 similarity-keyed semantic cache (ADR-0129 d.3;
//! ticket S5.1): the exact/marginal/miss lookup, the **mandatory**
//! verifier gate with `verifier_verdict{validator_ref, verdict}` on
//! `model.cache.resolved`, promotion after a pass, `readers`/principal/
//! narrow-or-preserve scoping refusals, the poisoning fixture
//! (AC-R-2.3.4-12), `cache.false_hit_rate[semantic]` per profile, and
//! IR-2 withholding.

use std::collections::BTreeSet;

use hh_context::{
    cosine_similarity_ppm, k6_dependencies, K6Cache, K6Entry, K6Refusal, K6Request, K6Resolution,
    K6Scope, K6Verifier, K6Write, PPM,
};
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, ReaderSet};
use hh_wire::json::Json;

fn deps() -> Vec<hh_context::DependencyStamp> {
    k6_dependencies("snap-1", "snaprec-1", "prof@1", "pv-1", "def-v1")
}

fn readers(names: &[&str]) -> ReaderSet {
    ReaderSet::Restricted(names.iter().map(|s| s.to_string()).collect())
}

/// An entry written by `principal:a` under `readers = {principal:a}`,
/// embedding `[1000, 0]` — near-key lookups along `[900, 100]` land at
/// ≈0.994 similarity.
fn write(
    cache: &mut K6Cache,
    request_key: &str,
    embedding: &[i64],
    writer_authority: AuthorityClass,
    readers: ReaderSet,
) -> String {
    cache.write(
        K6Write {
            request_key: request_key.to_string(),
            embedding: embedding.to_vec(),
            response: Json::obj([("role", Json::str("assistant"))]),
            profile_ref: "prof@1".to_string(),
            principal: "principal:a".to_string(),
            readers,
            writer_authority,
            writer_ref: "writer:proc-1".to_string(),
            verifier_ref: "validator:k6.verifier.1".to_string(),
            threshold_ppm: 900_000,
            dependencies: deps(),
        },
        7,
    )
}

fn scope(readers: ReaderSet) -> K6Scope {
    K6Scope {
        principal: "principal:a".to_string(),
        readers,
        authority: AuthorityClass::Principal,
        validators: BTreeSet::from(["validator:k6.verifier.1".to_string()]),
    }
}

fn request(request_key: &str, embedding: &[i64]) -> K6Request {
    K6Request {
        request_key: request_key.to_string(),
        embedding: embedding.to_vec(),
        threshold_ppm: 900_000,
        request: Json::obj([("prompt", Json::str("the request"))]),
    }
}

struct TestVerifier {
    validator_ref: &'static str,
    verdict: bool,
    calls: std::cell::Cell<u32>,
}

impl K6Verifier for TestVerifier {
    fn validator_ref(&self) -> &str {
        self.validator_ref
    }
    fn verify(&self, _entry: &K6Entry, _request: &Json) -> bool {
        self.calls.set(self.calls.get() + 1);
        self.verdict
    }
}

fn passing() -> TestVerifier {
    TestVerifier {
        validator_ref: "validator:k6.verifier.1",
        verdict: true,
        calls: std::cell::Cell::new(0),
    }
}

#[test]
fn cosine_similarity_ppm_math() {
    assert_eq!(cosine_similarity_ppm(&[1000, 0], &[1000, 0]), Some(PPM));
    assert_eq!(cosine_similarity_ppm(&[1000, 0], &[0, 1000]), Some(0));
    assert_eq!(cosine_similarity_ppm(&[1000, 0], &[-1000, 0]), Some(-PPM));
    // dot = 900_000, √(da·db) = √(10⁶·8.2·10⁵) ⇒ ≈ 0.99388
    let sim = cosine_similarity_ppm(&[900, 100], &[1000, 0]).unwrap();
    assert!((993_000..=994_000).contains(&sim));
    // Incomparable ranks and the zero vector are not candidates.
    assert_eq!(cosine_similarity_ppm(&[1], &[1, 2]), None);
    assert_eq!(cosine_similarity_ppm(&[0, 0], &[1, 0]), None);
}

// §5b.4: exact_hit leg — a matching request key serves without a verifier
// call.
#[test]
fn exact_hit_serves_without_verifier() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let verifier = passing();
    let res = cache.resolve(
        &request("req-1", &[1000, 0]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        Some(&verifier),
    );
    let K6Resolution::Hit {
        verdict,
        promoted,
        similarity_ppm,
        ..
    } = &res
    else {
        panic!("expected Hit, got {res:?}");
    };
    assert_eq!(verifier.calls.get(), 0, "exact leg never calls verify");
    assert!(verdict.is_none(), "no verifier_verdict on an exact hit");
    assert!(!promoted);
    assert_eq!(*similarity_ppm, None);
}

// §5b.4: a marginal is served only after a registered Validator passes,
// then promoted — the identical request is an exact hit thereafter.
#[test]
fn marginal_serves_only_after_verifier_pass_then_promotes() {
    let mut cache = K6Cache::new();
    let entry_ref = write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let sc = scope(readers(&["principal:a"]));
    let verifier = passing();

    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&verifier),
    );
    let K6Resolution::Hit {
        entry,
        verdict,
        promoted,
        similarity_ppm,
    } = &res
    else {
        panic!("expected served marginal, got {res:?}");
    };
    assert_eq!(verifier.calls.get(), 1);
    assert!(promoted);
    assert_eq!(entry.entry_ref, entry_ref);
    assert!(entry.promoted_keys.contains("req-2"));
    let v = verdict.as_ref().expect("verifier_verdict present");
    assert_eq!(v.validator_ref, "validator:k6.verifier.1");
    assert_eq!(v.verdict, "pass");
    assert!(similarity_ppm.unwrap() >= 900_000);

    // Promotion: the same request key now resolves on the exact leg.
    let verifier2 = passing();
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&verifier2),
    );
    let K6Resolution::Hit { verdict, .. } = &res else {
        panic!("expected promoted exact hit, got {res:?}");
    };
    assert_eq!(verifier2.calls.get(), 0);
    assert!(verdict.is_none());
}

// §5b.4 + AC-R-2.3.4-12: a marginal is never served without
// verifier_verdict = pass — a fail verdict refuses and lands in the
// false_hit_rate log.
#[test]
fn marginal_fail_refuses_verifier_failed() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let verifier = TestVerifier {
        validator_ref: "validator:k6.verifier.1",
        verdict: false,
        calls: std::cell::Cell::new(0),
    };
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        Some(&verifier),
    );
    let K6Resolution::Refused { refusal, .. } = &res else {
        panic!("expected refused, got {res:?}");
    };
    assert!(matches!(refusal, K6Refusal::VerifierFailed { .. }));
    assert_eq!(refusal.reason(), "verifier_failed{verdict:fail}");
    // The verdict is a fact in the false-hit-rate log.
    assert_eq!(cache.verdict_log(), &[("prof@1".to_string(), false)]);
    assert_eq!(cache.false_hit_rate_ppm("prof@1"), Some(PPM));
    // Nothing was promoted.
    assert!(cache.entries().all(|e| e.promoted_keys.is_empty()));
}

// A marginal with no verifier at all refuses — `verifier` is a required
// argument, not an optional nicety.
#[test]
fn marginal_without_verifier_refuses() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        None,
    );
    assert!(matches!(
        res,
        K6Resolution::Refused {
            refusal: K6Refusal::VerifierFailed { .. },
            ..
        }
    ));
}

// The entry's bound verifier must be a *registered* validator of the
// serving context — an unknown or mismatched validator refuses.
#[test]
fn unregistered_or_mismatched_verifier_refuses() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let sc = scope(readers(&["principal:a"]));

    // Right impl ref, but the context does not register it.
    let mut sc_unregistered = sc.clone();
    sc_unregistered.validators.clear();
    let verifier = passing();
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc_unregistered,
        ResolveMode::Execute,
        Some(&verifier),
    );
    assert!(matches!(
        res,
        K6Resolution::Refused {
            refusal: K6Refusal::VerifierFailed { .. },
            ..
        }
    ));

    // A different validator impl than the entry binds.
    let wrong = TestVerifier {
        validator_ref: "validator:other",
        verdict: true,
        calls: std::cell::Cell::new(0),
    };
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&wrong),
    );
    assert!(matches!(
        res,
        K6Resolution::Refused {
            refusal: K6Refusal::VerifierFailed { .. },
            ..
        }
    ));
    assert_eq!(wrong.calls.get(), 0, "verify() is never called");
}

// AC-R-2.3.4-9/-12: an entry written under readers = {A} never hits for a
// reader set it does not admit — the refusal is observable, not a silent
// miss.
#[test]
fn readers_violation_refuses() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &scope(readers(&["principal:b"])),
        ResolveMode::Execute,
        Some(&passing()),
    );
    let K6Resolution::Refused { refusal, .. } = &res else {
        panic!("expected refused, got {res:?}");
    };
    assert!(matches!(refusal, K6Refusal::ReadersViolation { .. }));
    assert_eq!(refusal.reason(), "readers_violation{readers}");
}

// Entries are scoped by principal — a cross-principal near-key refuses.
#[test]
fn cross_principal_near_key_refuses() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a", "principal:b"]),
    );
    let mut sc = scope(readers(&["principal:b"]));
    sc.principal = "principal:b".to_string();
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&passing()),
    );
    let K6Resolution::Refused { refusal, .. } = &res else {
        panic!("expected refused, got {res:?}");
    };
    assert_eq!(refusal.reason(), "readers_violation{principal}");
}

// AC-R-2.3.4-12: the poisoning fixture — an entry written by a `delegate`
// writer under a crafted near-key is refused for a `principal`-context
// read (ADR-0034 P1 narrow-or-preserve).
#[test]
fn delegate_written_near_key_never_serves_into_principal_context() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-poison",
        &[1000, 0],
        AuthorityClass::Delegate,
        readers(&["principal:a"]),
    );
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        Some(&passing()),
    );
    let K6Resolution::Refused { refusal, .. } = &res else {
        panic!("poisoning fixture must refuse, got {res:?}");
    };
    assert!(matches!(refusal, K6Refusal::AuthorityNarrowed { .. }));
    assert_eq!(
        refusal.reason(),
        "authority_narrowed{writer:delegate<context:principal}"
    );
}

// Narrow-or-preserve cuts both ways only upward — a delegate context may
// read a principal-authored entry (delivered at the entry's label).
#[test]
fn delegate_context_reads_principal_written_entry() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let mut sc = scope(readers(&["principal:a"]));
    sc.authority = AuthorityClass::Delegate;
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&passing()),
    );
    assert!(matches!(res, K6Resolution::Hit { .. }), "got {res:?}");
}

// Below-threshold similarity is an honest miss.
#[test]
fn below_threshold_misses() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let res = cache.resolve(
        &request("req-2", &[0, 1000]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        Some(&passing()),
    );
    assert_eq!(res, K6Resolution::Miss);
}

// IR-2: a revoked contract dep withholds the entry — `stale_withheld`
// under execute, `annotated` under audit; withholding precedes scoping.
#[test]
fn revoked_dependency_withholds() {
    let mut cache = K6Cache::new();
    let entry_ref = write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    cache.record_revocation("profile_version:prof@1", "pv-1");
    let sc = scope(readers(&["principal:a"]));
    let res = cache.resolve(
        &request("req-1", &[1000, 0]),
        &sc,
        ResolveMode::Execute,
        Some(&passing()),
    );
    let K6Resolution::StaleWithheld { reason, .. } = &res else {
        panic!("expected stale_withheld, got {res:?}");
    };
    assert!(reason.contains("revoked:profile_version:prof@1=pv-1"));
    let res = cache.resolve(
        &request("req-1", &[1000, 0]),
        &sc,
        ResolveMode::Audit,
        Some(&passing()),
    );
    let K6Resolution::Annotated { entry, reason } = &res else {
        panic!("expected annotated, got {res:?}");
    };
    assert_eq!(entry.entry_ref, entry_ref);
    assert!(reason.contains("revoked"));
}

// Drift marks withhold on the marginal leg too.
#[test]
fn drift_mark_withholds_marginal() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let marked = cache.note_drift("model_snapshot:snap-1", "fingerprint_drift");
    assert_eq!(marked.len(), 1);
    let res = cache.resolve(
        &request("req-2", &[900, 100]),
        &scope(readers(&["principal:a"])),
        ResolveMode::Execute,
        Some(&passing()),
    );
    assert!(matches!(res, K6Resolution::StaleWithheld { .. }));
}

// `cache.false_hit_rate[semantic]` is per-profile (AC-R-2.3.4-12):
// fail/(pass+fail) over the verdict log.
#[test]
fn false_hit_rate_per_profile() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let sc = scope(readers(&["principal:a"]));
    let fail = TestVerifier {
        validator_ref: "validator:k6.verifier.1",
        verdict: false,
        calls: std::cell::Cell::new(0),
    };
    cache.resolve(
        &request("r1", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&fail),
    );
    cache.resolve(
        &request("r2", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&fail),
    );
    cache.resolve(
        &request("r3", &[900, 100]),
        &sc,
        ResolveMode::Execute,
        Some(&passing()),
    );
    // 2 fails, 1 pass → 2/3 in ppm.
    assert_eq!(cache.false_hit_rate_ppm("prof@1"), Some(2 * PPM / 3));
    // No verdicts for another profile → undefined, not zero.
    assert_eq!(cache.false_hit_rate_ppm("prof@2"), None);
}

// The `model.cache.resolved` row: `cache_kind = semantic`, the
// `{principal, readers}` scope, and `verifier_verdict{validator_ref,
// verdict}` on a verified marginal (§5b.4 event row; ADR-0128 d.3).
#[test]
fn resolved_payload_carries_verifier_verdict() {
    let mut cache = K6Cache::new();
    write(
        &mut cache,
        "req-1",
        &[1000, 0],
        AuthorityClass::Principal,
        readers(&["principal:a"]),
    );
    let sc = scope(readers(&["principal:a"]));
    let req = request("req-2", &[900, 100]);
    let res = cache.resolve(&req, &sc, ResolveMode::Execute, Some(&passing()));
    let Json::Obj(m) = cache.resolved_payload(&req, &sc, &res, ResolveMode::Execute, 11) else {
        panic!("payload must be an object");
    };
    assert_eq!(m["cache_kind"], Json::str("semantic"));
    assert_eq!(m["outcome"], Json::str("hit"));
    let Json::Obj(v) = &m["verifier_verdict"] else {
        panic!("verifier_verdict must be an object");
    };
    assert_eq!(v["validator_ref"], Json::str("validator:k6.verifier.1"));
    assert_eq!(v["verdict"], Json::str("pass"));
    assert_eq!(m["promoted"], Json::Bool(true));
    let Json::Obj(s) = &m["scope"] else {
        panic!("scope must be an object");
    };
    assert_eq!(s["principal"], Json::str("principal:a"));

    // An exact hit carries no verifier_verdict.
    let req2 = request("req-2", &[900, 100]);
    let res2 = cache.resolve(&req2, &sc, ResolveMode::Execute, None);
    let Json::Obj(m2) = cache.resolved_payload(&req2, &sc, &res2, ResolveMode::Execute, 12) else {
        panic!("payload must be an object");
    };
    assert_eq!(m2["outcome"], Json::str("hit"));
    assert_eq!(m2["verifier_verdict"], Json::Null);
    assert_eq!(m2["promoted"], Json::Bool(false));
}
