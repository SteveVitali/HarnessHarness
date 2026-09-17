//! S1.13 secret-visibility battery — the SV-1..SV-10 invariants (§5g.3 §7)
//! that the AC tests don't already pin. Each test names the invariant(s) it
//! covers; each fails if the behaviour is removed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_hir::records::GrantConstraints;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::authority::AuthorityClass;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-sv-{}-{tag}-{n}", std::process::id()));
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
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
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

fn broker_with(source: SecretSource, value: &str) -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(&source, value);
    CredentialBroker::new(Box::new(vault), "test-fp-key")
}

fn gh_source() -> SecretSource {
    SecretSource::OperatorVault {
        vault_ref: "vault:github".into(),
    }
}

fn bound_binding(
    broker: &mut CredentialBroker,
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    handle_id: &str,
    effect_id: &str,
) -> CredentialBinding {
    grant_handle(store, run_id, lease, "agent.main", "github", handle_id);
    let dref = decide_allow(store, run_id, lease, effect_id, handle_id);
    broker
        .bind(
            store,
            run_id,
            lease,
            BindRequest {
                channel_id: "github".into(),
                holder: "agent.main".into(),
                env_handle_ref: "env-1".into(),
                env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                mode: hh_monitor::assess::SecretTransport::ProxyInjected,
                monitor_decision_ref: dref,
                destinations: ["api.github.com".into()].into_iter().collect(),
            },
        )
        .unwrap()
}

fn req(effect: &str, dest: &str, path: Option<&str>) -> RequestDescriptor {
    RequestDescriptor {
        effect_id: effect.into(),
        destination: dest.into(),
        method: Some("GET".into()),
        path: path.map(str::to_string),
    }
}

fn events_of_class(store: &Store, run_id: &str, class: &str) -> Vec<Json> {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .map(|e| e.payload.clone())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-1: the mask set covers every live channel AND retained rotated values.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-1.** After `rotate`, the old value stays in the mask set (live =
/// false), the new value is masked too, and a stale `expected_revision` is
/// `Conflict{current}` — never a silent overwrite.
#[test]
fn sv1_mask_set_covers_rotated_values() {
    let (mut store, run_id, lease) = open("sv1");
    let old_v = "ghp_old0123456789abcdefghij0123456";
    let new_v = "ghp_new0123456789abcdefghij0123456";
    let mut broker = broker_with(gh_source(), old_v);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    // Rotate to a new source coordinate carrying the new value.
    let new_source = SecretSource::OperatorVault {
        vault_ref: "vault:github:r2".into(),
    };
    // The vault needs the new coordinate populated.
    // (StaticVault is immutable inside the broker — rebuild via a composite
    // isn't exposed; rotate's contract is `new_source` + the resolver holds it.
    // For the test we use a resolver that knows both.)
    let mut vault = StaticVault::default();
    vault.put(&gh_source(), old_v);
    vault.put(&new_source, new_v);
    let mut broker = CredentialBroker::new(Box::new(vault), "k");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    // Stale revision → Conflict{current}.
    let r = broker.rotate(&mut store, &run_id, &lease, "github", 7, new_source.clone());
    match r {
        Err(BrokerError::Conflict {
            channel_id,
            expected,
            current,
        }) => {
            assert_eq!(channel_id, "github");
            assert_eq!(expected, 7);
            assert_eq!(current, 1);
        }
        _ => panic!("a stale expected_revision must be Conflict{{current}}"),
    }

    let new_rev = broker
        .rotate(&mut store, &run_id, &lease, "github", 1, new_source)
        .unwrap();
    assert_eq!(new_rev, 2);

    let ms = broker.mask_set().unwrap();
    // Both revisions masked.
    let old_e = ms.entry_for(old_v).unwrap();
    assert_eq!(old_e.revision, 1);
    assert!(!old_e.live);
    let new_e = ms.entry_for(new_v).unwrap();
    assert_eq!(new_e.revision, 2);
    assert!(new_e.live);
    // The `rotated` + (implicit) `revoked` rows are durable.
    assert_eq!(
        events_of_class(&store, &run_id, "security.credential.rotated").len(),
        1
    );
    // No live bindings were bound → no implicit-revoke row is required.
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-5 + SV-6: durable `used` per mediation; run-terminal revoke.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-5.** Every `mediate` produces a durable `security.credential.used` row
/// *before* the `Delivery` is returned — the `Delivery.used_event_id` resolves
/// against committed events.
#[test]
fn sv5_used_row_durable_before_delivery() {
    let (mut store, run_id, lease) = open("sv5");
    let mut broker = broker_with(gh_source(), "ghp_sv5_0123456789abcdefghij0123456");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");

    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-1", "api.github.com", Some("/repos/o/r")),
        )
        .unwrap();
    let delivery = match out {
        MediationOutcome::Staged(d) => d,
        MediationOutcome::Refused(r) => panic!("unexpected refusal: {r}"),
    };
    // The used row is committed *now* — durable before visible.
    let used = events_of_class(&store, &run_id, "security.credential.used");
    assert_eq!(used.len(), 1);
    assert!(
        used[0]
            .to_canonical_string()
            .contains(&delivery.used_event_id)
            || store
                .events(&run_id)
                .unwrap()
                .iter()
                .any(|e| e.event_id == delivery.used_event_id
                    && e.class == "security.credential.used")
    );
    // The delivery is content-free: no value in the record.
    assert!(!format!("{delivery:?}").contains("ghp_sv5_"));
}

/// **SV-6.** `on_run_terminal` revokes every live binding for the run; a
/// revoked binding's `mediate` is `Refused{revoked}` (durable `denied` row);
/// an explicit `revoke` is idempotent.
#[test]
fn sv6_run_terminal_revokes_bindings() {
    let (mut store, run_id, lease) = open("sv6");
    let mut broker = broker_with(gh_source(), "ghp_sv6_0123456789abcdefghij0123456");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let b1 = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");
    let b2 = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-2", "eff-2");

    // Run terminal → every live binding on the run drops, durably.
    let dropped = broker.on_run_terminal(&mut store, &run_id, &lease).unwrap();
    assert!(dropped.contains(&b1.binding_id));
    assert!(dropped.contains(&b2.binding_id));
    assert!(!broker.binding(&b1.binding_id).unwrap().is_live());
    assert_eq!(
        broker.binding(&b1.binding_id).unwrap().state,
        BindingState::Revoked
    );

    // A revoked binding refuses — durable denied row, closed code.
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &b1.binding_id,
            &req("eff-1", "api.github.com", None),
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::Revoked),
        MediationOutcome::Staged(_) => panic!("a revoked binding must not mediate"),
    }
    assert_eq!(
        events_of_class(&store, &run_id, "security.credential.denied").len(),
        1
    );

    // Idempotent revoke — no duplicate row.
    let out = broker
        .revoke(
            &mut store,
            &run_id,
            &lease,
            RevokeTarget::Binding(b1.binding_id.clone()),
            "again",
        )
        .unwrap();
    assert!(out.already);
    let revokes = events_of_class(&store, &run_id, "security.credential.revoked");
    // exactly one (the run_terminal batch) — the idempotent second call adds none.
    assert_eq!(revokes.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-7: scope and delegation attenuate; nothing widens.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-7.** `grant` refuses a destination outside the channel's declared set
/// (`ScopeExceedsChannel`); `mediate` to a destination outside the *binding's*
/// set is `Refused{out_of_scope}`; a sub-principal issuer is refused
/// (`IssuerAuthorityInsufficient`).
#[test]
fn sv7_scope_attenuates_never_widens() {
    let (mut store, run_id, lease) = open("sv7");
    let mut broker = broker_with(gh_source(), "ghp_sv7_0123456789abcdefghij0123456");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    // grant: scope ⊄ channel destinations → ScopeExceedsChannel.
    let r = broker.grant(
        &run_id,
        "agent.main",
        "github",
        &["evil.example.com".into()].into_iter().collect(),
        GrantConstraints::default(),
        &hh_hir::records::Issuer {
            authority: AuthorityClass::Principal,
            reference: "principal:test".into(),
        },
    );
    match r {
        Err(BrokerError::ScopeExceedsChannel { channel_id, scope }) => {
            assert_eq!(channel_id, "github");
            assert_eq!(scope, "evil.example.com");
        }
        _ => panic!("a scope outside the channel must be ScopeExceedsChannel"),
    }

    // grant: a delegate issuer never confers.
    let r = broker.grant(
        &run_id,
        "agent.main",
        "github",
        &["api.github.com".into()].into_iter().collect(),
        GrantConstraints::default(),
        &hh_hir::records::Issuer {
            authority: AuthorityClass::Delegate,
            reference: "agent.main".into(),
        },
    );
    assert!(matches!(
        r,
        Err(BrokerError::IssuerAuthorityInsufficient { .. })
    ));

    // bind narrowed to a subset, then mediate outside it → out_of_scope.
    let binding = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-1", "other.github.com", None),
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::OutOfScope),
        MediationOutcome::Staged(_) => panic!("out-of-binding destination must refuse"),
    }
}

/// **SV-7 (path).** An ambiguous request path is `Refused{ambiguous_path}` —
/// the Stage-1 LT-02 shape (dot-segments, encoded separators, backslashes).
#[test]
fn sv7_ambiguous_path_refused() {
    let (mut store, run_id, lease) = open("sv7p");
    let mut broker = broker_with(gh_source(), "ghp_sv7p_0123456789abcdefghij012345");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");

    for bad in ["/a/../b", "/a/%2e%2e/b", "/a\\b", "/a/%2f/b"] {
        let out = broker
            .mediate(
                &mut store,
                &run_id,
                &lease,
                &binding.binding_id,
                &req("eff-1", "api.github.com", Some(bad)),
            )
            .unwrap();
        match out {
            MediationOutcome::Refused(r) => {
                assert_eq!(r.code, RefusedCode::AmbiguousPath, "path {bad:?}")
            }
            MediationOutcome::Staged(_) => panic!("ambiguous path {bad:?} must refuse"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-3/SV-10: SecretRef is metadata-only; placeholders are binding-scoped.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-3 + SV-10.** `SecretRef` renders `channel_id (description)` only;
/// placeholders are distinct per binding and scoped to `(run, binding)` —
/// non-transferable on the Stage-1 best-effort basis.
#[test]
fn sv3_sv10_ref_metadata_only_placeholder_scoped() {
    let (mut store, run_id, lease) = open("sv3");
    let secret = "ghp_sv3_0123456789abcdefghij0123456";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    let r = broker.channel("github").unwrap().secret_ref();
    assert_eq!(r.channel_id, "github");
    assert_eq!(r.render(), "github (GitHub API token)");
    assert!(!r.render().contains(secret));

    let b1 = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");
    let b2 = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-2", "eff-2");
    let p1 = broker.placeholder_for(&b1.binding_id).unwrap();
    let p2 = broker.placeholder_for(&b2.binding_id).unwrap();
    assert_ne!(p1.spelling, p2.spelling);
    assert!(p1.spelling.starts_with("mh_secret:v1:"));
    // Binding-scoped: the nonce is a digest of (run_id ∥ binding_id) — the
    // struct records the coordinate, the spelling carries the channel.
    assert_eq!(p1.binding_id, b1.binding_id);
    assert_eq!(p2.binding_id, b2.binding_id);
    assert!(p1.spelling.contains("github"));
    // Deterministic: re-minting the same binding yields the same spelling
    // (env snapshot stability); a different binding differs (SV-10).
    assert_eq!(Placeholder::mint(&run_id, &b1).spelling, p1.spelling);
    assert_ne!(Placeholder::mint(&run_id, &b2).spelling, p1.spelling);
    assert!(p1.spelling.len() >= 16);
    assert!(p1.is_well_formed());
    // A placeholder is not a secret value — detectors pass it through.
    assert!(!p1.spelling.contains(secret));
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-8: canary tripwire; kernel_only refuses bind; kernel_use is audited.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-8 (canary).** A canary channel refuses `bind`/`kernel_use` with
/// `Refused{canary}` and appends `security.secret.leak_detected` *before* the
/// refusal is visible (ADR-0059 D3).
#[test]
fn sv8_canary_never_injected() {
    let (mut store, run_id, lease) = open("sv8");
    let canary_v = "ghp_canary_tripwire_value_0123456789";
    let mut spec = github_spec();
    spec.canary = true;
    let mut broker = broker_with(gh_source(), canary_v);
    broker
        .register_channel(
            "tripwire",
            spec,
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "tripwire".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: "evt-x".into(),
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::Canary),
        Ok(_) => panic!("a canary channel must never bind"),
    }
    let leaks = events_of_class(&store, &run_id, "security.secret.leak_detected");
    assert_eq!(leaks.len(), 1);
    // The leak row carries the location + detector — never the value.
    assert!(!leaks[0].to_canonical_string().contains(canary_v));
    assert!(leaks[0].to_canonical_string().contains("canary"));
}

/// **SV-8 (kernel_use).** A `bindable = false` / `kernel_only` channel refuses
/// `bind` (`NotBindable`) and serves `kernel_use` with a durable
/// `used{destination = kernel:<purpose>}` row.
#[test]
fn sv8_kernel_use_audited_and_bind_refused() {
    let (mut store, run_id, lease) = open("sv8k");
    let secret = "ghp_kernel_0123456789abcdefghij012345";
    let mut spec = github_spec();
    spec.bindable = false;
    spec.access_class = AccessClass::KernelOnly;
    spec.delivery_modes = BTreeSet::new();
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel("gw", spec, ProvenanceRecord::kernel("kernel:test", 1_000))
        .unwrap();

    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "gw".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: "evt-x".into(),
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::NotBindable),
        Ok(_) => panic!("a kernel_only channel must refuse bind"),
    }

    let v = broker
        .kernel_use(
            &mut store,
            &run_id,
            &lease,
            "gw",
            KernelPurpose::GatewayAuth,
            "kernel:gateway",
            "eff-k",
        )
        .unwrap();
    assert_eq!(v, secret);
    let used = events_of_class(&store, &run_id, "security.credential.used");
    assert_eq!(used.len(), 1);
    let s = used[0].to_canonical_string();
    assert!(s.contains("kernel:gateway_auth"));
    assert!(!s.contains(secret));
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-9: mode/isolation gates; mint is a typed Stage-2 deferral.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-9.** A mode outside `delivery_modes_allowed` is `ModeNotAllowed`;
/// `wrapped_long_lived` under insufficient isolation is
/// `EnvironmentNotIsolated`; `mint` is `Deferred{stage: 2}` (failure-typed,
/// never a panic).
#[test]
fn sv9_mode_isolation_gates_and_typed_deferral() {
    let (mut store, run_id, lease) = open("sv9");
    let mut broker = broker_with(gh_source(), "ghp_sv9_0123456789abcdefghij0123456");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");

    // minted_scoped is not in delivery_modes → ModeNotAllowed.
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::MintedScoped,
            monitor_decision_ref: dref.clone(),
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::ModeNotAllowed),
        Ok(_) => panic!("a non-declared mode must refuse"),
    }

    // wrapped_long_lived under `none` isolation → EnvironmentNotIsolated.
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::None,
            mode: hh_monitor::assess::SecretTransport::WrappedLongLived,
            monitor_decision_ref: dref,
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::EnvironmentNotIsolated),
        Ok(_) => panic!("wrapped_long_lived under no isolation must refuse"),
    }

    // mint — the failure-typed Stage-2 SPI.
    match broker.mint("bnd-x", "aud", 60_000) {
        Err(BrokerError::Deferred { verb, stage }) => {
            assert_eq!(verb, "mint");
            assert_eq!(stage, 2);
        }
        _ => panic!("mint must be a typed deferral"),
    }
}

/// **SV-9 (sender constraint).** A `minted_scoped` bind on a
/// `sender_constraint`-declaring channel is `SenderConstraintUnmet` — Stage 1
/// has no verifier, so it fails closed.
#[test]
fn sv9_sender_constraint_fails_closed() {
    let (mut store, run_id, lease) = open("sv9s");
    let mut spec = github_spec();
    spec.sender_constraint = SenderConstraint::Dpop;
    spec.delivery_modes
        .insert(hh_monitor::assess::SecretTransport::MintedScoped);
    let mut broker = broker_with(gh_source(), "ghp_sv9s_0123456789abcdefghij012345");
    broker
        .register_channel(
            "github",
            spec,
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        BindRequest {
            channel_id: "github".into(),
            holder: "agent.main".into(),
            env_handle_ref: "env-1".into(),
            env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
            mode: hh_monitor::assess::SecretTransport::MintedScoped,
            monitor_decision_ref: dref,
            destinations: ["api.github.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::SenderConstraintUnmet),
        Ok(_) => panic!("an unverifiable sender constraint must fail closed"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-2: no durable surface carries a value; redaction tombstones are
// content-free.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-2.** After a full lifecycle (grant → decide → bind → mediate → rotate
/// → revoke), no committed event payload contains a secret value, and every
/// `security.*` row is content-free (refs, ids, codes only).
#[test]
fn sv2_no_durable_surface_carries_value() {
    let (mut store, run_id, lease) = open("sv2");
    let old_v = "ghp_sv2old_0123456789abcdefghij01234";
    let new_v = "ghp_sv2new_0123456789abcdefghij01234";
    let new_source = SecretSource::OperatorVault {
        vault_ref: "vault:github:r2".into(),
    };
    let mut vault = StaticVault::default();
    vault.put(&gh_source(), old_v);
    vault.put(&new_source, new_v);
    let mut broker = CredentialBroker::new(Box::new(vault), "k");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");
    let _ = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-1", "api.github.com", Some("/x")),
        )
        .unwrap();
    broker
        .rotate(&mut store, &run_id, &lease, "github", 1, new_source)
        .unwrap();
    broker
        .revoke(
            &mut store,
            &run_id,
            &lease,
            RevokeTarget::Channel("github".into()),
            "done",
        )
        .unwrap();

    // Every committed payload is value-free — both revisions.
    for e in store.events(&run_id).unwrap() {
        let s = e.payload.to_canonical_string();
        assert!(!s.contains(old_v), "{} payload carries old value", e.class);
        assert!(!s.contains(new_v), "{} payload carries new value", e.class);
    }
    // The mask set still covers the retired value (SV-1 post-revoke? — the
    // channel is revoked so mask_set skips it; the retained rotated values were
    // asserted in sv1).
}

/// **SV-2 (tombstones).** `redact` replaces a hit with a content-free
/// tombstone — `REDACTED[<detector>:<label>]` never echoes the value.
#[test]
fn sv2_redaction_tombstones_are_content_free() {
    let secret = "ghp_tombstone_0123456789abcdefghij01";
    let mut ms = MaskSet::default();
    ms.insert(MaskEntry {
        channel_id: "github".into(),
        revision: 1,
        value: secret.into(),
        fingerprint: SecretFingerprint {
            channel_id: "github".into(),
            revision: 1,
            digest: "d".repeat(32),
        },
        live: true,
        placeholder: None,
    });
    let detectors = DetectorSet::standard(ms);
    let (out, hits) = redact(&format!("token is {secret} ok"), &detectors);
    assert!(!out.contains(secret));
    assert!(out.contains("REDACTED["));
    assert_eq!(hits.len(), 1);
    // The tombstone renders the detector + fingerprint — never the bytes.
    let t = hits[0]
        .tombstone
        .as_ref()
        .expect("a known-value hit carries a tombstone")
        .render();
    assert!(t.starts_with("REDACTED["));
    assert!(!t.contains(secret));
}

// ─────────────────────────────────────────────────────────────────────────────
// SV-4: deny-by-default env; credential-shaped names withheld; known values
// masked inside allowed vars.
// ─────────────────────────────────────────────────────────────────────────────

/// **SV-4.** `env_apply` is deny-by-default: non-allow-listed names are absent,
/// credential-shaped names are withheld even when allow-listed, a bound channel
/// projects only its placeholder, and a known secret value inside an ordinary
/// allowed variable is masked.
#[test]
fn sv4_env_deny_by_default_and_masking() {
    let (mut store, run_id, lease) = open("sv4");
    let secret = "ghp_sv4_0123456789abcdefghij0123456";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(&mut broker, &mut store, &run_id, &lease, "hnd-1", "eff-1");
    let ph = broker.placeholder_for(&binding.binding_id).unwrap().clone();

    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    let base_env: BTreeMap<String, String> = [
        ("PATH".into(), "/usr/bin".into()),
        (
            "AWS_SECRET_ACCESS_KEY".into(),
            "AKIAIOSFODNN7EXAMPLE".into(),
        ),
        ("MY_TOKEN".into(), "x".into()),
        ("LEAKED".into(), format!("prefix {secret} suffix")),
        ("NOT_ALLOWED".into(), "y".into()),
    ]
    .into_iter()
    .collect();

    // Allow-list PATH, LEAKED, GH_API_TOKEN — and even the credential-shaped
    // MY_TOKEN (it must still be withheld).
    let spec = EnvSpec {
        allowlist: [
            "PATH",
            "LEAKED",
            "GH_API_TOKEN",
            "MY_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        bindings: vec![EnvBinding {
            env_name: "GH_API_TOKEN".into(),
            channel_id: "github".into(),
            binding_id: binding.binding_id.clone(),
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
        }],
    };
    let mut bound = BTreeMap::new();
    bound.insert("GH_API_TOKEN".to_string(), ph.clone());
    let projected = env_apply(&spec, &base_env, &bound, &detectors).unwrap();

    // PATH carried verbatim; GH_API_TOKEN is the placeholder.
    assert_eq!(projected.vars.get("PATH").unwrap(), "/usr/bin");
    assert_eq!(projected.vars.get("GH_API_TOKEN").unwrap(), &ph.spelling);
    // Credential-shaped names withheld even when allow-listed.
    assert!(!projected.vars.contains_key("MY_TOKEN"));
    assert!(!projected.vars.contains_key("AWS_SECRET_ACCESS_KEY"));
    assert!(projected.withheld.contains("MY_TOKEN"));
    // Non-allow-listed absent.
    assert!(!projected.vars.contains_key("NOT_ALLOWED"));
    // A known secret inside an allowed var is masked.
    let leaked = projected.vars.get("LEAKED").unwrap();
    assert!(!leaked.contains(secret));
    assert!(leaked.contains("REDACTED["));
    // The env sweep is clean.
    assert!(env_sweep(&projected, &detectors).is_empty());
}
