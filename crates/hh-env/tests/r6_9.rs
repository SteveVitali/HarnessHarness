//! R6.9 — the AR-07 task-declared deadline battery + the AR-06
//! record-side legs (REQ-XB-118/117/107): the separate admitted path
//! enforces declared × multiplier under the declared protocol ceiling
//! while `verifier_bound`'s legacy `min(declared, 900 s)` derivation
//! stays byte-identical (the r5_57 battery covers it); the cell-spec
//! ext decode and the digest-only task-image admission refuse typed.

use std::collections::{BTreeMap, BTreeSet};

use hharness_env::cell_spec::{task_image_ref, CellSpec, SPEC_FIELD};
use hharness_env::errors::EnvError;
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::task_deadline::{
    admit_task_deadlines, task_deadline_echo, task_declared_deadline_ms, PhaseDeadline,
    TaskDeadlineAdmission, ADMISSION_FIELD,
};
use hharness_wire::json::Json;

fn cell_record(task_id: Option<&str>, ext: BTreeMap<String, Json>) -> EnvironmentRecord {
    let mut e = ext;
    if let Some(t) = task_id {
        e.insert("task_id".to_string(), Json::str(t));
    }
    EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("localhost/t:latest".to_string()),
        build_context: None,
        platform: "linux/arm64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("p".to_string(), "v".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: e,
        canary_channels: vec![],
    }
}

fn admission() -> TaskDeadlineAdmission {
    TaskDeadlineAdmission {
        task_id: "build-pov-ray".to_string(),
        dataset_pin: "terminal-bench-2-1@sha256:7d7bdc1c".to_string(),
        task_digest: "sha256:task".to_string(),
        protocol_id: "protocol:tb21".to_string(),
        agent: PhaseDeadline {
            declared_sec: 12_000,
            multiplier_ppm: 1_000_000,
        },
        verifier: PhaseDeadline {
            declared_sec: 12_000,
            multiplier_ppm: 1_000_000,
        },
        ceiling_sec: 12_000,
    }
}

fn expect_deadline_refusal(err: EnvError) {
    assert!(
        matches!(
            err,
            EnvError::Unsupported {
                capability: "deadline.task_declared",
                ..
            }
        ),
        "expected deadline.task_declared refusal, got {err:?}"
    );
}

/// AC-4: declared 12 000 s × 1.0 is enforced at 12 000 000 ms under a
/// 12 000 s ceiling.
#[test]
fn declared_enforces_under_the_ceiling() {
    assert_eq!(
        task_declared_deadline_ms(12_000, 1_000_000, 12_000).unwrap(),
        12_000_000
    );
    // A setup multiplier other than 1.0 is declared per run.
    assert_eq!(
        task_declared_deadline_ms(600, 3_000_000, 12_000).unwrap(),
        1_800_000
    );
}

/// AC-4: a declaration above the ceiling, a multiplier pushing the
/// enforced value over it, and malformed declarations all refuse.
#[test]
fn above_ceiling_and_malformed_refuse() {
    expect_deadline_refusal(task_declared_deadline_ms(12_001, 1_000_000, 12_000).unwrap_err());
    expect_deadline_refusal(task_declared_deadline_ms(12_000, 2_000_000, 12_000).unwrap_err());
    expect_deadline_refusal(task_declared_deadline_ms(0, 1_000_000, 12_000).unwrap_err());
    expect_deadline_refusal(task_declared_deadline_ms(60, 0, 12_000).unwrap_err());
}

/// AC-4: the admission stamps both enforced phases + the verbatim
/// scope; the echo carries declared and enforced values.
#[test]
fn admission_stamps_and_echoes() {
    let mut r = cell_record(Some("build-pov-ray"), BTreeMap::new());
    admit_task_deadlines(&mut r, &admission()).unwrap();
    let Json::Obj(adm) = r.ext.get(ADMISSION_FIELD).unwrap().clone() else {
        panic!("no admission record")
    };
    assert_eq!(adm.get("source"), Some(&Json::str("task.toml")));
    assert_eq!(
        adm.get("dataset_pin"),
        Some(&Json::str("terminal-bench-2-1@sha256:7d7bdc1c"))
    );
    assert_eq!(adm.get("ceiling_sec"), Some(&Json::Int(12_000)));
    for phase in ["agent", "verifier"] {
        let p = adm.get(phase).unwrap();
        assert_eq!(p.get("declared_sec"), Some(&Json::Int(12_000)));
        assert_eq!(p.get("multiplier_ppm"), Some(&Json::Int(1_000_000)));
        assert_eq!(p.get("enforced_ms"), Some(&Json::Int(12_000_000)));
    }
    let echo: BTreeMap<&str, Json> = task_deadline_echo(&r).into_iter().collect();
    assert_eq!(
        echo.get("deadline_agent")
            .and_then(|j| j.get("enforced_ms")),
        Some(&Json::Int(12_000_000))
    );
    assert_eq!(echo.get("deadline_ceiling_sec"), Some(&Json::Int(12_000)));
    // No admission ⇒ no echo.
    assert!(task_deadline_echo(&cell_record(Some("x"), BTreeMap::new())).is_empty());
}

/// A refused admission is transactional — the spec stays byte-identical.
#[test]
fn refused_admission_changes_nothing() {
    for a in [
        TaskDeadlineAdmission {
            task_id: "other-task".to_string(),
            ..admission()
        },
        TaskDeadlineAdmission {
            dataset_pin: String::new(),
            ..admission()
        },
        TaskDeadlineAdmission {
            verifier: PhaseDeadline {
                declared_sec: 99_999,
                multiplier_ppm: 1_000_000,
            },
            ..admission()
        },
    ] {
        let mut r = cell_record(Some("build-pov-ray"), BTreeMap::new());
        let before = r.ext.clone();
        expect_deadline_refusal(admit_task_deadlines(&mut r, &a).unwrap_err());
        assert_eq!(r.ext, before);
    }
    // A spec with no task_id member fails closed.
    let mut r = cell_record(None, BTreeMap::new());
    expect_deadline_refusal(admit_task_deadlines(&mut r, &admission()).unwrap_err());
}

/// AC-3: a tag-only image reference refuses typed; a digest reference
/// records `ForeignDigest` with platform and the R1 label.
#[test]
fn digest_only_images() {
    let (image, echo) = task_image_ref(
        "docker.io/alexgshaw/fix-git@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "linux/arm64",
    )
    .unwrap();
    assert!(matches!(image, ImageRef::ForeignDigest { .. }));
    assert_eq!(echo.get("platform"), Some(&Json::str("linux/arm64")));
    assert_eq!(echo.get("reproducibility"), Some(&Json::str("R1")));
    for bad in [
        "alexgshaw/fix-git:20260403",
        "docker.io/alexgshaw/fix-git",
        "alexgshaw/fix-git@sha256:nothex",
        "latest",
    ] {
        assert!(task_image_ref(bad, "linux/arm64").is_err(), "{bad}");
    }
}

/// AC-2: `cell_spec` decodes strictly; helper args map verbatim; a
/// malformed spec refuses.
#[test]
fn cell_spec_decode_and_args() {
    let r = cell_record(Some("t"), BTreeMap::new());
    assert_eq!(CellSpec::from_record(&r).unwrap(), None);
    let mut ext = BTreeMap::new();
    ext.insert(
        SPEC_FIELD.to_string(),
        Json::obj([
            ("workdir", Json::str("/app/x")),
            ("cpus", Json::Int(4)),
            ("memory_mb", Json::Int(8192)),
        ]),
    );
    let r = cell_record(Some("t"), ext);
    let spec = CellSpec::from_record(&r).unwrap().unwrap();
    assert_eq!(
        spec.helper_args(),
        vec!["--workdir", "/app/x", "--cpus", "4", "--memory", "8192"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
    for bad in [
        Json::obj([
            ("workdir", Json::str("/x")),
            ("workdir_from_image", Json::Bool(true)),
        ]),
        Json::obj([("cpus", Json::str("many"))]),
        Json::obj([("memory_mb", Json::Int(0))]),
        Json::obj([("unknown", Json::Bool(true))]),
        Json::Bool(true),
    ] {
        let mut e = BTreeMap::new();
        e.insert(SPEC_FIELD.to_string(), bad);
        let r = cell_record(Some("t"), e);
        assert!(CellSpec::from_record(&r).is_err());
    }
}
