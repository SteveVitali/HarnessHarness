//! R2.10 — credential-broker legs (DF-S1.13-1's sender-constraint half):
//!
//! * `audience` sender constraints **verify** offline through the broker —
//!   a `minted_scoped` bind on an `audience`-declaring channel admits, `mint`
//!   scopes the token's audience to the binding's declared destinations, and
//!   `verify_minted` checks audience + MAC + expiry + liveness at the
//!   destination. For injected modes the audience *is* the destination,
//!   fenced by the channel's declared `destinations`.
//! * `dpop` stays a typed `SenderConstraintUnmet` on **every** delivery mode
//!   — pure-std has no asymmetric proof-of-possession verifier; an
//!   unverifiable constraint refuses, never fabricates an accept (the
//!   ticket's explicit trap).
//!
//! Each test fails if the behaviour is removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_monitor::assess::SecretTransport;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-r210-{}-{tag}-{n}", std::process::id()));
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

fn gh_source() -> SecretSource {
    SecretSource::OperatorVault {
        vault_ref: "vault:github".into(),
    }
}

/// A channel over `api.github.com` declaring `sender_constraint` and the
/// given delivery modes.
fn constrained_spec(constraint: SenderConstraint, modes: &[SecretTransport]) -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: gh_source(),
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            revocation_path: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
        allowed_env_names: Some(["GH_API_TOKEN".into()].into_iter().collect()),
        delivery_modes: modes.iter().copied().collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: constraint,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "GitHub API token".into(),
    }
}

fn broker() -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(&gh_source(), "ghp_r210_0123456789abcdefghij0123456");
    CredentialBroker::new(Box::new(vault), "test-fp-key")
}

fn bind_req(mode: SecretTransport, decision_ref: String) -> BindRequest {
    BindRequest {
        channel_id: "github".into(),
        holder: "agent.main".into(),
        env_handle_ref: "env-1".into(),
        env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
        mode,
        monitor_decision_ref: decision_ref,
        destinations: ["api.github.com".into()].into_iter().collect(),
    }
}

fn refused_code<T>(r: Result<T, Refused>) -> RefusedCode {
    match r {
        Err(e) => e.code,
        Ok(_) => panic!("expected a typed refusal"),
    }
}

fn broker_refused_code<T>(r: Result<T, BrokerError>) -> RefusedCode {
    match r {
        Err(BrokerError::Refused(e)) => e.code,
        Err(e) => panic!("expected a typed refusal, got {e:?}"),
        Ok(_) => panic!("expected a typed refusal"),
    }
}

// ── `audience` verifies ─────────────────────────────────────────────────

/// A `minted_scoped` bind on an `audience`-declaring channel **succeeds** —
/// the constraint is verified, not refused: the minted token binds the
/// audience and `verify_minted` is the destination-side check.
#[test]
fn r2_10_audience_minted_scoped_bind_admits_and_token_verifies() {
    let (mut store, run_id, lease) = open("aud-mint");
    let mut broker = broker();
    broker
        .register_channel(
            "github",
            constrained_spec(SenderConstraint::Audience, &[SecretTransport::MintedScoped]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");

    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            bind_req(SecretTransport::MintedScoped, dref),
        )
        .expect("audience + minted_scoped binds — the verifier exists");

    // The token is the audience binding: mint for a declared audience.
    let minted = broker
        .mint(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            "api.github.com",
            60_000,
        )
        .expect("in-scope audience mints");
    let now = store.now_ms();
    assert_eq!(
        broker.verify_minted(&minted.token, "api.github.com", now),
        MintedVerdict::Valid,
        "the token verifies for the audience it was minted to"
    );
    assert_eq!(
        broker.verify_minted(&minted.token, "evil.example.com", now),
        MintedVerdict::Invalid,
        "a different audience does not verify — the constraint is real"
    );
}

/// `mint` for an audience outside the binding's declared destinations is
/// `out_of_scope` — the audience constraint is *enforced*, not advisory.
#[test]
fn r2_10_audience_mint_outside_binding_scope_refuses() {
    let (mut store, run_id, lease) = open("aud-scope");
    let mut broker = broker();
    broker
        .register_channel(
            "github",
            constrained_spec(SenderConstraint::Audience, &[SecretTransport::MintedScoped]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            bind_req(SecretTransport::MintedScoped, dref),
        )
        .unwrap();
    let r = broker.mint(
        &mut store,
        &run_id,
        &lease,
        &binding.binding_id,
        "evil.example.com",
        60_000,
    );
    assert_eq!(broker_refused_code(r), RefusedCode::OutOfScope);
}

/// `audience` + `proxy_injected` binds and mediates — the destination-scope
/// fence (bind's `destinations ⊆ channel.destinations`, mediate's
/// `request.destination ∈ binding.destinations`) *is* the audience check
/// for an injected credential.
#[test]
fn r2_10_audience_injected_delivery_fenced_by_destinations() {
    let (mut store, run_id, lease) = open("aud-inj");
    let mut broker = broker();
    broker
        .register_channel(
            "github",
            constrained_spec(
                SenderConstraint::Audience,
                &[SecretTransport::ProxyInjected],
            ),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            bind_req(SecretTransport::ProxyInjected, dref),
        )
        .expect("audience + proxy_injected binds — destination scope verifies");

    // A request to the declared audience stages a delivery.
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &RequestDescriptor {
                env_handle: "env-1".into(),
                effect_id: "eff-1".into(),
                destination: "api.github.com".into(),
                method: Some("GET".into()),
                path: None,
            },
        )
        .unwrap();
    assert!(matches!(out, MediationOutcome::Staged(_)));

    // A request to any other audience is refused `out_of_scope` — the
    // constraint bites at delivery, not just at bind.
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &RequestDescriptor {
                env_handle: "env-1".into(),
                effect_id: "eff-2".into(),
                destination: "evil.example.com".into(),
                method: Some("GET".into()),
                path: None,
            },
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::OutOfScope),
        _ => panic!("an out-of-audience request must refuse"),
    }
}

// ── `dpop` stays typed-refused on every mode ─────────────────────────────

/// `dpop` refuses `SenderConstraintUnmet` on **every** delivery mode —
/// pure-std has no asymmetric signature verification, so a `dpop` channel
/// can never accept under any mode (the ticket's fabrication trap).
#[test]
fn r2_10_dpop_refuses_every_delivery_mode() {
    let (mut store, run_id, lease) = open("dpop");
    let mut broker = broker();
    broker
        .register_channel(
            "github",
            constrained_spec(
                SenderConstraint::Dpop,
                &[
                    SecretTransport::ProxyInjected,
                    SecretTransport::MintedScoped,
                    SecretTransport::WrappedLongLived,
                ],
            ),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");

    for (i, mode) in [
        SecretTransport::ProxyInjected,
        SecretTransport::MintedScoped,
        SecretTransport::WrappedLongLived,
    ]
    .into_iter()
    .enumerate()
    {
        let dref = decide_allow(&mut store, &run_id, &lease, &format!("eff-{i}"), "hnd-1");
        let r = broker.bind(&mut store, &run_id, &lease, bind_req(mode, dref));
        assert_eq!(
            refused_code(r),
            RefusedCode::SenderConstraintUnmet,
            "dpop must refuse on mode {}",
            mode.as_str()
        );
    }
}
