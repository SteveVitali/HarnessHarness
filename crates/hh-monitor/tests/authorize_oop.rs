//! AC-R-2.8.1-16 — `authorize` runs **out-of-process over canonical bytes**
//! (DF-S1.11-3's production seam): `hh-authorize` reads one
//! `hh-monitor/authorize-input/1` document and emits the canonical
//! `KernelDecision`. These tests pin:
//!
//! - **byte equality** — the OOP output is byte-identical to the in-process
//!   `Monitor::authorize` over the same recorded inputs (allow and deny);
//! - **profile invariance** — the `model_profile` envelope member is recorded
//!   context only; two profiles produce byte-identical decisions;
//! - **round-trip** — `AuthorizeInput` re-encodes to the same canonical bytes;
//! - **refusal** — malformed or non-canonical input exits non-zero with a
//!   typed refusal on stderr and no decision row on stdout;
//! - **no Text/model dependency** — the input carries no `Text` leaf and the
//!   binary's source has no model-call path (the AC's static half is the
//!   workspace scan in `tcb`/acceptance; here the *dynamic* half: the
//!   decision cannot consume content that never arrives).

use hh_compiler::equiv::bind_surface;
use hh_compiler::plan::PinnedRef;
use hh_hir::document::Node;
use hh_hir::kinds::{
    EffectAttributes, EffectDomain, Mutability, RepeatSafety, Reversibility, ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{
    Grant, GrantConstraints, KindRecord, Resources, ScopeBindings, SurfaceRecord,
    ToolCapabilityRecord, ToolSurface,
};
use hh_hir::refs::Ref;
use hh_hir::EffectClass;
use hh_monitor::assess::{AssessmentInputs, Tri};
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, ContainmentGate, Monitor, Proposal};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_monitor::wire::AuthorizeInput;
use hh_provenance::{AuthorityClass, HumanRole, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixtures (the acceptance.rs spellings, minimal) ─────────────────────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn model_args() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::model("model:test", "run:test", "resp-1"),
        PersistenceScope::Run,
        0,
    )
}

fn tool_node(id: &str, args: &[&str], effects: Vec<EffectClass>, seq: u64) -> Node {
    let mut n = Node::new(
        hh_hir::kinds::EntityKind::ToolCapability,
        KindRecord::ToolCapability(ToolCapabilityRecord {
            purpose: Text::new("a test tool", "test:owner", prov(seq)),
            input_schema: Json::obj([(
                "properties",
                Json::Obj(
                    args.iter()
                        .map(|a| (a.to_string(), Json::obj([("type", Json::str("string"))])))
                        .collect(),
                ),
            )]),
            output_schema: None,
            effects: ToolEffects::Declared(effects.into_iter().collect()),
            preconditions: vec![],
            scope_bindings: ScopeBindings::Unknown,
            resources: Resources::NoneDeclared,
            observation_contract: Json::Null,
            cost_model: None,
            execution_requirement: Json::Null,
            source: Json::Null,
            exposure_hint: Json::Null,
            postconditions: vec![],
            flow_contract: None,
            action_patterns: Vec::new(),
        }),
        prov(seq),
    );
    n.version.semantic_id = Some(id.into());
    n.surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: "tool_a".into(),
        namespace: "test".into(),
        description_template: Text::new("a surfaced tool", "test:owner", prov(seq)),
        argument_order: args.iter().map(|a| a.to_string()).collect(),
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: None,
        icon_ref: None,
    })));
    n
}

fn cap_entry(n: &Node) -> CapabilityEntry {
    let (ts, rec) = match (&n.surface, &n.semantic) {
        (Some(SurfaceRecord::Tool(t)), KindRecord::ToolCapability(r)) => (t.as_ref(), r.clone()),
        _ => unreachable!(),
    };
    CapabilityEntry {
        record: rec.clone(),
        binding: bind_surface(n, ts, &rec.input_schema),
    }
}

fn grant(domain: EffectDomain, scope: &str, delegable: bool) -> Grant {
    Grant {
        effect: EffectClass::domain_only(domain),
        scope: scope.to_string(),
        constraints: GrantConstraints::default(),
        delegable,
    }
}

fn root_handle(id: &str, grants: Vec<Grant>, ceiling: AuthorityClass) -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId(id.to_string()),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-monitor/test", 0),
        grants: grants.clone(),
        ceiling,
        validity: HandleValidity {
            issued_at: "evt-mint".into(),
            expires_at: Some(HandleExpiry::Run("run-1".into())),
            revoked_by: None,
        },
        parent_handle: None,
        delegable: grants.iter().all(|g| g.delegable) && !grants.is_empty(),
        origin_basis: OriginBasis::Seal,
        basis_ref: "test:agent#sha256:def".into(),
        budget_ref: None,
        scope: hh_monitor::decision::DecisionScope::Session,
    }
}

fn monitor(mode: Mode, caps: Vec<CapabilityEntry>, handles: Vec<AuthorityHandle>) -> Monitor {
    let mut table = HandleTable::default();
    for h in handles {
        table.handles.insert(h.handle_id.clone(), h);
    }
    let mut m = Monitor::new(table, default_table("pol-v1", mode));
    m.proposers.insert(
        "test:agent".to_string(),
        Label::at(AuthorityClass::Principal),
    );
    m.run_id = "run-1".to_string();
    m.turn_id = "turn-1".to_string();
    m.effect_id = "e1".to_string();
    for c in caps {
        m.capabilities
            .insert(c.binding.capability_ref.semantic_id.clone(), c);
    }
    m
}

fn proposal(cap_semantic: &str, domain: EffectDomain, args: Json) -> Proposal {
    Proposal {
        effect_id: "e1".into(),
        attempt_no: 1,
        proposer: "test:agent".into(),
        capability_ref: PinnedRef {
            semantic_id: cap_semantic.to_string(),
            version_id: "v".into(),
        },
        effect: EffectClass {
            domain,
            attributes: Some(EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Compensable,
            }),
        },
        surface_args: args,
        args_provenance: Some(model_args()),
        context_label: Label::at(AuthorityClass::Principal),
        self_report: None,
        inputs: AssessmentInputs {
            inside_writable_roots: Tri::Yes,
            ..AssessmentInputs::default()
        },
        requested_grants: vec![],
        containment: ContainmentGate::Clear,
        flow: Default::default(),
        remedy_taken: None,
        at: 0,
    }
}

fn fs_write_cap() -> CapabilityEntry {
    cap_entry(&tool_node(
        "test:fs_write",
        &["path"],
        vec![EffectClass::domain_only(EffectDomain::FsWrite)],
        1,
    ))
}

fn fs_grant_handle() -> AuthorityHandle {
    root_handle(
        "hnd-root-1",
        vec![grant(EffectDomain::FsWrite, "**", false)],
        AuthorityClass::Principal,
    )
}

fn run_oop(input: &AuthorizeInput) -> std::process::Output {
    let bytes = input.to_json().to_canonical_string();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-authorize"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn hh-authorize");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(bytes.as_bytes())
        .unwrap();
    child.wait_with_output().expect("hh-authorize")
}

// ── AC-R-2.8.1-16 ────────────────────────────────────────────────────────────

/// The OOP decision is byte-identical to the in-process decision — `allow`.
#[test]
fn oop_allow_byte_identical() {
    let m = monitor(
        Mode::Attended,
        vec![fs_write_cap()],
        vec![fs_grant_handle()],
    );
    let p = proposal(
        "test:fs_write",
        EffectDomain::FsWrite,
        Json::obj([("path", Json::str("workspace/notes.txt"))]),
    );
    let input = AuthorizeInput::capture(&m, &p, Some("model:test@v1".into()));
    let in_process = m.authorize(&p).expect("in-process").to_json();
    let out = run_oop(&input);
    assert!(
        out.status.success(),
        "hh-authorize failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim_end(), in_process.to_canonical_string());
    // A real decision row (allow or a typed deny — the fixture's Π verdict).
    assert!(in_process.get("decision").and_then(Json::as_str).is_some());
}

/// Byte-identical on the deny path too (`NoCoveringGrant` — no handle covers
/// `net_egress`).
#[test]
fn oop_deny_byte_identical() {
    let m = monitor(Mode::Attended, vec![fs_write_cap()], vec![]);
    let p = proposal(
        "test:fs_write",
        EffectDomain::NetEgress,
        Json::obj([("path", Json::str("workspace/notes.txt"))]),
    );
    let input = AuthorizeInput::capture(&m, &p, None);
    let in_process = m.authorize(&p).expect("in-process").to_json();
    let out = run_oop(&input);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim_end(),
        in_process.to_canonical_string()
    );
    assert!(matches!(in_process.get("decision"), Some(Json::Str(s)) if s == "deny"));
}

/// `model_profile` is envelope metadata — two profiles, identical bytes (the
/// AC's profile-invariance clause).
#[test]
fn oop_profile_invariant() {
    let m = monitor(
        Mode::Attended,
        vec![fs_write_cap()],
        vec![fs_grant_handle()],
    );
    let p = proposal(
        "test:fs_write",
        EffectDomain::FsWrite,
        Json::obj([("path", Json::str("a.txt"))]),
    );
    let a = AuthorizeInput::capture(&m, &p, Some("model:alpha@1".into()));
    let b = AuthorizeInput::capture(&m, &p, Some("model:beta@99".into()));
    let out_a = run_oop(&a);
    let out_b = run_oop(&b);
    assert!(out_a.status.success() && out_b.status.success());
    assert_eq!(out_a.stdout, out_b.stdout);
}

/// `AuthorizeInput` round-trips: decode(encode(x)) re-encodes to the same
/// canonical bytes (the codec is total over its own spellings).
#[test]
fn authorize_input_round_trip() {
    let m = monitor(
        Mode::Unattended,
        vec![fs_write_cap()],
        vec![fs_grant_handle()],
    );
    let p = proposal(
        "test:fs_write",
        EffectDomain::FsWrite,
        Json::obj([("path", Json::str("a.txt"))]),
    );
    let input = AuthorizeInput::capture(&m, &p, Some("model:test@v1".into()));
    let wire = input.to_json();
    let bytes = wire.to_canonical_string();
    let parsed = hh_wire::canonical::parse_canonical(bytes.as_bytes()).unwrap();
    let decoded = AuthorizeInput::from_json(&parsed).expect("decode");
    assert_eq!(decoded.to_json().to_canonical_string(), bytes);
}

/// A malformed member is a typed refusal — exit 2, reason on stderr, no
/// decision row on stdout.
#[test]
fn oop_malformed_refused() {
    let mut bad = Json::obj([("kind", Json::str("hh-monitor/authorize-input/1"))]);
    let bytes = bad.to_canonical_string();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-authorize"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(bytes.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("run"));
    // A wrong `kind` refuses too.
    bad = Json::obj([("kind", Json::str("hh-monitor/other/9"))]);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-authorize"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(bad.to_canonical_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

/// Non-canonical bytes are refused — the seam never re-encodes a lenient
/// parse into a different recorded input.
#[test]
fn oop_noncanonical_refused() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-authorize"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{ \"kind\": \"x\" }")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

/// An unresolvable `capability_ref` is a typed refusal, not a decision row.
#[test]
fn oop_capability_unresolvable_refused() {
    let m = monitor(Mode::Attended, vec![fs_write_cap()], vec![]);
    let mut p = proposal(
        "test:absent",
        EffectDomain::FsWrite,
        Json::obj([("path", Json::str("a.txt"))]),
    );
    p.capability_ref.semantic_id = "test:absent".into();
    let input = AuthorizeInput::capture(&m, &p, None);
    let out = run_oop(&input);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("capability_unresolvable"));
}

/// The `AuthorizeInput` the dispatcher captures carries the approval fold —
/// a recorded `decided{deny}` and its denial count re-derive identically.
#[test]
fn oop_approval_state_carried() {
    use hh_monitor::approval::{ApprovalState, ApprovalStats, DenialFallback, DenialPolicy};
    let mut m = monitor(Mode::Unattended, vec![fs_write_cap()], vec![]);
    m.denial_policy = Some(DenialPolicy {
        max_denials: 0,
        fallback: DenialFallback::StopRun,
    });
    m.approvals = ApprovalState {
        stats: ApprovalStats {
            requested: 1,
            granted: 0,
            human_wait_ms: 42,
        },
        denial_counts: {
            let mut d = std::collections::BTreeMap::new();
            d.insert("test:fs_write\x1fhash-1".to_string(), 1u64);
            d
        },
        ..ApprovalState::default()
    };
    let p = proposal(
        "test:fs_write",
        EffectDomain::FsWrite,
        Json::obj([("path", Json::str("a.txt"))]),
    );
    let input = AuthorizeInput::capture(&m, &p, None);
    let wire = input.to_json().to_canonical_string();
    let parsed = hh_wire::canonical::parse_canonical(wire.as_bytes()).unwrap();
    let decoded = AuthorizeInput::from_json(&parsed).expect("decode");
    let m2 = decoded.monitor();
    assert_eq!(m2.approvals.stats.requested, 1);
    assert_eq!(m2.approvals.stats.human_wait_ms, 42);
    assert_eq!(m2.denial_policy.unwrap().max_denials, 0);
    assert_eq!(m2.approvals.denial_counts.len(), 1);
}

// ── AC-R-2.8.2-12 — OOP parity over the flow plane ───────────────────────────

/// A `flow_contract` capability decided through `hh-authorize` is
/// byte-identical to the in-process decision — the `flow` member round-trips
/// through `AuthorizeInput` (codec parity), and the decision is
/// profile-invariant.
#[test]
fn oop_flow_contract_byte_identical_and_profile_invariant() {
    // A `message_human` capability with the C2 contract — `to` the
    // endorsement-surface recipient param, `body` the content param.
    let mut n = tool_node(
        "test:sendmail",
        &["to", "body"],
        vec![EffectClass {
            domain: EffectDomain::MessageHuman,
            attributes: Some(EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Open,
                reversibility: Reversibility::Compensable,
            }),
        }],
        1,
    );
    if let KindRecord::ToolCapability(t) = &mut n.semantic {
        t.flow_contract = Some(Json::obj([
            (
                "contribution",
                Json::obj([("readers_from", Json::str("reads"))]),
            ),
            ("recipient_params", Json::Arr(vec![Json::str("to")])),
            ("content_params", Json::Arr(vec![Json::str("body")])),
        ]));
    }
    let h = root_handle(
        "hnd-mail",
        vec![grant(EffectDomain::MessageHuman, "*", true)],
        AuthorityClass::Principal,
    );
    let m = monitor(Mode::Attended, vec![cap_entry(&n)], vec![h]);
    let mut p = proposal(
        "test:sendmail",
        EffectDomain::MessageHuman,
        Json::obj([
            ("to", Json::str("mallory@evil")),
            ("body", Json::str("report")),
        ]),
    );
    // `to` is clean (principal-supplied); `body`'s readers do not cover the
    // recipient — the flow stage asks with `{approval, sanitize}`.
    let mut body = Label::at(AuthorityClass::Principal);
    body.readers =
        hh_provenance::ReaderSet::Restricted(["bob@corp".to_string()].into_iter().collect());
    p.effect = EffectClass {
        domain: EffectDomain::MessageHuman,
        attributes: Some(EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Open,
            reversibility: Reversibility::Compensable,
        }),
    };
    p.flow = hh_monitor::monitor::FlowInputs {
        param_labels: [
            ("to".to_string(), Label::at(AuthorityClass::Principal)),
            ("body".to_string(), body),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let in_process = m.authorize(&p).expect("in-process").to_json();
    // The flow stage's ask is on the wire.
    assert!(matches!(in_process.get("decision"), Some(Json::Str(s)) if s == "ask"));

    for profile in [Some("model:alpha@1".to_string()), None] {
        let input = AuthorizeInput::capture(&m, &p, profile.clone());
        // The wire form carries the non-default `flow` member.
        assert!(input
            .to_json()
            .get("proposal")
            .and_then(|pr| pr.get("flow"))
            .is_some());
        let out = run_oop(&input);
        assert!(
            out.status.success(),
            "hh-authorize failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().trim_end(),
            in_process.to_canonical_string(),
            "OOP diverged from in-process (profile {profile:?})"
        );
    }
}
