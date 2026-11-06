//! R5.57 — the verifier-bound admission regression battery
//! (ADR-0422). Deterministic legs only (offline, USD $0):
//!
//! - AC-1/AC-2: `admit_verifier_bound` relaxes `max_test_timeout_sec`
//!   only for the named task under the declared admission —
//!   `ext["max_test_timeout_sec"]` carries the admitted decl and
//!   `verifier_deadline_ms` derives `min(admitted, 900 s)` verbatim;
//!   the `verifier_bound_admission` record carries before → after,
//!   basis, scope, `prior_disposition`, `supplement_epoch`, envelope
//!   check and authorization verbatim.
//! - AC-2 (fails-without): a spec lacking the admission keeps the
//!   prior enforced decl — the same derivation yields the unrelaxed
//!   deadline; no code path relaxes a bound silently.
//! - Refusals stay: a different task's spec, an unverifiable scope, a
//!   mislabeled before (the recorded run ledger's stated `60` on a
//!   spec that enforced `360`), a bound beyond the 900 s §9.2
//!   envelope, a non-relaxing "admission", empty verbatim members, and
//!   a malformed decl member all refuse typed and change nothing.

use std::collections::{BTreeMap, BTreeSet};

use hharness_env::errors::EnvError;
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::verifier_bound::{
    admit_verifier_bound, enforced_decl_sec, slot_echo, verifier_deadline_ms,
    VerifierBoundAdmission, ADMISSION_FIELD, SPEC_FIELD, SPEC_GEN_DEFAULT_SEC,
    VERIFIER_BOUND_CAP_SEC,
};
use hharness_wire::json::Json;

fn cell_record(task_id: Option<&str>, decl: Option<Json>) -> EnvironmentRecord {
    let mut ext = BTreeMap::new();
    if let Some(t) = task_id {
        ext.insert("task_id".to_string(), Json::str(t.to_string()));
    }
    if let Some(d) = decl {
        ext.insert(SPEC_FIELD.to_string(), d);
    }
    EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("localhost/s4-tb__swe-bench-astropy-2:latest".to_string()),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext,
        canary_channels: vec![],
    }
}

fn admission() -> VerifierBoundAdmission {
    VerifierBoundAdmission {
        task_id: "swe-bench-astropy-2".to_string(),
        enforced_before_sec: 360,
        admitted_sec: 900,
        basis: "ten slots inconclusive{verifier_unavailable:no_parseable_verdict} — \
                slot:6cc2a5bb… slot:8147eefb… slot:ad2ed084… slot:64bb5eb0… \
                slot:4fb854c1… slot:837bbdf3… slot:298b0c7e… slot:b1c89915… \
                slot:34719ba8… slot:51947953 (hh-s4-egress-supplement/1)"
            .to_string(),
        prior_disposition: "inconclusive{verifier_unavailable:no_parseable_verdict}".to_string(),
        supplement_epoch: "verifier-bound-rerun".to_string(),
        authorization: "\"insert these into the build plan as new ticket(s), then continue \
                        driving orchestrate-build over them to the end\" + \"you can \
                        provision everything. just make sure we don't forget to \
                        deprovision at the end. proceed\" (operator chat, 2026-11-06)"
            .to_string(),
        scope: "this task's verifier leg only — no other task, no other bound".to_string(),
    }
}

fn expect_unsupported(err: EnvError) {
    assert!(
        matches!(
            err,
            EnvError::Unsupported {
                capability: "verifier_bound.admission",
                ..
            }
        ),
        "expected Unsupported{{verifier_bound.admission}}, got {err:?}"
    );
}

// ── AC-1/AC-2: the declared admission relaxes the named task only ────────────

/// The named task's spec carries the admitted decl; the deadline
/// derivation yields the relaxed bound verbatim (`min(900, 900) ·
/// 1000`), and the admission record echoes before → after, basis,
/// scope, prior disposition, epoch, envelope check, authorization.
#[test]
fn admit_relaxes_the_named_task_verifier_bound() {
    let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
    admit_verifier_bound(&mut record, &admission()).unwrap();
    assert_eq!(record.ext.get(SPEC_FIELD), Some(&Json::Int(900)));
    assert_eq!(enforced_decl_sec(&record).unwrap(), 900);
    assert_eq!(verifier_deadline_ms(900), 900_000);
    let Some(Json::Obj(adm)) = record.ext.get(ADMISSION_FIELD).cloned() else {
        panic!("verifier_bound_admission record missing");
    };
    assert_eq!(adm.get("task_id"), Some(&Json::str("swe-bench-astropy-2")));
    assert_eq!(adm.get("enforced_before_sec"), Some(&Json::Int(360)));
    assert_eq!(adm.get("admitted_sec"), Some(&Json::Int(900)));
    assert_eq!(adm.get("deadline_before_ms"), Some(&Json::Int(360_000)));
    assert_eq!(adm.get("deadline_after_ms"), Some(&Json::Int(900_000)));
    for member in [
        "basis",
        "prior_disposition",
        "supplement_epoch",
        "scope",
        "envelope_check",
        "authorization",
    ] {
        assert!(
            matches!(adm.get(member), Some(Json::Str(s)) if !s.is_empty()),
            "admission record member `{member}` missing or empty"
        );
    }
}

/// `verifier_deadline_ms` is the recorded driver derivation verbatim —
/// `min(declared, 900 s) · 1000` — the cap binds admitted and
/// unadmitted decls alike.
#[test]
fn verifier_deadline_ms_is_the_driver_derivation() {
    assert_eq!(verifier_deadline_ms(60), 60_000);
    assert_eq!(verifier_deadline_ms(360), 360_000);
    assert_eq!(verifier_deadline_ms(900), 900_000);
    assert_eq!(verifier_deadline_ms(1800), 900_000);
    assert_eq!(verifier_deadline_ms(0), 0);
}

/// A spec carrying no decl member enforces the spec-gen `60` default —
/// the driver's `jf("max_test_timeout_sec", 60.0)` verbatim.
#[test]
fn absent_decl_enforces_the_spec_gen_default() {
    let record = cell_record(Some("swe-bench-astropy-2"), None);
    assert_eq!(enforced_decl_sec(&record).unwrap(), SPEC_GEN_DEFAULT_SEC);
    assert_eq!(verifier_deadline_ms(SPEC_GEN_DEFAULT_SEC), 60_000);
}

/// The enforced-decl reader also honors the task-cfg float spelling
/// (`"360.0"` → `360`) — the spec surface's declared forms.
#[test]
fn str_decl_member_reads_verbatim() {
    let record = cell_record(Some("swe-bench-astropy-2"), Some(Json::str("360.0")));
    assert_eq!(enforced_decl_sec(&record).unwrap(), 360);
}

// ── Refusals — the scoped legs stay closed ───────────────────────────────────

/// A spec for a different task refuses typed and keeps its enforced
/// decl byte-identically — the bound relaxes only for the named task.
#[test]
fn other_task_refuses_and_keeps_the_enforced_decl() {
    let mut record = cell_record(Some("build-linux-kernel-qemu"), Some(Json::Int(360)));
    let before = record.ext.clone();
    let err = admit_verifier_bound(&mut record, &admission()).unwrap_err();
    expect_unsupported(err);
    assert_eq!(record.ext, before);
    assert_eq!(record.ext.get(SPEC_FIELD), Some(&Json::Int(360)));
}

/// A spec with no `task_id` member cannot verify the named-task scope
/// — fail-closed, never presumed.
#[test]
fn missing_task_id_member_refuses() {
    let mut record = cell_record(None, Some(Json::Int(360)));
    let before = record.ext.clone();
    let err = admit_verifier_bound(&mut record, &admission()).unwrap_err();
    expect_unsupported(err);
    assert_eq!(record.ext, before);
}

/// The recorded run ledger's stated `60` cannot be stamped onto a spec
/// that actually enforced `360` — the admission names the *actually
/// enforced* decl verbatim, never presumes.
#[test]
fn mislabeled_before_refuses() {
    let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
    let before = record.ext.clone();
    let err = admit_verifier_bound(
        &mut record,
        &VerifierBoundAdmission {
            enforced_before_sec: 60,
            ..admission()
        },
    )
    .unwrap_err();
    expect_unsupported(err);
    assert_eq!(record.ext, before);
}

/// The frozen §9.2 verifier envelope is the cap — `901` refuses, the
/// `900` cap itself admits.
#[test]
fn beyond_the_frozen_envelope_refuses() {
    let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
    let before = record.ext.clone();
    let err = admit_verifier_bound(
        &mut record,
        &VerifierBoundAdmission {
            admitted_sec: VERIFIER_BOUND_CAP_SEC + 1,
            ..admission()
        },
    )
    .unwrap_err();
    expect_unsupported(err);
    assert_eq!(record.ext, before);
}

/// An "admission" that does not relax the enforced decl is refused —
/// equal or tighter bounds are not this machinery.
#[test]
fn non_relaxing_admission_refuses() {
    for admitted in [360, 100] {
        let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
        let before = record.ext.clone();
        let err = admit_verifier_bound(
            &mut record,
            &VerifierBoundAdmission {
                admitted_sec: admitted,
                ..admission()
            },
        )
        .unwrap_err();
        expect_unsupported(err);
        assert_eq!(record.ext, before);
    }
}

/// The verbatim record members are required — an empty member refuses
/// before any stamp.
#[test]
fn empty_verbatim_members_refuse() {
    for a in [
        VerifierBoundAdmission {
            task_id: String::new(),
            ..admission()
        },
        VerifierBoundAdmission {
            basis: String::new(),
            ..admission()
        },
        VerifierBoundAdmission {
            prior_disposition: String::new(),
            ..admission()
        },
        VerifierBoundAdmission {
            supplement_epoch: String::new(),
            ..admission()
        },
        VerifierBoundAdmission {
            authorization: String::new(),
            ..admission()
        },
        VerifierBoundAdmission {
            scope: String::new(),
            ..admission()
        },
    ] {
        let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
        let before = record.ext.clone();
        expect_unsupported(admit_verifier_bound(&mut record, &a).unwrap_err());
        assert_eq!(record.ext, before);
    }
}

/// A malformed decl member refuses — an unreadable bound never
/// silently becomes the default.
#[test]
fn malformed_decl_member_refuses() {
    for bad in [
        Json::Bool(true),
        Json::Arr(vec![]),
        Json::str("not-a-number"),
    ] {
        let mut record = cell_record(Some("swe-bench-astropy-2"), Some(bad));
        expect_unsupported(enforced_decl_sec(&record).unwrap_err());
        let before = record.ext.clone();
        expect_unsupported(admit_verifier_bound(&mut record, &admission()).unwrap_err());
        assert_eq!(record.ext, before);
    }
}

// ── AC-2 fails-without + the verbatim row echo ───────────────────────────────

/// Absent the admission there is no relaxation path: the spec keeps
/// the enforced decl, the deadline derives `min(declared, 900 s)`
/// unrelaxed, and `slot_echo` carries nothing — never a silent bound
/// change.
#[test]
fn absent_admission_the_bound_stays_the_enforced_decl() {
    let record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
    assert!(record.ext.get(ADMISSION_FIELD).is_none());
    assert_eq!(enforced_decl_sec(&record).unwrap(), 360);
    assert_eq!(
        verifier_deadline_ms(enforced_decl_sec(&record).unwrap()),
        360_000
    );
    assert!(slot_echo(&record).is_empty());
}

/// After admission the per-row echo carries the admitted bound,
/// `prior_disposition`, and `supplement_epoch` verbatim — the members
/// the re-run's slot rows record while the original inconclusive
/// records stand untouched.
#[test]
fn slot_echo_carries_the_verbatim_row_members() {
    let mut record = cell_record(Some("swe-bench-astropy-2"), Some(Json::Int(360)));
    admit_verifier_bound(&mut record, &admission()).unwrap();
    let echo: BTreeMap<&str, Json> = slot_echo(&record).into_iter().collect();
    assert_eq!(echo.get("verifier_bound_admitted"), Some(&Json::Int(900)));
    assert_eq!(
        echo.get("prior_disposition"),
        Some(&Json::str(
            "inconclusive{verifier_unavailable:no_parseable_verdict}"
        ))
    );
    assert_eq!(
        echo.get("supplement_epoch"),
        Some(&Json::str("verifier-bound-rerun"))
    );
}
