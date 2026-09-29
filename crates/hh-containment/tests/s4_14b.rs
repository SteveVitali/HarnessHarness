//! S4.14b acceptance coverage — the R-2.8.4¹ Stage-4 containment slice:
//!
//! - **AC-R-2.8.4-12 (AC-H4-12)** — the probe battery runs against every
//!   shipped `isolation_class` backend (`process_sandbox`, `namespaces`,
//!   `user_space_kernel`, `microvm`, `transparent_redirect`, `external`);
//!   any capture (a denied probe observed `allow`) fails the class.
//! - **AC-R-2.8.4-13 (AC-H4-13)** — the `container-installed` hosted
//!   boundary (`isolation_class = external`) reports `reported`
//!   enforcement evidence — never `probed`/`attested`.
//! - the `attested` evidence kind lands only under a substrate
//!   attestation; `for_policy`/`for_class` drive backend selection.

use hh_containment::attach::{
    attach, evidence_preview, AttachInput, AttachMode, AttachOutcome, PolicySlot,
};
use hh_containment::backend::{
    for_class, for_policy, Attestation, BackendClass, ContainmentBackend, ModelBackend, NetPlane,
};
use hh_containment::policy::{kernel_default, EgressRule, HostPattern, NetMode, RuleDecision};
use hh_containment::report::{EnforcementEvidence, FieldGroup, ProbeKind};
use hh_hir::leaves::Text;
use hh_provenance::{AuthorityClass, ProvenanceRecord};

use hh_containment::backend::{GateVerdict, Syscall};

fn att() -> Attestation {
    Attestation {
        method: "guest_quote".to_string(),
        attestation_ref: "sha256:".to_string() + &"c".repeat(64),
    }
}

/// A principal-authority policy at the given isolation class.
fn policy_at(
    class: hh_containment::policy::IsolationClass,
) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    p.provenance.authority = AuthorityClass::Principal;
    p.proc.isolation_class = class;
    p.fs.write.allow.push(hh_containment::policy::WritableRoot {
        root: "workspace".to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.compute_ids();
    p
}

fn input<'a>(
    policy: &'a hh_containment::policy::ContainmentPolicy,
    backend: &'a dyn hh_containment::backend::ContainmentBackend,
) -> AttachInput<'a> {
    AttachInput {
        env_handle: "env:test",
        policy: PolicySlot::Inline(Box::new(policy.clone())),
        backend: Some(backend),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[],
    }
}

// ── backend selection ───────────────────────────────────────────────────────

#[test]
fn for_policy_maps_isolation_class_to_backend() {
    for (class, expect_backend) in [
        (
            hh_containment::policy::IsolationClass::ProcessSandbox,
            "ep2_model",
        ),
        (
            hh_containment::policy::IsolationClass::Namespaces,
            "namespaces_model",
        ),
        (
            hh_containment::policy::IsolationClass::UserSpaceKernel,
            "user_space_kernel_model",
        ),
        (
            hh_containment::policy::IsolationClass::Microvm,
            "microvm_model",
        ),
    ] {
        let p = policy_at(class);
        let need = matches!(
            class,
            hh_containment::policy::IsolationClass::UserSpaceKernel
                | hh_containment::policy::IsolationClass::Microvm
        );
        let b = for_policy(&p, need.then(att)).unwrap();
        assert_eq!(b.name(), expect_backend, "class {class:?}");
        assert_eq!(b.isolation_class(), class);
    }
}

#[test]
fn attested_classes_refuse_without_attestation() {
    for class in [
        hh_containment::policy::IsolationClass::UserSpaceKernel,
        hh_containment::policy::IsolationClass::Microvm,
    ] {
        let p = policy_at(class);
        match for_policy(&p, None) {
            Err(u) => assert_eq!(u.reason, "attestation_missing"),
            Ok(_) => panic!("{class:?} selected without an attestation"),
        }
    }
}

// ── AC-R-2.8.4-12: the battery against every shipped class ──────────────────

#[test]
fn ac_h4_12_battery_runs_against_each_shipped_class() {
    let backends: Vec<Box<dyn hh_containment::backend::ContainmentBackend>> = vec![
        for_class(BackendClass::ProcessSandbox, None).unwrap(),
        for_class(BackendClass::Namespaces, None).unwrap(),
        for_class(BackendClass::UserSpaceKernel, Some(att())).unwrap(),
        for_class(BackendClass::Microvm, Some(att())).unwrap(),
        for_class(BackendClass::TransparentRedirect, None).unwrap(),
    ];
    for b in &backends {
        // `mediated`/`none` carry the deny probes; run both modes.
        for mode in [NetMode::None, NetMode::Mediated] {
            let mut p = policy_at(b.isolation_class());
            p.net.mode = mode;
            p.compute_ids();
            let out = attach(&input(&p, &**b)).unwrap();
            let AttachOutcome::Applied { report, .. } = out else {
                panic!("{}: attach degraded/refused", b.name())
            };
            // No denied probe may have been observed allowed — a capture
            // is a readiness blocker for the class.
            for pr in &report.probes {
                assert!(
                    pr.passed(),
                    "{}: probe {:?} captured ({:?} expected {:?})",
                    b.name(),
                    pr.kind,
                    pr.observed,
                    pr.expected
                );
            }
            assert_ne!(
                report.evidence(FieldGroup::Fs),
                EnforcementEvidence::Unknown,
                "{}: fs evidence unknown",
                b.name()
            );
        }
    }
}

#[test]
fn attested_evidence_lands_only_under_a_substrate_attestation() {
    let p = policy_at(hh_containment::policy::IsolationClass::UserSpaceKernel);
    let b = ModelBackend::user_space_kernel(att());
    let map = evidence_preview(&b, &p).unwrap();
    assert_eq!(map[&FieldGroup::Fs], EnforcementEvidence::Attested);
    assert_eq!(map[&FieldGroup::Net], EnforcementEvidence::Attested);
    assert_eq!(map[&FieldGroup::Proc], EnforcementEvidence::Attested);
    // `resources` is never attested (the substrate measures the boundary,
    // not the counters) — declared enforcement stays `reported`.
    assert_eq!(map[&FieldGroup::Resources], EnforcementEvidence::Reported);

    // A namespaces model carries no attestation — `probed`, not `attested`.
    let p2 = policy_at(hh_containment::policy::IsolationClass::Namespaces);
    let map = evidence_preview(&ModelBackend::namespaces(), &p2).unwrap();
    assert_eq!(map[&FieldGroup::Fs], EnforcementEvidence::Probed);
}

// ── the transparent-redirect backend class ──────────────────────────────────

#[test]
fn transparent_redirect_mediated_allows_rule_covered_hosts_only() {
    let mut p = policy_at(hh_containment::policy::IsolationClass::Namespaces);
    p.net.mode = NetMode::Mediated;
    p.net.rules.push(EgressRule {
        host: HostPattern::Exact("allowed.example".to_string()),
        ports: vec![],
        protocols: vec![],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: Some(Text::new("t", "t", ProvenanceRecord::kernel("t", 0))),
        provenance: None,
    });
    p.compute_ids();
    let b = ModelBackend::transparent_redirect();
    assert_eq!(b.net_plane(), NetPlane::TransparentRedirect);

    assert_eq!(
        b.gate(
            &Syscall::Connect {
                target: "allowed.example:443".into()
            },
            &p
        ),
        GateVerdict::Allow,
        "a rule-covered host completes through the redirect"
    );
    assert!(matches!(
        b.gate(
            &Syscall::Connect {
                target: "denied.example:443".into()
            },
            &p
        ),
        GateVerdict::Deny { .. }
    ));
    // The battery still lands `probed` — the bridged-socket probe is
    // inapplicable on this plane (skipped, not failed).
    let out = attach(&input(&p, &b)).unwrap();
    let AttachOutcome::Applied { report, .. } = out else {
        panic!("attach degraded")
    };
    assert_eq!(
        report.evidence(FieldGroup::Net),
        EnforcementEvidence::Probed
    );
    assert!(!report
        .probes
        .iter()
        .any(|r| r.kind == ProbeKind::BridgedUnixSocket));
}

// ── AC-R-2.8.4-13: the hosted `external` boundary ───────────────────────────

#[test]
fn ac_h4_13_external_boundary_reports_reported_never_probed() {
    let p = policy_at(hh_containment::policy::IsolationClass::External);
    let b = ModelBackend::external();
    let out = attach(&input(&p, &b)).unwrap();
    let AttachOutcome::Applied { report, .. } = out else {
        panic!("hosted attach degraded")
    };
    assert_eq!(
        report.isolation_class,
        hh_containment::policy::IsolationClass::External
    );
    // Every enforced group is `reported` — the participant's own
    // declaration; no probes run inside the opaque boundary.
    for g in FieldGroup::ALL {
        assert_eq!(
            report.evidence(g),
            EnforcementEvidence::Reported,
            "group {g:?}"
        );
    }
    assert!(report.probes.is_empty());
    // The same policy's gate honestly answers Unenforced.
    assert_eq!(
        b.gate(&Syscall::FsRead { path: "x".into() }, &p),
        GateVerdict::Unenforced
    );
    // And a hosted boundary refuses a non-external class claim.
    let mut p2 = p.clone();
    p2.proc.isolation_class = hh_containment::policy::IsolationClass::Namespaces;
    assert!(b.apply(&p2).is_err());
}
