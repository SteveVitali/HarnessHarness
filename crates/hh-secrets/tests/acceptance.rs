//! S1.13 acceptance coverage — every test names the AC it pins
//! (AC-R-2.8.3-N, §5g.3 §8/§9; ADR-0057/0058/0059).
//!
//! Each test fails if the behaviour it covers is removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_hir::diff::{
    AuthorityDelta, Delta, DiffClassification, DiffDerivation, DiffOp, DiffOpTag, HirDiff,
};
use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
use hh_hir::errors::HirError;
use hh_hir::kinds::EntityKind;
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, KindRecord};
use hh_hir::refs::Ref;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

/// A kernel-authored event for a run (the shape `Store::append` expects).
fn k_ev(store: &Store, run_id: &str, id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// Record a `security.permission.granted` handle covering `secret_access` on
/// `secret:<channel>` for `holder` — the monitor's durable row the broker's
/// PDP gate reads (test-authored in the monitor's own payload shape).
fn grant_handle(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    holder: &str,
    channel_id: &str,
    handle_id: &str,
) {
    let issuer = ProvenanceRecord::kernel("kernel:test", store.now_ms());
    let payload = Json::obj([
        ("handle_id", Json::str(handle_id)),
        (
            "permission_ref",
            Json::obj([
                ("semantic_id", Json::str("perm.secrets")),
                (
                    "version_id",
                    Json::str("sha256:".to_string() + &"0".repeat(64)),
                ),
            ]),
        ),
        ("holder", Json::str(holder)),
        ("issuer", issuer.to_json()),
        (
            "grants",
            Json::Arr(vec![Json::obj([
                (
                    "effect",
                    Json::obj([("domain", Json::str("secret_access"))]),
                ),
                ("scope", Json::str(SecretChannel::grant_scope(channel_id))),
                ("constraints", Json::obj([])),
                ("delegable", Json::Bool(false)),
            ])]),
        ),
        ("ceiling", Json::str("principal")),
        (
            "validity",
            Json::obj([
                ("issued_at", Json::str(store.ts_now())),
                ("expires_at", Json::Null),
            ]),
        ),
        ("parent_handle", Json::Null),
        ("delegable", Json::Bool(false)),
        ("origin_basis", Json::str("approval")),
        ("basis_ref", Json::str("perm.secrets")),
        ("budget_ref", Json::Null),
        ("authority_delta", Json::str("none")),
    ]);
    let ev = k_ev(
        store,
        run_id,
        handle_id,
        "security.permission.granted",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
}

/// Record the `security.permission.decided{decision = allow}` row relying on
/// `handle_id` for `effect_id` — returns the decided event id (the
/// `monitor_decision_ref` `bind` takes).
fn decide_allow(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    effect_id: &str,
    handle_id: &str,
) -> String {
    let decided_id = store.alloc_id("evt");
    let payload = Json::obj([
        ("effect_id", Json::str(effect_id)),
        ("decision", Json::str("allow")),
        ("effective_authority", Json::str("principal")),
        (
            "effective_risk_class",
            Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("ephemeral")),
            ]),
        ),
        ("handle_ids", Json::Arr(vec![Json::str(handle_id)])),
        ("policy_ref", Json::str("pi/1")),
        ("decider", Json::str("policy")),
        ("attempt_no", Json::Int(1)),
        ("proposal", Json::str("prop-1")),
        ("taint", Json::Arr(vec![])),
    ]);
    let ev = k_ev(
        store,
        run_id,
        &decided_id,
        "security.permission.decided",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
    decided_id
}

fn github_spec() -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: SecretSource::OperatorVault {
            vault_ref: "vault:github".into(),
        },
        destinations: vec![
            DestinationBinding {
                scheme: "https".into(),
                host_pattern: "api.github.com".into(),
                port: None,
                path_prefix: None,
                auth_carrier: AuthCarrier::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into()),
                },
            },
            DestinationBinding {
                scheme: "https".into(),
                host_pattern: "api.github.com/*".into(),
                port: None,
                path_prefix: None,
                auth_carrier: AuthCarrier::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into()),
                },
            },
        ],
        allowed_env_names: Some(["GH_API_TOKEN".into()].into_iter().collect()),
        delivery_modes: [
            hh_monitor::assess::SecretTransport::ProxyInjected,
            hh_monitor::assess::SecretTransport::WrappedLongLived,
        ]
        .into_iter()
        .collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "GitHub API token".into(),
    }
}

fn broker_with(github_value: &str) -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::OperatorVault {
            vault_ref: "vault:github".into(),
        },
        github_value,
    );
    CredentialBroker::new(Box::new(vault), "test-fp-key")
}

fn registered(broker: &mut CredentialBroker) -> SecretChannel {
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.3-1 — LT-01: placeholders only; leak_scan(run) = ∅; SecretRef
// renders name + description.
// ─────────────────────────────────────────────────────────────────────────────

/// **AC-R-2.8.3-1.** An environment dump and the `/proc`-equivalent read inside
/// the environment handle show placeholders only; `leak_scan(run) = ∅`;
/// `SecretRef` renders name + description.
#[test]
fn ac_r_2_8_3_1_environment_and_views_show_placeholders_only() {
    let (mut store, run_id, lease) = open("ac1");
    let secret = "ghp_live0123456789abcdefghij0123456789";
    let mut broker = broker_with(secret);
    registered(&mut broker);

    // Grant + decide through the monitor's rows, then bind.
    grant_handle(
        &mut store,
        &run_id,
        &lease,
        "agent.main",
        "github",
        "hnd-gh",
    );
    let decision_ref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-gh");
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            BindRequest {
                channel_id: "github".into(),
                holder: "agent.main".into(),
                env_handle_ref: "env-1".into(),
                env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                mode: hh_monitor::assess::SecretTransport::ProxyInjected,
                monitor_decision_ref: decision_ref,
                destinations: ["api.github.com".into()].into_iter().collect(),
            },
        )
        .unwrap();

    // The projected env: base env carries an inheritable var + a
    // non-inheritable one + a non-allowlisted one; the binding projects a
    // placeholder.
    let base_env: BTreeMap<String, String> = [
        ("PATH".into(), "/usr/bin".into()),
        (
            "AWS_SECRET_ACCESS_KEY".into(),
            "AKIAIOSFODNN7EXAMPLE".into(),
        ),
        ("RANDOM_NOT_ALLOWED".into(), "x".into()),
        ("HOME".into(), "/home/u".into()),
    ]
    .into_iter()
    .collect();
    let spec = broker.env_spec_for(
        "env-1",
        ["PATH".into(), "HOME".into(), "GH_API_TOKEN".into()]
            .into_iter()
            .collect(),
    );
    let mut bound_placeholders = BTreeMap::new();
    bound_placeholders.insert(
        "GH_API_TOKEN".to_string(),
        broker.placeholder_for(&binding.binding_id).unwrap().clone(),
    );
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    let projected = env_apply(&spec, &base_env, &bound_placeholders, &detectors).unwrap();

    // Deny-by-default: PATH/HOME carried; GH_API_TOKEN is a placeholder;
    // AWS_SECRET_ACCESS_KEY + RANDOM_NOT_ALLOWED absent.
    assert_eq!(projected.vars.get("PATH").unwrap(), "/usr/bin");
    let placeholder = projected.vars.get("GH_API_TOKEN").unwrap();
    assert!(Placeholder::is_placeholder(placeholder));
    assert!(placeholder.len() >= 16);
    assert!(!projected.vars.contains_key("AWS_SECRET_ACCESS_KEY"));
    assert!(!projected.vars.contains_key("RANDOM_NOT_ALLOWED"));

    // The dump + proc-env read + snapshot carry placeholders only — the value
    // appears nowhere (LT-01).
    let dump = projected.dump();
    let proc_env = projected.proc_env();
    let snapshot = projected.snapshot().to_canonical_string();
    assert!(!dump.contains(secret));
    assert!(!String::from_utf8_lossy(&proc_env).contains(secret));
    assert!(!snapshot.contains(secret));
    assert!(dump.contains(placeholder.as_str()));

    // leak_scan over every Stage-1 surface: ∅.
    let mut items: Vec<(ScanTarget, &str)> = vec![
        (ScanTarget::SinkDelivery("model".into()), dump.as_str()),
        (ScanTarget::SinkDelivery("env".into()), snapshot.as_str()),
    ];
    let proc_text = String::from_utf8_lossy(&proc_env).to_string();
    items.push((ScanTarget::RequestView, proc_text.as_str()));
    assert!(leak_scan(&items, &detectors).is_empty());
    // …and over the run's committed event payloads.
    assert!(broker
        .leak_scan_run(&store, &run_id, &detectors)
        .unwrap()
        .is_empty());

    // SecretRef renders name + description — never the value.
    let r = broker.channel("github").unwrap().secret_ref();
    assert_eq!(r.render(), "github (GitHub API token)");
    assert!(!r.render().contains(secret));

    // env_sweep — the SV-4 verifier — is clean.
    assert!(env_sweep(&projected, &detectors).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.3-8 — LT-08: literal credential fails seal; HirDiff rejected.
// ─────────────────────────────────────────────────────────────────────────────

fn doc_with_goal_text(content: &str) -> HirDocument {
    let prov = ProvenanceRecord::kernel("kernel:test", 1_000);
    let goal = Node::new(
        EntityKind::Goal,
        KindRecord::Goal(GoalRecord {
            statement: Text::new(content, "agent.main", prov.clone()),
            success_criteria: Vec::new(),
            unverifiable_reason: None,
            budget: Ref::pinned("budget.root", "sha256:".to_string() + &"0".repeat(64)),
            origin: GoalOrigin::Human,
            parent: None,
        }),
        prov,
    );
    let mut doc = HirDocument::new(Ref::pinned(
        "proc.main",
        "sha256:".to_string() + &"1".repeat(64),
    ));
    doc.nodes.push(goal);
    doc
}

/// **AC-R-2.8.3-8.** A definition containing a literal credential fails `seal`
/// with `SecretValueInDefinition`; a `HirDiff` introducing one is rejected in an
/// evolution context.
#[test]
fn ac_r_2_8_3_8_literal_credential_fails_seal_and_diff() {
    let secret = "sk-live-0123456789abcdefghijklmnop";
    let detectors = DetectorSet::standard(MaskSet::default());

    // A Text leaf carrying a literal credential → SecretValueInDefinition.
    let bad = doc_with_goal_text(&format!("deploy with {secret}"));
    let errs = seal_checked(&bad, 1_000, &detectors).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HirError::SecretValueInDefinition { .. })));

    // The $secret: marker is the *legal* reference form — not a credential.
    let ok = doc_with_goal_text("deploy with $secret:github");
    assert!(scan_document(&ok, &detectors).is_empty());

    // A HirDiff whose op introduces a literal credential is rejected.
    let prov = ProvenanceRecord::kernel("kernel:test", 1_000);
    let diff = HirDiff {
        base: DefinitionVersionRef {
            semantic_id: "def".into(),
            version_id: "sha256:".to_string() + &"a".repeat(64),
        },
        target: DefinitionVersionRef {
            semantic_id: "def".into(),
            version_id: "sha256:".to_string() + &"b".repeat(64),
        },
        dialect: "HIR/1".into(),
        ops: vec![DiffOp::ReplaceField {
            id: "goal".into(),
            path: "statement".into(),
            old: Json::str("deploy"),
            new: Json::str(format!("deploy with {secret}")),
            tag: DiffOpTag {
                plane: Some("P1".into()),
                entity_kind: "Goal".into(),
                semantic: true,
            },
        }],
        classification: DiffClassification {
            semantic_ops: 1,
            surface_ops: 0,
            provenance_only_ops: 0,
            ext_ops: 0,
            authority_delta: AuthorityDelta::None,
            budget_delta: Delta::None,
            validity_delta: Delta::None,
            coordination_delta: Delta::None,
            touches_conditioned_rules: Vec::new(),
        },
        provenance: prov,
        derivation: DiffDerivation::default(),
    };
    let errs = check_diff(&diff, &detectors).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HirError::SecretValueInDefinition { .. })));

    // A diff that *removes* a credential-shaped string is not introducing one —
    // the scan covers the ops' carried members either way (the `old` member is
    // the base's record, not an introduction).
    let narrowing = HirDiff {
        ops: vec![DiffOp::ReplaceField {
            id: "goal".into(),
            path: "statement".into(),
            old: Json::str(format!("deploy with {secret}")),
            new: Json::str("deploy with $secret:github"),
            tag: DiffOpTag {
                plane: Some("P1".into()),
                entity_kind: "Goal".into(),
                semantic: true,
            },
        }],
        ..diff.clone()
    };
    // Both members are scanned (the canonical form carries old+new); the
    // introduced-content rule still flags the *old* credential — the
    // conservative verdict at this gate is that the diff record itself is
    // durable history: the AC's rejection targets the introduction, and the
    // scan is over the whole diff record.
    let errs2 = check_diff(&narrowing, &detectors).unwrap_err();
    assert!(errs2
        .iter()
        .any(|e| matches!(e, HirError::SecretValueInDefinition { .. })));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.3-10 — LT-10: fail closed; no env fallback; SecretUnavailable.
// ─────────────────────────────────────────────────────────────────────────────

/// **AC-R-2.8.3-10.** Broker/vault/ledger-writer unavailability fails closed:
/// mediation returns `Refused` (+ a durable `denied` row where the writer
/// lives), never hangs and never falls back to an environment value; the model
/// boundary receives a terminal `SecretUnavailable{channel, reason}`.
#[test]
fn ac_r_2_8_3_10_fail_closed_no_env_fallback() {
    let (mut store, run_id, lease) = open("ac10");

    // Vault unavailable — the resolver denies every source.
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "k");
    registered(&mut broker);
    grant_handle(
        &mut store,
        &run_id,
        &lease,
        "agent.main",
        "github",
        "hnd-gh",
    );
    let decision_ref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-gh");
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            BindRequest {
                channel_id: "github".into(),
                holder: "agent.main".into(),
                env_handle_ref: "env-1".into(),
                env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                mode: hh_monitor::assess::SecretTransport::ProxyInjected,
                monitor_decision_ref: decision_ref.clone(),
                destinations: ["api.github.com".into()].into_iter().collect(),
            },
        )
        .unwrap();

    // `mediate` → Refused{broker_unavailable} — the denial is itself durable
    // (the writer lives), and no environment value is ever substituted.
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &RequestDescriptor {
                effect_id: "eff-1".into(),
                destination: "api.github.com".into(),
                method: Some("GET".into()),
                path: Some("/repos/o/r".into()),
            },
        )
        .unwrap();
    let refused = match out {
        MediationOutcome::Refused(r) => r,
        MediationOutcome::Staged(_) => panic!("mediation must refuse when the vault is down"),
    };
    assert_eq!(refused.code, RefusedCode::BrokerUnavailable);
    // The denied row is durable.
    let denied = store
        .events(&run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.credential.denied")
        .count();
    assert_eq!(denied, 1);

    // The terminal model-facing observation is SecretUnavailable — a closed
    // reason, never a value.
    let term = CredentialBroker::terminal_secret_unavailable("github", &refused);
    let j = term.to_json();
    assert_eq!(
        j.get("reason").and_then(Json::as_str),
        Some("broker_unavailable")
    );

    // The mask set fails closed too — an unavailable source means the set
    // cannot cover every live channel (SV-1 completeness).
    assert!(matches!(
        broker.mask_set(),
        Err(BrokerError::Refused(Refused {
            code: RefusedCode::BrokerUnavailable,
            ..
        }))
    ));

    // The ledger-writer-unavailable half: a fenced lease turns the audit append
    // into a refusal — `bind` then reports audit_unavailable, and the binding
    // is *not* created (durable-before-visible).
    let mut broker2 = broker_with("ghp_x0123456789abcdefghij0123456789");
    registered(&mut broker2);
    // Release then take over the run's writer lease → the old lease is
    // fenced ("lease not active" on the next append).
    store.release(&lease, "test-fence").unwrap();
    let _other = store.acquire_writer("writer-b", &run_id, 60_000).unwrap();
    let r = broker2.bind(
        &mut store,
        &run_id,
        &lease, // the fenced lease
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: decision_ref,
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::AuditUnavailable),
        Ok(_) => panic!("a fenced writer must fail closed"),
    }
    assert!(broker2.live_bindings("github").is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.3-13 — PDP/CDP: no covering decided{allow} ⇒ DecisionMissing; the
// monitor's inputs contain no value.
// ─────────────────────────────────────────────────────────────────────────────

/// **AC-R-2.8.3-13.** A `bind` or `mediate` without a
/// `security.permission.decided{decision = allow}` for a covering
/// `secret_access` grant is `DecisionMissing`; the monitor's inputs contain no
/// value (the decided/granted payloads are value-free by construction).
#[test]
fn ac_r_2_8_3_13_pdp_decides_broker_delivers() {
    let (mut store, run_id, lease) = open("ac13");
    let secret = "ghp_live0123456789abcdefghij0123456789";
    let mut broker = broker_with(secret);
    registered(&mut broker);

    // bind without any decided row → DecisionMissing.
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: "evt-nonexistent".into(),
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::DecisionMissing),
        Ok(_) => panic!("bind without a covering decision must refuse"),
    }

    // A decided{deny} row does not cover (a distinct effect — the ledger
    // enforces unique (effect_id, attempt_no) decisions).
    grant_handle(
        &mut store,
        &run_id,
        &lease,
        "agent.main",
        "github",
        "hnd-gh",
    );
    let deny_id = store.alloc_id("evt");
    let deny = k_ev(
        &store,
        &run_id,
        &deny_id,
        "security.permission.decided",
        Json::obj([
            ("effect_id", Json::str("eff-deny")),
            ("decision", Json::str("deny")),
            ("handle_ids", Json::Arr(vec![Json::str("hnd-gh")])),
            ("attempt_no", Json::Int(1)),
        ]),
    );
    store.append(&run_id, &lease, vec![deny]).unwrap();
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: deny_id.clone(),
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::DecisionMissing),
        Ok(_) => panic!("a decided{{deny}} row must not cover a bind"),
    }

    // With a covering decided{allow}, bind succeeds.
    let decision_ref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-gh");
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            BindRequest {
                channel_id: "github".into(),
                holder: "agent.main".into(),
                env_handle_ref: "env-1".into(),
                env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                mode: hh_monitor::assess::SecretTransport::ProxyInjected,
                monitor_decision_ref: decision_ref,
                destinations: ["api.github.com".into()].into_iter().collect(),
            },
        )
        .unwrap();

    // The monitor's *recorded inputs* contain no value: every
    // `security.permission.*` payload is free of the secret bytes.
    for e in store.events(&run_id).unwrap() {
        if e.class.starts_with("security.permission.") {
            let s = e.payload.to_canonical_string();
            assert!(!s.contains(secret), "{} payload carries a value", e.class);
        }
    }

    // Revoke the handle → the covering decision no longer covers → `mediate`
    // is DecisionMissing (the PDP gate is re-checked at delivery).
    let revoke = k_ev(
        &store,
        &run_id,
        &store.alloc_id("evt"),
        "security.permission.revoked",
        Json::obj([
            ("handle_id", Json::str("hnd-gh")),
            ("cascade", Json::Arr(vec![])),
            ("reason", Json::str("test")),
            ("revoker", Json::str("kernel:test")),
        ]),
    );
    store.append(&run_id, &lease, vec![revoke]).unwrap();
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &RequestDescriptor {
                effect_id: "eff-1".into(),
                destination: "api.github.com".into(),
                method: None,
                path: None,
            },
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::DecisionMissing),
        MediationOutcome::Staged(_) => panic!("mediate must refuse once the grant is revoked"),
    }
}
