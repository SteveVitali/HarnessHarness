//! Acceptance + unit tests for `hh-containment` (ticket S1.12; R-2.8.4 /
//! §5g.4, the C0/Stage-1 slice).
//!
//! Coverage:
//! - **AC-R-2.8.4-1** (AC-H4-01, Stage-1 static) — write containment,
//!   protected metadata and symlink-before-validate deny at the EP2 gate,
//!   observed as `violated{evidence_kind}`-shaped verdicts.
//! - **AC-R-2.8.4-3** (AC-H4-03) — the `none`-mode probe battery: every
//!   connect-class syscall fails, only the bridged `AF_UNIX` channel
//!   succeeds, evidence recorded `probed`.
//! - **AC-R-2.8.4-8** (AC-H4-08) — fail-closed attach: helper absent,
//!   backend unsupported, stale report, ref mismatch, relied-on `unknown`;
//!   `warn_and_degrade` stamps `containment_degraded`; Lab runs refuse it.
//! - **AC-R-2.8.4-9** (AC-H4-09) — the layered-meet property test over
//!   seeded random stacks.
//! - **AC-R-2.8.4-10** (AC-H4-10, static) — `lowering_loss` names every
//!   unenforceable relied-on field; a relied-on loss fails the attach.
//! - **AC-R-2.8.4-14** (ADR-0062 D3) — the `authorize` precondition:
//!   `ContainmentUnverified` before any check, a floor `refused`/`amendable`
//!   never consults Π or asks, `PermissionUnreachable` warns at attach.

use std::collections::{BTreeMap, BTreeSet};

use hh_containment::admit::{
    admit_input, admits, floor_gate, unreachable_permissions, workspace_scope, AdmitInput,
    AdmitVerdict, ContainmentDiff, RefusedReason,
};
use hh_containment::attach::{
    attach, relied_groups, AttachError, AttachInput, AttachMode, AttachOutcome, PolicySlot,
    SealWarning,
};
use hh_containment::backend::{
    BackendCaps, ContainmentBackend, Ep2Model, GateVerdict, Syscall, ViolationKind,
};
use hh_containment::events;
use hh_containment::meet::{effective, MeetError};
use hh_containment::paths;
use hh_containment::policy::{
    kernel_default, ContainmentPolicy, EgressRule, HostPattern, NetMode, ReadMode, ResidualChannel,
    ResidualKind, RuleDecision, WritableRoot, KERNEL_PROTECTED,
};
use hh_containment::probes;
use hh_containment::report::{
    verify_report, ContainmentReport, EnforcementEvidence, FieldGroup, ProbeKind, ProbeOutcome,
};
use hh_hir::kinds::EffectDomain;
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, ScopeBindings};
use hh_hir::refs::Ref;
use hh_hir::EffectClass;
use hh_monitor::args::CanonicalArgs;
use hh_monitor::assess::AssessmentInputs;
use hh_monitor::decision::{Decision, DenyReason};
use hh_monitor::monitor::{ContainmentGate, Monitor, Proposal};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_provenance::{AuthorityClass, Label, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixtures ──────────────────────────────────────────────────────────────────

/// A layer at the given authority (test fixture — the meet reads the
/// conferred class on `provenance.authority`; the record is the kernel
/// default's shape).
fn layer_at(a: AuthorityClass) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.provenance.authority = a;
    p.compute_ids();
    p
}

/// A principal layer with a writable root and `mediated` net — the
/// "permissive" fixture the happy-path tests run.
fn principal_workspace() -> ContainmentPolicy {
    let mut p = layer_at(AuthorityClass::Principal);
    p.net.mode = NetMode::Mediated;
    p.fs.write.allow.push(WritableRoot {
        root: "workspace".to_string(),
        read_only_subpaths: vec!["keep".to_string()],
        protected_metadata_names: vec![],
    });
    p.compute_ids();
    p
}

fn residual() -> ResidualChannel {
    ResidualChannel {
        kind: ResidualKind::HostBridge,
        statement: Text::new(
            "the bridged helper channel",
            "test",
            ProvenanceRecord::kernel("t", 0),
        ),
        closer: "ep2_model".to_string(),
        owner: "test".to_string(),
    }
}

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn deny_rule(host: &str) -> EgressRule {
    EgressRule {
        decision: RuleDecision::Deny,
        ..allow_rule(host)
    }
}

fn grant(domain: EffectDomain, scope: &str) -> Grant {
    Grant {
        effect: EffectClass::domain_only(domain),
        scope: scope.to_string(),
        constraints: GrantConstraints::default(),
        delegable: true,
    }
}

fn input(domain: EffectDomain, fs_paths: &[&str], hosts: &[&str]) -> AdmitInput {
    AdmitInput {
        domain: Some(domain),
        fs_paths: fs_paths.iter().map(|s| s.to_string()).collect(),
        hosts: hosts.iter().map(|s| s.to_string()).collect(),
        scope_unknown: false,
    }
}

/// Attach the policy on the reference backend (fail-closed, non-lab).
fn attach_ok(policy: &ContainmentPolicy) -> ContainmentReport {
    let backend = Ep2Model::reference();
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(policy.clone())),
        backend: Some(&backend),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[],
    };
    match attach(&input).unwrap() {
        AttachOutcome::Applied { report, .. } => report,
        AttachOutcome::Degraded { .. } => panic!("expected Applied"),
    }
}

// ── the policy record & codec ─────────────────────────────────────────────────

#[test]
fn kernel_default_is_the_deny_by_default_floor() {
    let p = kernel_default(0);
    p.validate().unwrap();
    assert_eq!(p.net.mode, NetMode::None);
    assert!(p.fs.write.allow.is_empty());
    assert_eq!(p.fs.read.mode, ReadMode::AllowAllExcept);
    for d in hh_containment::KERNEL_DENY {
        assert!(p.fs.read.deny.iter().any(|x| x == d));
    }
    for name in KERNEL_PROTECTED {
        assert!(p.fs.protected_metadata.contains(*name));
    }
    // `effective([])` is the kernel default — same semantic id (the
    // provenance restamp is excluded from the projection).
    let e = effective(&[], 9).unwrap();
    assert_eq!(e.policy_id, p.policy_id);
    assert_eq!(e.provenance.authority, AuthorityClass::Kernel);
}

#[test]
fn codec_round_trip_preserves_every_field() {
    let mut p = principal_workspace();
    p.net.rules = vec![deny_rule("bad.example"), allow_rule("ok.example")];
    p.resources.cpu_ms = Some(100);
    p.residual_channels.push(residual());
    p.ext.insert("x".into(), Json::Int(1));
    p.compute_ids();
    let back = ContainmentPolicy::from_json(&p.to_json()).unwrap();
    // The canonical form carries `content_hash`, never raw `content` —
    // decoded `Text` leaves lose `content` by design; the check is on the
    // canonical projection and the derived ids.
    assert_eq!(back.to_json(), p.to_json());
    assert_eq!(back.policy_id, p.policy_id);
    assert_eq!(back.version_id, p.version_id);
}

#[test]
fn semantic_id_ignores_spelling_and_provenance_version_covers_it() {
    let mut a = layer_at(AuthorityClass::Principal);
    a.net.mode = NetMode::Mediated;
    a.net.rules = vec![{
        let mut r = allow_rule("OK.Example.COM.");
        r.justification = Some(Text::new("why", "test", ProvenanceRecord::kernel("t", 0)));
        r
    }];
    a.compute_ids();
    let mut b = a.clone();
    // A different spelling of the same host + different prose/provenance.
    b.net.rules[0].host = HostPattern::parse("ok.example.com").unwrap();
    b.net.rules[0].justification = Some(Text::new(
        "reworded",
        "test",
        ProvenanceRecord::kernel("t", 1),
    ));
    b.provenance = ProvenanceRecord::kernel("other", 7);
    b.ext.insert("note".into(), Json::str("x"));
    b.compute_ids();
    assert_eq!(
        a.policy_id, b.policy_id,
        "semantic projection must ignore spelling/provenance/ext"
    );
    assert_ne!(a.version_id, b.version_id, "version covers provenance/ext");
}

// ── effective(): the layered meet ─────────────────────────────────────────────

#[test]
fn meet_narrowing_is_free_at_any_authority() {
    let mut d = layer_at(AuthorityClass::Delegate);
    d.fs.read.deny.push("secret".into());
    d.fs.write.deny_within_allow.push("tmp".into());
    d.net.local_binding = false;
    d.proc.env.exclude.push("AWS_*".into());
    let e = effective(&[d], 1).unwrap();
    assert!(e.fs.read.deny.contains(&"secret".to_string()));
    assert!(e.proc.env.exclude.contains(&"AWS_*".to_string()));
}

#[test]
fn meet_sub_principal_loosening_is_containment_widening() {
    for a in [
        AuthorityClass::Delegate,
        AuthorityClass::Environment,
        AuthorityClass::External,
        AuthorityClass::Unverified,
    ] {
        let mut l = layer_at(a);
        l.net.mode = NetMode::Mediated;
        match effective(&[l], 1) {
            Err(MeetError::Widening(w)) => {
                assert_eq!(w.field, "net.mode");
                assert_eq!(w.layer_authority, a);
                assert_eq!(w.required, AuthorityClass::Principal);
            }
            other => panic!("{a:?}: expected ContainmentWidening, got {other:?}"),
        }
        // A writable root is the same story.
        let mut l = layer_at(a);
        l.fs.write.allow.push(WritableRoot {
            root: "w".into(),
            read_only_subpaths: vec![],
            protected_metadata_names: vec![],
        });
        assert!(matches!(effective(&[l], 1), Err(MeetError::Widening(_))));
        // And a kernel-only field (`exec` → an allow list).
        let mut l = layer_at(a);
        l.fs.exec = hh_containment::policy::ExecPolicy::Any;
        assert!(matches!(effective(&[l], 1), Err(MeetError::Widening(_))));
    }
}

#[test]
fn meet_principal_and_definition_entitlements() {
    // principal: net.mode → mediated, writable roots, allow rules, strict→false.
    let mut p = layer_at(AuthorityClass::Principal);
    p.net.mode = NetMode::Mediated;
    p.net.rules = vec![allow_rule("ok.example")];
    p.fs.write.allow.push(WritableRoot {
        root: "workspace".into(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.amendment.strict = false;
    let e = effective(&[p], 1).unwrap();
    assert_eq!(e.net.mode, NetMode::Mediated);
    assert_eq!(e.fs.write.allow.len(), 1);
    assert!(!e.amendment.strict);

    // definition: allow rules *with* justification, mounts, resources.
    let mut d = layer_at(AuthorityClass::Definition);
    d.net.mode = NetMode::Mediated; // definition ≥ principal — mode entitled
    let mut r = allow_rule("cfg.example");
    r.justification = Some(Text::new("sealed", "t", ProvenanceRecord::kernel("t", 0)));
    d.net.rules = vec![r];
    d.resources.cpu_ms = Some(50);
    let e = effective(&[d], 1).unwrap();
    assert_eq!(e.resources.cpu_ms, Some(50));

    // A definition allow rule WITHOUT justification widens (the entitlement
    // is `definition` ∧ justified).
    let mut d2 = layer_at(AuthorityClass::Definition);
    d2.net.mode = NetMode::Mediated;
    d2.net.rules = vec![allow_rule("unjustified.example")];
    assert!(matches!(effective(&[d2], 1), Err(MeetError::Widening(_))));

    // A delegate allow rule widens even with justification.
    let mut g = layer_at(AuthorityClass::Delegate);
    g.net.mode = NetMode::Mediated;
    g.net.rules = vec![{
        let mut r = allow_rule("x.example");
        r.justification = Some(Text::new("please", "t", ProvenanceRecord::kernel("t", 0)));
        r
    }];
    assert!(matches!(effective(&[g], 1), Err(MeetError::Widening(_))));
}

#[test]
fn meet_protected_path_exemption_and_deny_survival() {
    // A layer missing a KERNEL_PROTECTED name is the exemption error.
    let mut l = layer_at(AuthorityClass::Principal);
    l.fs.protected_metadata.remove(KERNEL_PROTECTED[0]);
    match effective(&[l], 1) {
        Err(MeetError::ProtectedPathExemption { name }) => {
            assert_eq!(name, KERNEL_PROTECTED[0]);
        }
        other => panic!("expected ProtectedPathExemption, got {other:?}"),
    }

    // Deny entries survive across every layer; allow entries intersect.
    let mut k = layer_at(AuthorityClass::Kernel);
    k.net.mode = NetMode::Mediated;
    k.net.rules = vec![deny_rule("bad.example"), allow_rule("ok.example")];
    let mut d = layer_at(AuthorityClass::Delegate);
    d.net.mode = NetMode::Mediated;
    d.net.rules = vec![deny_rule("worse.example")];
    let e = effective(&[k, d], 1).unwrap();
    let hosts: Vec<String> = e
        .net
        .rules
        .iter()
        .map(|r| format!("{}:{:?}", r.host.spelling(), r.decision))
        .collect();
    assert!(hosts.iter().any(|h| h.starts_with("bad.example:Deny")));
    assert!(hosts.iter().any(|h| h.starts_with("worse.example:Deny")));
    // The kernel allow rule did not survive the delegate layer's omission —
    // allow sets intersect.
    assert!(!hosts.iter().any(|h| h.starts_with("ok.example:Allow")));
}

#[test]
fn meet_residuals_and_provenance_of_result() {
    let mut p = layer_at(AuthorityClass::Principal);
    p.net.mode = NetMode::Public;
    p.residual_channels.push(residual());
    let e = effective(&[p], 1).unwrap();
    assert_eq!(e.net.mode, NetMode::Public);
    assert_eq!(e.residual_channels.len(), 1);
    assert_eq!(e.provenance.authority, AuthorityClass::Kernel);
    e.validate().unwrap();
}

// ── admits() ──────────────────────────────────────────────────────────────────

#[test]
fn admits_net_none_refuses_mediated_checks_deny_then_allow() {
    let none = kernel_default(0);
    let i = input(EffectDomain::NetEgress, &[], &["any.example"]);
    assert!(matches!(
        admits(&none, &i),
        AdmitVerdict::Refused {
            reason: RefusedReason::ModeNone
        }
    ));

    let mut p = layer_at(AuthorityClass::Principal);
    p.net.mode = NetMode::Mediated;
    p.net.rules = vec![deny_rule("bad.example"), allow_rule("ok.example")];
    // deny before allow (N1): a deny-rule hit refuses…
    assert!(matches!(
        admits(&p, &input(EffectDomain::NetEgress, &[], &["bad.example"])),
        AdmitVerdict::Refused {
            reason: RefusedReason::DenyRule { .. }
        }
    ));
    // …even when an allow rule also covers it — deny is evaluated first.
    p.net.rules.push(allow_rule("bad.example"));
    assert!(matches!(
        admits(&p, &input(EffectDomain::NetEgress, &[], &["bad.example"])),
        AdmitVerdict::Refused { .. }
    ));
    // no allow coverage → amendable (the Stage-1 diff), not a silent allow.
    assert!(matches!(
        admits(&p, &input(EffectDomain::NetEgress, &[], &["other.example"])),
        AdmitVerdict::Amendable {
            diff: ContainmentDiff::AddEgressAllow { .. }
        }
    ));
    // covered → admitted.
    assert_eq!(
        admits(&p, &input(EffectDomain::NetEgress, &[], &["ok.example"])),
        AdmitVerdict::Admitted
    );
    // normalisation: a mixed-case/trailing-dot spelling matches.
    assert_eq!(
        admits(&p, &input(EffectDomain::NetEgress, &[], &["OK.Example."])),
        AdmitVerdict::Admitted
    );
}

#[test]
fn admits_fs_write_classifies_the_extent() {
    let p = principal_workspace();
    assert_eq!(
        admits(&p, &input(EffectDomain::FsWrite, &["workspace/a.txt"], &[])),
        AdmitVerdict::Admitted
    );
    // outside the root → amendable (a hatch could add the root).
    assert!(matches!(
        admits(&p, &input(EffectDomain::FsWrite, &["outside/x"], &[])),
        AdmitVerdict::Amendable {
            diff: ContainmentDiff::AddWritableRoot { .. }
        }
    ));
    // protected metadata inside the root → refused.
    assert!(matches!(
        admits(
            &p,
            &input(EffectDomain::FsWrite, &["workspace/.hh/hooks/x"], &[])
        ),
        AdmitVerdict::Refused {
            reason: RefusedReason::ProtectedPath { .. }
        }
    ));
    // the declared read-only subpath → refused.
    assert!(matches!(
        admits(
            &p,
            &input(EffectDomain::FsWrite, &["workspace/keep/a"], &[])
        ),
        AdmitVerdict::Refused {
            reason: RefusedReason::ReadOnlySubpath { .. }
        }
    ));
    // the kernel default (no roots) — amendable, never silently allowed.
    assert!(matches!(
        admits(
            &kernel_default(0),
            &input(EffectDomain::FsWrite, &["w/x"], &[])
        ),
        AdmitVerdict::Amendable { .. }
    ));
}

#[test]
fn admits_read_and_scope_unknown() {
    // allow_all_except: a deny-listed read refuses even without a scope.
    let p = kernel_default(0);
    assert!(matches!(
        admits(&p, &input(EffectDomain::FsRead, &["k/.ssh/id"], &[])),
        AdmitVerdict::Refused {
            reason: RefusedReason::DenyRule { .. }
        }
    ));
    // allow_only + unknown scope → unverifiable (never a silent admit).
    let mut p2 = layer_at(AuthorityClass::Kernel);
    p2.fs.read.mode = ReadMode::AllowOnly;
    p2.fs.read.allow = vec!["docs".into()];
    let mut i = input(EffectDomain::FsRead, &[], &[]);
    i.scope_unknown = true;
    assert!(matches!(
        admits(&p2, &i),
        AdmitVerdict::Refused {
            reason: RefusedReason::Unverifiable
        }
    ));
    i.scope_unknown = false;
    i.fs_paths = vec!["docs/a".into()];
    assert_eq!(admits(&p2, &i), AdmitVerdict::Admitted);
    i.fs_paths = vec!["other/a".into()];
    assert!(matches!(admits(&p2, &i), AdmitVerdict::Amendable { .. }));
}

#[test]
fn unreachable_permissions_warns_never_refuses() {
    let p = kernel_default(0);
    let grants = vec![
        ("perm-1".to_string(), grant(EffectDomain::NetEgress, "*")),
        ("perm-2".to_string(), grant(EffectDomain::FsRead, "*")),
    ];
    let w = unreachable_permissions(&p, &grants);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].permission, "perm-1");
    assert_eq!(w[0].domain, EffectDomain::NetEgress);
}

// ── AC-R-2.8.4-1/-3 — the EP2 gate and the probe battery ─────────────────────

#[test]
fn ac1_write_containment_protected_and_symlink_escape_deny() {
    let p = principal_workspace();
    let b = Ep2Model::reference();
    // outside the root
    assert!(matches!(
        b.gate(
            &Syscall::FsWrite {
                path: "outside/x".into()
            },
            &p
        ),
        GateVerdict::Deny {
            kind: ViolationKind::FsWrite,
            ..
        }
    ));
    // a KERNEL_PROTECTED name inside the root
    assert!(matches!(
        b.gate(
            &Syscall::FsWrite {
                path: "workspace/.hh/hooks/pre".into()
            },
            &p
        ),
        GateVerdict::Deny {
            kind: ViolationKind::FsWrite,
            ..
        }
    ));
    // F1 — a symlink inside the root whose target escapes it: resolution
    // precedes validation, the write denies.
    let b = b.with_link("workspace/link", "outside");
    assert!(matches!(
        b.gate(
            &Syscall::FsWrite {
                path: "workspace/link/x".into()
            },
            &p
        ),
        GateVerdict::Deny { .. }
    ));
    // the denied verdict maps onto a `violated` payload with evidence_kind.
    let ev = events::violated_payload(
        "ep2",
        b.name(),
        ViolationKind::FsWrite,
        "workspace/.hh/hooks/pre",
        hh_containment::report::EvidenceKind::KernelLog,
        Some("e1"),
    );
    assert_eq!(ev.get("kind").and_then(Json::as_str), Some("fs_write"));
    assert_eq!(
        ev.get("evidence_kind").and_then(Json::as_str),
        Some("kernel_log")
    );
    assert_eq!(ev.get("effect_id").and_then(Json::as_str), Some("e1"));
    // a read-deny path fails the read gate.
    assert!(matches!(
        b.gate(
            &Syscall::FsRead {
                path: "k/.ssh/id".into()
            },
            &p
        ),
        GateVerdict::Deny {
            kind: ViolationKind::FsRead,
            ..
        }
    ));
    // inside the root allows.
    assert_eq!(
        b.gate(
            &Syscall::FsWrite {
                path: "workspace/a".into()
            },
            &p
        ),
        GateVerdict::Allow
    );
}

#[test]
fn ac3_none_mode_battery_every_connect_fails_only_bridge_succeeds() {
    let p = kernel_default(0); // net.mode = none
    let b = Ep2Model::reference();
    let results = probes::run_battery(&b, &p);
    for r in &results {
        assert!(
            r.passed(),
            "probe {:?} expected {:?} got {:?}",
            r.kind,
            r.expected,
            r.observed
        );
    }
    let observed = |k: ProbeKind| results.iter().find(|r| r.kind == k).unwrap().observed;
    for k in [
        ProbeKind::Connect,
        ProbeKind::SendTo,
        ProbeKind::RawSocket,
        ProbeKind::NameResolve,
        ProbeKind::Icmp,
        ProbeKind::LocalBind,
    ] {
        assert_eq!(observed(k), ProbeOutcome::Deny, "{k:?}");
    }
    for k in [
        ProbeKind::IoUring,
        ProbeKind::Ptrace,
        ProbeKind::ProcessVm,
        ProbeKind::Setuid,
    ] {
        assert_eq!(observed(k), ProbeOutcome::Deny, "{k:?}");
    }
    assert_eq!(observed(ProbeKind::BridgedUnixSocket), ProbeOutcome::Allow);
    // The evidence records `probed` for every covered group.
    assert_eq!(
        probes::group_evidence(FieldGroup::Net, &results),
        EnforcementEvidence::Probed
    );
    assert_eq!(
        probes::group_evidence(FieldGroup::Proc, &results),
        EnforcementEvidence::Probed
    );
    assert_eq!(
        probes::group_evidence(FieldGroup::Fs, &results),
        EnforcementEvidence::Probed
    );
    // A mediated policy is the same at EP2 — the namespace is removed; only
    // the bridge succeeds.
    let mut m = layer_at(AuthorityClass::Principal);
    m.net.mode = NetMode::Mediated;
    let r2 = probes::run_battery(&b, &m);
    assert!(r2.iter().all(|r| r.passed()));
    assert_eq!(
        r2.iter()
            .find(|r| r.kind == ProbeKind::Connect)
            .unwrap()
            .observed,
        ProbeOutcome::Deny
    );
    // A local bind under `local_binding = true` allows (the probe's expected
    // tracks the policy).
    let mut lb = layer_at(AuthorityClass::Kernel);
    lb.net.local_binding = true;
    let r3 = probes::run_battery(&b, &lb);
    assert_eq!(
        r3.iter()
            .find(|r| r.kind == ProbeKind::LocalBind)
            .unwrap()
            .observed,
        ProbeOutcome::Allow
    );
    // A cap-off backend is honest: Unenforced, never Allow — and the group
    // evidence is `unknown`, not `probed`.
    let mut caps = BackendCaps::full();
    caps.enforce_net = false;
    let weak = Ep2Model::reference().with_caps(caps);
    let r4 = probes::run_battery(&weak, &p);
    assert_eq!(
        r4.iter()
            .find(|r| r.kind == ProbeKind::Connect)
            .unwrap()
            .observed,
        ProbeOutcome::Unenforced
    );
    assert_eq!(
        probes::group_evidence(FieldGroup::Net, &r4),
        EnforcementEvidence::Unknown
    );
}

// ── AC-R-2.8.4-8 — the fail-closed attach ─────────────────────────────────────

#[test]
fn ac8_attach_applied_produces_the_report_and_event() {
    let p = principal_workspace();
    let b = Ep2Model::reference();
    let g = grant(EffectDomain::NetEgress, "*");
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p.clone())),
        backend: Some(&b),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[("perm-1".to_string(), g)],
    };
    let out = attach(&input).unwrap();
    let AttachOutcome::Applied {
        report,
        event,
        warnings,
    } = out
    else {
        panic!("expected Applied");
    };
    // AC-14 third half: the net_egress grant is unreachable? — no: mode is
    // `mediated`, so the warning set is empty here.
    assert!(warnings.is_empty());
    assert_eq!(report.policy_version_id, p.version_id);
    assert_eq!(report.backend, "ep2_model");
    assert_eq!(report.evidence(FieldGroup::Fs), EnforcementEvidence::Probed);
    assert_eq!(
        report.evidence(FieldGroup::Net),
        EnforcementEvidence::Probed
    );
    // The applied payload carries the spec members; the refs are `sha256:`.
    for k in [
        "env_handle",
        "policy_version_id",
        "effective_policy_hash",
        "backend",
        "isolation_class",
        "enforcement_evidence",
        "lowering_loss_ref",
        "probes_ref",
    ] {
        assert!(event.get(k).is_some(), "missing {k}");
    }
    assert!(event
        .get("policy_version_id")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
}

fn base_input<'a>(
    policy: &ContainmentPolicy,
    backend: Option<&'a dyn ContainmentBackend>,
) -> AttachInput<'a> {
    AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(policy.clone())),
        backend,
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[],
    }
}

#[test]
fn ac8_fail_closed_fault_matrix() {
    let p = principal_workspace();
    let b = Ep2Model::reference();

    // helper absent
    match attach(&base_input(&p, None)).unwrap_err() {
        AttachError::Unverified {
            field_group,
            reason,
            event,
        } => {
            assert_eq!(field_group, "attach");
            assert_eq!(reason, "helper_unavailable");
            assert_eq!(
                event.get("field_group").and_then(Json::as_str),
                Some("attach")
            );
        }
        e => panic!("expected Unverified, got {e:?}"),
    }

    // backend unsupported — an isolation class above the EP2 model's.
    let mut strong = layer_at(AuthorityClass::Kernel);
    strong.proc.isolation_class = hh_containment::policy::IsolationClass::Microvm;
    strong.compute_ids();
    let mut i = base_input(&p, Some(&b));
    i.policy = PolicySlot::Inline(Box::new(strong));
    match attach(&i).unwrap_err() {
        AttachError::Unverified { reason, .. } => {
            assert!(reason.starts_with("backend_unsupported"), "{reason}");
        }
        e => panic!("expected Unverified, got {e:?}"),
    }

    // a stale stored report on resume
    let report = attach_ok(&p);
    let mut other = layer_at(AuthorityClass::Kernel);
    other.compute_ids();
    let mut i = base_input(&p, Some(&b));
    i.resume_report = Some(&report);
    i.policy = PolicySlot::Inline(Box::new(other));
    match attach(&i).unwrap_err() {
        AttachError::Unverified {
            field_group,
            reason,
            ..
        } => {
            assert_eq!(field_group, "report");
            assert_eq!(reason, "report_stale");
        }
        e => panic!("expected Unverified, got {e:?}"),
    }
    // a fresh report resumes fine.
    let mut i = base_input(&p, Some(&b));
    i.resume_report = Some(&report);
    assert!(matches!(attach(&i).unwrap(), AttachOutcome::Applied { .. }));

    // a resolved ref that doesn't name the policy
    let mut i = base_input(&p, Some(&b));
    i.policy = PolicySlot::ResolvedRef {
        reference: Ref::pinned("sha256:not-the-policy", "sha256:wrong"),
        policy: Box::new(p.clone()),
    };
    match attach(&i).unwrap_err() {
        AttachError::Unverified { reason, .. } => assert_eq!(reason, "policy_ref_mismatch"),
        e => panic!("expected Unverified, got {e:?}"),
    }
    // the matching ref attaches.
    let mut i = base_input(&p, Some(&b));
    i.policy = PolicySlot::ResolvedRef {
        reference: Ref::pinned(p.policy_id.clone(), p.version_id.clone()),
        policy: Box::new(p.clone()),
    };
    assert!(matches!(attach(&i).unwrap(), AttachOutcome::Applied { .. }));

    // A ref mismatch is an integrity failure — `warn_and_degrade` covers
    // unverified *enforcement*, never unaddressed bytes: refused outright.
    let i = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::ResolvedRef {
            reference: Ref::pinned("sha256:not-the-policy", "sha256:wrong"),
            policy: Box::new(p.clone()),
        },
        backend: Some(&b),
        mode: AttachMode::WarnAndDegrade,
        lab_run: false,
        resume_report: None,
        grants: &[],
    };
    match attach(&i).unwrap_err() {
        AttachError::Unverified { reason, .. } => {
            assert_eq!(reason, "policy_ref_mismatch");
        }
        e => panic!("expected Unverified under degrade, got {e:?}"),
    }

    // a relied-on group with `unknown` evidence — a resources bound the EP2
    // model cannot enforce (the `lowering_loss` lands AND the group is
    // relied on ⇒ fail-closed).
    let mut res = layer_at(AuthorityClass::Kernel);
    res.resources.cpu_ms = Some(100);
    res.compute_ids();
    let mut i = base_input(&p, Some(&b));
    i.policy = PolicySlot::Inline(Box::new(res));
    match attach(&i).unwrap_err() {
        AttachError::Unverified {
            field_group,
            reason,
            ..
        } => {
            assert_eq!(field_group, "resources");
            assert!(reason.starts_with("evidence_unknown"));
        }
        e => panic!("expected Unverified, got {e:?}"),
    }
}

#[test]
fn ac8_warn_and_degrade_and_the_lab_rule() {
    let p = principal_workspace();
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p.clone())),
        backend: None, // helper absent
        mode: AttachMode::WarnAndDegrade,
        lab_run: false,
        resume_report: None,
        grants: &[],
    };
    let out = attach(&input).unwrap();
    let AttachOutcome::Degraded {
        unverified_event,
        report,
        ..
    } = &out
    else {
        panic!("expected Degraded");
    };
    assert!(out.degraded(), "the containment_degraded stamp");
    assert!(report.is_none());
    assert_eq!(
        unverified_event.get("reason").and_then(Json::as_str),
        Some("helper_unavailable")
    );

    // A Lab run may not declare it — refused outright.
    let lab = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p.clone())),
        backend: None,
        mode: AttachMode::WarnAndDegrade,
        lab_run: true,
        resume_report: None,
        grants: &[],
    };
    assert!(matches!(
        attach(&lab).unwrap_err(),
        AttachError::DegradeDeclaredOnLab
    ));
    // A Lab run under fail-closed attaches normally.
    let lab_fc = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p)),
        backend: Some(&Ep2Model::reference()),
        mode: AttachMode::FailClosed,
        lab_run: true,
        resume_report: None,
        grants: &[],
    };
    assert!(matches!(
        attach(&lab_fc).unwrap(),
        AttachOutcome::Applied { .. }
    ));
}

#[test]
fn ac8_permission_unreachable_warns_at_attach() {
    // net_egress grant on a `none` policy — the seal-time warning, computed
    // at the Stage-1 seam where policy and grants meet.
    let p = kernel_default(0);
    let b = Ep2Model::reference();
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p)),
        backend: Some(&b),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[("perm-9".to_string(), grant(EffectDomain::NetEgress, "*"))],
    };
    let AttachOutcome::Applied { warnings, .. } = attach(&input).unwrap() else {
        panic!("expected Applied");
    };
    assert!(warnings
        .iter()
        .any(|w| matches!(w, SealWarning::PermissionUnreachable(u) if u.permission == "perm-9")));
}

// ── AC-R-2.8.4-9 — the meet property test ─────────────────────────────────────

/// A seeded xorshift64* — deterministic, no deps, hermetic.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 0
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u64) as usize]
    }
}

const AUTHORITIES: &[AuthorityClass] = &[
    AuthorityClass::Kernel,
    AuthorityClass::Definition,
    AuthorityClass::Principal,
    AuthorityClass::Delegate,
    AuthorityClass::Environment,
    AuthorityClass::External,
    AuthorityClass::Unverified,
];

/// Apply one random mutation to a layer (each is a narrowing or a loosening
/// attempt; whether it is *entitled* is the meet's business).
fn mutate(rng: &mut Rng, p: &mut ContainmentPolicy) {
    match rng.below(14) {
        // deny-kind unions — always narrowing.
        0 => p.fs.read.deny.push(format!("secret{}", rng.below(4))),
        1 => {
            p.fs.write
                .deny_within_allow
                .push(format!("tmp{}", rng.below(4)))
        }
        2 => {
            p.fs.protected_metadata
                .insert(format!("extra{}", rng.below(4)));
        }
        3 => p.proc.env.exclude.push(format!("VAR{}", rng.below(4))),
        // allow-kind touches — entitlement-dependent.
        4 => p.fs.write.allow.push(WritableRoot {
            root: format!("w{}", rng.below(4)),
            read_only_subpaths: vec![],
            protected_metadata_names: vec![],
        }),
        5 => {
            p.fs.exec =
                hh_containment::policy::ExecPolicy::Allow(vec![format!("bin{}", rng.below(3))])
        }
        6 => {
            p.net.mode = if rng.coin() {
                NetMode::Mediated
            } else {
                p.residual_channels.push(residual());
                NetMode::Public
            }
        }
        7 => {
            if p.net.mode != NetMode::None {
                p.net.rules.push(if rng.coin() {
                    deny_rule(&format!("deny{}.example", rng.below(4)))
                } else {
                    let mut r = allow_rule(&format!("ok{}.example", rng.below(4)));
                    if rng.coin() {
                        r.justification =
                            Some(Text::new("j", "t", ProvenanceRecord::kernel("t", 0)));
                    }
                    r
                });
            }
        }
        8 => p.proc.no_new_privs = rng.coin(),
        9 => p.amendment.strict = rng.coin(),
        10 => p.resources.cpu_ms = Some(rng.below(4) * 100),
        11 => p.net.local_binding = rng.coin(),
        // the exemption attempt — drop a KERNEL_PROTECTED name.
        12 => {
            let name = KERNEL_PROTECTED[rng.below(KERNEL_PROTECTED.len() as u64) as usize];
            p.fs.protected_metadata.remove(name);
        }
        _ => {
            p.fs.read.mode = if rng.coin() {
                p.fs.read.allow = vec![format!("docs{}", rng.below(3))];
                ReadMode::AllowOnly
            } else {
                p.fs.read.allow = vec![];
                ReadMode::AllowAllExcept
            }
        }
    }
}

/// The meet's unconditional invariants on `Ok` — AC-H4-09's property halves.
fn check_meet_invariants(layers: &[ContainmentPolicy], e: &ContainmentPolicy) {
    // (a) no KERNEL_PROTECTED name is ever removed.
    for name in KERNEL_PROTECTED {
        assert!(
            e.fs.protected_metadata.contains(*name),
            "KERNEL_PROTECTED {name} dropped"
        );
    }
    // (b) no deny is dropped — deny-kind sets union across layers.
    let d = |a: &[String]| a.iter().cloned().collect::<BTreeSet<_>>();
    let read_deny = d(&e.fs.read.deny);
    for l in layers {
        for x in &l.fs.read.deny {
            assert!(read_deny.contains(x), "read.deny {x} dropped");
        }
        for x in &l.fs.write.deny_within_allow {
            assert!(
                e.fs.write.deny_within_allow.contains(x),
                "deny_within_allow {x} dropped"
            );
        }
        for x in &l.proc.env.exclude {
            assert!(e.proc.env.exclude.contains(x), "env.exclude {x} dropped");
        }
        for r in l
            .net
            .rules
            .iter()
            .filter(|r| r.decision == RuleDecision::Deny)
        {
            assert!(
                e.net
                    .rules
                    .iter()
                    .any(|x| x.decision == RuleDecision::Deny && x.host == r.host),
                "net deny {} dropped",
                r.host.spelling()
            );
        }
        for r in &l.residual_channels {
            assert!(
                e.residual_channels
                    .iter()
                    .any(|x| x.kind == r.kind && x.closer == r.closer && x.owner == r.owner),
                "residual dropped"
            );
        }
    }
    // (c) no allow appears that no entitled layer declared — every result
    // member is in the kernel default's set or was declared by a layer
    // whose class held the field's entitlement.
    let default = kernel_default(0);
    let declared_by = |f: &dyn Fn(&ContainmentPolicy) -> bool, min: AuthorityClass| {
        layers.iter().any(|l| l.provenance.authority >= min && f(l))
    };
    for w in &e.fs.write.allow {
        assert!(
            declared_by(
                &|l: &ContainmentPolicy| l.fs.write.allow.iter().any(|x| x.root == w.root),
                AuthorityClass::Principal
            ) || default.fs.write.allow.iter().any(|x| x.root == w.root),
            "writable root {} never declared by an entitled layer",
            w.root
        );
    }
    if let hh_containment::policy::ExecPolicy::Allow(set) = &e.fs.exec {
        for x in set {
            assert!(
                declared_by(
                    &|l: &ContainmentPolicy| matches!(&l.fs.exec, hh_containment::policy::ExecPolicy::Allow(s) if s.contains(x)),
                    AuthorityClass::Kernel
                ),
                "exec allow {x} never declared by a kernel layer"
            );
        }
    }
    for r in e
        .net
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Allow)
    {
        // Per-class entitlement: `kernel`/`principal` free; `definition`
        // only when ITS declaration of the rule carried a justification.
        let declared = layers.iter().any(|l| {
            let mine = l
                .net
                .rules
                .iter()
                .find(|x| x.decision == RuleDecision::Allow && x.host == r.host);
            match (l.provenance.authority, mine) {
                (_, None) => false,
                (AuthorityClass::Kernel | AuthorityClass::Principal, Some(_)) => true,
                (AuthorityClass::Definition, Some(x)) => x.justification.is_some(),
                _ => false,
            }
        });
        assert!(declared, "allow rule {} undeclared", r.host.spelling());
    }
    // `read.allow` is special: the first layer switching to `allow_only`
    // installs its list wholesale — that install IS the narrowing (the
    // allow_only extent ⊆ the allow_all_except extent), so any authority
    // may declare it; subsequent sub-kernel layers only intersect. The
    // checkable half: no invented member.
    for x in &e.fs.read.allow {
        assert!(
            layers.iter().any(|l| l.fs.read.allow.contains(x)),
            "read.allow {x} never declared by any layer"
        );
    }
    // `net.mode` is always a declared value (a narrowing writes the layer's
    // own mode; a loosening requires ≥ principal — covered by the directed
    // tests).
    if e.net.mode != NetMode::None {
        assert!(
            layers.iter().any(|l| l.net.mode == e.net.mode),
            "net.mode {:?} never declared",
            e.net.mode
        );
    }
    // Resource bounds are never invented: a surviving bound was declared by
    // some layer (raises need `definition` — covered by the directed test).
    if let Some(v) = e.resources.cpu_ms {
        assert!(
            layers.iter().any(|l| l.resources.cpu_ms == Some(v)),
            "cpu_ms {v} never declared"
        );
    }
    // (d) the result validates and is kernel-provenanced.
    e.validate().unwrap();
    assert_eq!(e.provenance.authority, AuthorityClass::Kernel);
}

#[test]
fn ac9_meet_property_over_seeded_random_stacks() {
    let mut rng = Rng(0xC0FF_EE12);
    let mut oks = 0usize;
    let mut widenings = 0usize;
    let mut exemptions = 0usize;
    let mut invalid = 0usize;
    for _ in 0..4000 {
        let n = 1 + rng.below(4) as usize;
        let mut layers = Vec::with_capacity(n);
        for _ in 0..n {
            let a = *rng.pick(AUTHORITIES);
            let mut l = layer_at(a);
            for _ in 0..(1 + rng.below(3)) {
                mutate(&mut rng, &mut l);
            }
            l.compute_ids();
            layers.push(l);
        }
        match effective(&layers, 7) {
            Ok(e) => {
                oks += 1;
                check_meet_invariants(&layers, &e);
            }
            Err(MeetError::Widening(_)) => widenings += 1,
            Err(MeetError::ProtectedPathExemption { .. }) => exemptions += 1,
            Err(MeetError::InvalidLayer { .. } | MeetError::InvalidEffective(_)) => invalid += 1,
        }
    }
    // The corpus exercises both halves — a test that only ever errs proves
    // nothing about the Ok invariants.
    assert!(oks > 100, "too few Ok meets: {oks}");
    assert!(widenings > 50, "too few ContainmentWidening: {widenings}");
    assert!(exemptions > 0, "the exemption arm was never exercised");
    eprintln!("ac9: ok={oks} widening={widenings} exempt={exemptions} invalid={invalid}");
}

// ── AC-R-2.8.4-10 — lowering loss ─────────────────────────────────────────────

#[test]
fn ac10_lowering_loss_names_every_unenforceable_relied_field() {
    let mut p = layer_at(AuthorityClass::Definition);
    p.net.mode = NetMode::Mediated;
    p.resources.cpu_ms = Some(10);
    p.resources.network_calls = Some(3);
    p.net.tls.terminate = true;
    p.net.tls.inspect_hooks = vec!["hook-1".into()];
    p.net.upstream_proxy = Some("proxy-1".into());
    p.proc.syscall_filter = Some("filter-1".into());
    p.compute_ids();
    let b = Ep2Model::reference();
    let loss = b.lowering_loss(&p);
    let fields: BTreeSet<&str> = loss.iter().map(|l| l.field.as_str()).collect();
    for f in [
        "resources.cpu_ms",
        "resources.network_calls",
        "net.tls.terminate",
        "net.tls.inspect_hooks",
        "net.upstream_proxy",
        "proc.syscall_filter",
    ] {
        assert!(fields.contains(f), "missing loss for {f}: {fields:?}");
    }
    // The report lands the same list — and a relied-on loss fails the
    // attach fail-closed (AC-8/-10 joined).
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(p)),
        backend: Some(&b),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[],
    };
    assert!(matches!(
        attach(&input).unwrap_err(),
        AttachError::Unverified { .. }
    ));

    // An *unrelied* loss does not fail the attach — a `public`-mode policy
    // declaring a proxy lands the loss and applies (net is not relied on
    // under public; the proxy is a declared loss, never mediation).
    let mut pub_ = layer_at(AuthorityClass::Principal);
    pub_.net.mode = NetMode::Public;
    pub_.net.upstream_proxy = Some("proxy-1".into());
    pub_.residual_channels.push(residual());
    pub_.compute_ids();
    let input = AttachInput {
        env_handle: "env-1",
        policy: PolicySlot::Inline(Box::new(pub_)),
        backend: Some(&b),
        mode: AttachMode::FailClosed,
        lab_run: false,
        resume_report: None,
        grants: &[],
    };
    let AttachOutcome::Applied { report, .. } = attach(&input).unwrap() else {
        panic!("expected Applied");
    };
    assert!(report
        .lowering_loss
        .iter()
        .any(|l| l.field == "net.upstream_proxy" && l.reason == "proxy_env_is_not_mediation"));
}

// ── AC-R-2.8.4-14 — the authorize precondition (the monitor seam) ────────────

/// A minimal `Proposal` — deliberately malformed (unresolvable capability,
/// no args provenance) so only the *precondition* can decide it.
fn bare_proposal(domain: EffectDomain, gate: ContainmentGate) -> Proposal {
    Proposal {
        effect_id: "e1".into(),
        attempt_no: 1,
        proposer: "test:agent".into(),
        capability_ref: hh_compiler::plan::PinnedRef {
            semantic_id: "nonexistent".into(),
            version_id: "v".into(),
        },
        effect: EffectClass::domain_only(domain),
        surface_args: Json::obj([]),
        args_provenance: None,
        context_label: Label::at(AuthorityClass::Principal),
        self_report: None,
        inputs: AssessmentInputs::default(),
        requested_grants: vec![],
        containment: gate,
        at: 0,
    }
}

#[test]
fn ac14_unverified_precondition_runs_before_any_check() {
    let m = Monitor::new(
        HandleTable::default(),
        default_table("pol-1", Mode::Attended),
    );
    // floor_gate over a domain whose required group's evidence is unknown.
    let p = kernel_default(0);
    let mut report = attach_ok(&p);
    report
        .enforcement_evidence
        .insert(FieldGroup::Net, EnforcementEvidence::Unknown);
    let gate = floor_gate(&p, Some(&report), &input(EffectDomain::NetEgress, &[], &[]));
    let ContainmentGate::Unverified { group } = &gate else {
        panic!("expected Unverified, got {gate:?}")
    };
    assert_eq!(group, "net");
    let d = m
        .authorize(&bare_proposal(EffectDomain::NetEgress, gate))
        .unwrap();
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::ContainmentUnverified,
            ..
        }
    ));
    // The trail's first record is the precondition — before well-formedness.
    assert_eq!(d.checks[0].step, 0);
    assert_eq!(d.checks[0].outcome, "fail");
    assert!(d.checks[0].detail.starts_with("containment:unverified"));

    // No report at all (helper absent) → unverified for every domain.
    let g2 = floor_gate(&p, None, &input(EffectDomain::FsRead, &["x"], &[]));
    assert!(matches!(g2, ContainmentGate::Unverified { .. }));
    // A stale report likewise.
    let mut stale = attach_ok(&p);
    stale.policy_version_id = "sha256:other".into();
    let g3 = floor_gate(&p, Some(&stale), &input(EffectDomain::FsRead, &["x"], &[]));
    assert!(matches!(
        g3,
        ContainmentGate::Unverified { ref group } if group == "report"
    ));
}

#[test]
fn ac14_floor_denial_never_consults_pi_or_asks() {
    let m = Monitor::new(
        HandleTable::default(),
        default_table("pol-1", Mode::Attended),
    );
    let p = kernel_default(0); // net.mode = none
    let report = attach_ok(&p);
    // A `net_egress` proposal — `admits` refuses `mode_none`; Π would `ask`
    // on `net_egress` under no taint? — the point is the gate denies first,
    // never an Ask shape.
    let gate = floor_gate(
        &p,
        Some(&report),
        &input(EffectDomain::NetEgress, &[], &["x.example"]),
    );
    let ContainmentGate::Denied { detail } = &gate else {
        panic!("expected Denied, got {gate:?}")
    };
    assert_eq!(detail, "refused:mode_none");
    let d = m
        .authorize(&bare_proposal(EffectDomain::NetEgress, gate))
        .unwrap();
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::Containment,
            ..
        }
    ));
    assert!(!matches!(d.decision, Decision::Ask { .. }));
    assert_eq!(d.checks[0].detail, "refused:mode_none");

    // `amendable` denies the same way at Stage 1, recorded distinctly.
    let gate = floor_gate(
        &p,
        Some(&report),
        &input(EffectDomain::FsWrite, &["w/x"], &[]),
    );
    let ContainmentGate::Denied { detail } = &gate else {
        panic!("expected Denied, got {gate:?}")
    };
    assert_eq!(detail, "amendable:add_writable_root");
    let d = m
        .authorize(&bare_proposal(EffectDomain::FsWrite, gate))
        .unwrap();
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::Containment,
            ..
        }
    ));

    // `Clear` proceeds past the precondition (the malformed proposal then
    // fails step 0 — proving the gate let it through to the checks).
    let gate = floor_gate(
        &p,
        Some(&report),
        &input(EffectDomain::FsRead, &["docs/a"], &[]),
    );
    assert_eq!(gate, ContainmentGate::Clear);
    let d = m
        .authorize(&bare_proposal(EffectDomain::FsRead, gate))
        .unwrap();
    assert!(matches!(
        d.decision,
        Decision::Deny {
            reason: DenyReason::MissingProvenance,
            ..
        }
    ));
}

#[test]
fn ac14_admit_input_and_scope_feed() {
    // `admit_input` lifts the scope-bound canonical args (fs_path/host).
    let bindings = ScopeBindings::Bindings(Json::Arr(vec![
        Json::obj([
            ("param_path", Json::str("p")),
            ("scope_kind", Json::str("fs_path")),
        ]),
        Json::obj([
            ("param_path", Json::str("h")),
            ("scope_kind", Json::str("host")),
        ]),
    ]));
    let canonical = CanonicalArgs {
        params: BTreeMap::new(),
        scoped: BTreeMap::from([
            ("p".to_string(), Json::str("workspace/a")),
            ("h".to_string(), Json::str("x.example")),
        ]),
        scope_unknown: false,
    };
    let i = admit_input(EffectDomain::FsWrite, &bindings, &canonical);
    assert_eq!(i.fs_paths, vec!["workspace/a".to_string()]);
    assert_eq!(i.hosts, vec!["x.example".to_string()]);

    // I-C3 — workspace_local only inside the roots with net none.
    let p = principal_workspace();
    assert!(workspace_scope(&p, &["workspace/a".into()], true).is_yes());
    // `mediated` + not read_only ⇒ external.
    assert!(!workspace_scope(&p, &["workspace/a".into()], false).is_yes());
    // outside the roots ⇒ never workspace_local.
    assert!(!workspace_scope(&p, &["outside/a".into()], true).is_yes());
    // net none + inside ⇒ local.
    let mut n = p.clone();
    n.net.mode = NetMode::None;
    assert!(workspace_scope(&n, &["workspace/a".into()], false).is_yes());
}

// ── misc unit coverage ────────────────────────────────────────────────────────

#[test]
fn verify_report_binds_report_to_policy_version() {
    let p = kernel_default(0);
    let r = attach_ok(&p);
    assert!(verify_report(&r, &p.version_id).is_ok());
    assert!(verify_report(&r, "sha256:other").is_err());
}

#[test]
fn paths_normalize_within_and_suffix_match() {
    assert_eq!(paths::normalize_path("./a//b/"), "a/b");
    assert!(paths::within("root", "root/a/b"));
    assert!(!paths::within("root", "rooted/a"));
    assert!(paths::pattern_matches(".env", "w/x/.env"));
    assert!(!paths::pattern_matches("/abs", "w/abs"));
    // `..` is a component, never rewritten — a spelled escape is denied by
    // `within`, not hidden.
    assert!(!paths::within("root", "root/../x"));
}

#[test]
fn events_payload_shapes_are_content_free() {
    let p = principal_workspace();
    let r = attach_ok(&p);
    let ev = events::applied_payload("env-1", &p, &r);
    // every member is a tag/id/ref — audit-grade.
    let Json::Obj(m) = &ev else { panic!() };
    assert!(m.contains_key("enforcement_evidence"));
    let un = events::unverified_payload("fs", "evidence_unknown:fs");
    assert_eq!(un.get("field_group").and_then(Json::as_str), Some("fs"));
    assert_eq!(
        un.get("reason").and_then(Json::as_str),
        Some("evidence_unknown:fs")
    );
}

#[test]
fn relied_groups_cover_the_declared_boundary() {
    let p = kernel_default(0);
    let g = relied_groups(&p);
    assert!(g.contains(&FieldGroup::Fs));
    assert!(g.contains(&FieldGroup::Proc));
    assert!(g.contains(&FieldGroup::Net)); // none mode relies on the net boundary
    assert!(!g.contains(&FieldGroup::Resources));
    let mut pub_ = layer_at(AuthorityClass::Principal);
    pub_.net.mode = NetMode::Public;
    pub_.residual_channels.push(residual());
    pub_.resources.cpu_ms = Some(1);
    let g = relied_groups(&pub_);
    assert!(!g.contains(&FieldGroup::Net));
    assert!(g.contains(&FieldGroup::Resources));
}
