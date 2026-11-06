//! R5.56 — the `declared_deps` egress-profile regression battery
//! (ADR-0421). Deterministic legs only (offline, USD $0):
//!
//! - AC-2: `podman_helper_args` derives the helper's podman `--network`
//!   flag from the attached containment policy — `net.mode = public`
//!   yields `--network bridge`; `none`/`mediated` keep the pre-variant
//!   argv byte-identical (the flag absent ⇒ the helper's `none` default).
//! - AC-3: `admit_declared_deps` records `cell_network: bridge` +
//!   `policy_net_mode: public` + the `residual_channels[host_bridge]`
//!   declaration verbatim on the cell spec; a public-mode policy without
//!   a declared residual still fails `PublicWithoutResidual`; refused
//!   admissions leave record and policy untouched.
//! - AC-4 (fails-without): without the admission/`--network` plumbing a
//!   public policy produces no bridge argv leg and no profile stamps.

use std::collections::{BTreeMap, BTreeSet};

use hharness_containment::attach::AttachError;
use hharness_containment::policy::{kernel_default, NetMode, PolicyError, ResidualKind};
use hharness_env::egress_profile::{admit_declared_deps, podman_helper_args, DeclaredDeps};
use hharness_env::errors::EnvError;
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_provenance::AuthorityClass;
use hharness_wire::json::Json;

fn cell_record(class: EnvironmentClass) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image: ImageRef::Tag("docker.io/library/alpine:3.20".to_string()),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

fn decl() -> DeclaredDeps {
    DeclaredDeps {
        deps: vec!["uv add pytest".to_string(), "apt-get install".to_string()],
        owner: "s4-egress-supplement (operator-authorized bounded egress profile)".to_string(),
        closer: "cell teardown at slot end (the helper owns the container's lifecycle)".to_string(),
    }
}

fn ext_str(record: &EnvironmentRecord, key: &str) -> String {
    match record.ext.get(key) {
        Some(Json::Str(s)) => s.clone(),
        other => panic!("ext[{key}] missing or not a string: {other:?}"),
    }
}

// ── AC-2: policy-driven cell network ─────────────────────────────────────────

/// A `public` policy selects `--network bridge` in the helper argv; the
/// `none`/`mediated` postures produce argv byte-identical to the
/// pre-variant `[--container, --image, --workspace]` triple.
#[test]
fn helper_args_derive_network_from_the_policy() {
    let base = [
        "--container".to_string(),
        "hh-env-e1".to_string(),
        "--image".to_string(),
        "alpine:3.20".to_string(),
        "--workspace".to_string(),
        "/tmp/ws".to_string(),
    ];
    for mode in [NetMode::None, NetMode::Mediated] {
        let mut p = kernel_default(0);
        p.net.mode = mode;
        p.compute_ids();
        let argv = podman_helper_args("hh-env-e1", "alpine:3.20", "/tmp/ws", &p);
        assert_eq!(argv, base, "{mode:?} must keep the pre-variant argv");
        assert!(!argv.iter().any(|a| a == "--network"));
    }
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Public;
    p.residual_channels
        .push(hharness_containment::policy::ResidualChannel {
            kind: ResidualKind::HostBridge,
            statement: hharness_hir::leaves::Text::new(
                "declared bridge",
                "test",
                hharness_provenance::ProvenanceRecord::kernel("t", 0),
            ),
            owner: "o".to_string(),
            closer: "c".to_string(),
        });
    p.compute_ids();
    let argv = podman_helper_args("hh-env-e1", "alpine:3.20", "/tmp/ws", &p);
    assert_eq!(
        argv,
        [
            "--container",
            "hh-env-e1",
            "--image",
            "alpine:3.20",
            "--workspace",
            "/tmp/ws",
            "--network",
            "bridge"
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
    );
}

// ── AC-3: the declared admission + N4 honesty ────────────────────────────────

/// The admission mutates the policy to the honest `public` declaration —
/// `host_bridge` residual with owner/closer/statement verbatim — and
/// stamps the supplement's field spellings on the cell spec.
#[test]
fn admit_declared_deps_produces_the_honest_policy() {
    let mut record = cell_record(EnvironmentClass::LocalContainer);
    let mut policy = kernel_default(0);
    admit_declared_deps(&mut record, &mut policy, &decl(), 7).unwrap();
    assert_eq!(policy.net.mode, NetMode::Public);
    assert_eq!(policy.residual_channels.len(), 1);
    let r = &policy.residual_channels[0];
    assert_eq!(r.kind, ResidualKind::HostBridge);
    assert_eq!(r.owner, decl().owner);
    assert_eq!(r.closer, decl().closer);
    let statement = r.statement.content.clone().unwrap();
    assert!(statement.contains("egress_profile: declared_deps"));
    assert!(statement.contains("--network bridge"));
    assert!(statement.contains("uv add pytest, apt-get install"));
    // The admitted policy is itself valid (kernel authority ≥ principal).
    policy.validate().unwrap();
    // The cell spec records the supplement's field spellings verbatim.
    assert_eq!(ext_str(&record, "egress_profile"), "declared_deps");
    assert_eq!(ext_str(&record, "cell_network"), "bridge");
    assert_eq!(ext_str(&record, "policy_net_mode"), "public");
    match record.ext.get("declared_deps") {
        Some(Json::Arr(v)) => assert_eq!(v.len(), 2),
        other => panic!("declared_deps not recorded verbatim: {other:?}"),
    }
}

/// A task with no declared runtime egress deps is refused typed — the
/// cell keeps `none`; the refusal touches neither record nor policy.
#[test]
fn undeclared_task_refuses_and_stays_none() {
    let mut record = cell_record(EnvironmentClass::LocalContainer);
    let mut policy = kernel_default(0);
    let before = policy.clone();
    let err = admit_declared_deps(
        &mut record,
        &mut policy,
        &DeclaredDeps {
            deps: vec![],
            ..decl()
        },
        0,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        EnvError::Unsupported {
            capability: "egress_profile.declared_deps",
            ..
        }
    ));
    assert_eq!(policy, before);
    assert_eq!(policy.net.mode, NetMode::None);
    assert!(record.ext.is_empty());
}

/// An unowned/unclosed residual is refused before any mutation.
#[test]
fn unowned_residual_refuses() {
    let mut record = cell_record(EnvironmentClass::LocalContainer);
    let mut policy = kernel_default(0);
    for d in [
        DeclaredDeps {
            owner: String::new(),
            ..decl()
        },
        DeclaredDeps {
            closer: String::new(),
            ..decl()
        },
    ] {
        let err = admit_declared_deps(&mut record, &mut policy, &d, 0).unwrap_err();
        assert!(matches!(err, EnvError::Unsupported { .. }));
        assert_eq!(policy.net.mode, NetMode::None);
        assert!(record.ext.is_empty());
    }
}

/// The profile admits a podman cell only — a non-container class refuses.
#[test]
fn non_container_class_refuses() {
    for class in [
        EnvironmentClass::LocalHost,
        EnvironmentClass::LocalSandboxed,
        EnvironmentClass::RemoteEphemeral,
    ] {
        let mut record = cell_record(class);
        let mut policy = kernel_default(0);
        let err = admit_declared_deps(&mut record, &mut policy, &decl(), 0).unwrap_err();
        assert!(matches!(
            err,
            EnvError::Unsupported {
                capability: "egress_profile.declared_deps",
                ..
            }
        ));
        assert_eq!(policy.net.mode, NetMode::None);
        assert!(record.ext.is_empty());
    }
}

/// N4 stands: `mode = public` without a declared residual still fails
/// `PublicWithoutResidual` — the validator is consulted, never bypassed.
#[test]
fn public_without_residual_still_refuses() {
    let mut policy = kernel_default(0);
    policy.net.mode = NetMode::Public;
    policy.compute_ids();
    assert_eq!(policy.validate(), Err(PolicyError::PublicWithoutResidual));
    // And a below-principal layer cannot declare public either — the
    // admission surfaces the typed refusal, not a half-mutated policy.
    let mut low = kernel_default(0);
    low.provenance.authority = AuthorityClass::Delegate;
    let before = low.clone();
    let mut record = cell_record(EnvironmentClass::LocalContainer);
    let err = admit_declared_deps(&mut record, &mut low, &decl(), 0).unwrap_err();
    match err {
        EnvError::Attach(AttachError::PolicyInvalid(PolicyError::PublicBelowPrincipal)) => {}
        other => panic!("expected Attach(PolicyInvalid(PublicBelowPrincipal)), got {other:?}"),
    }
    assert_eq!(low, before);
    assert!(record.ext.is_empty());
}

/// AC-4 leg: absent the admission, a `none`-posture cell stamps nothing
/// and selects no bridge — the profile is never inferred.
#[test]
fn absent_admission_stamps_nothing() {
    let record = cell_record(EnvironmentClass::LocalContainer);
    assert!(record.ext.is_empty());
    let policy = kernel_default(0);
    let argv = podman_helper_args("hh-env-e1", "alpine:3.20", "/tmp/ws", &policy);
    assert!(!argv.iter().any(|a| a == "--network" || a == "bridge"));
}
