//! R3.7 — credential-broker dep legs (BL-19; DF-S1.13-1 + DF-S1.13-4's last
//! residuals; ADR-0359's `ring =0.17.14` admission; ADR-0360):
//!
//! * `dpop` sender constraints **verify** through `crate::dpop` — a
//!   fixture-signed proof (a real `ring` signature generated at test time —
//!   fixture-verified, never live-attested) verifies end-to-end on `bind`,
//!   `mediate`/`mediate_sentinel`, `mint`/`verify_minted` and
//!   `deliver_wrapped`; an absent, mismatched, wrong-context or unverifying
//!   proof refuses `sender_constraint_unmet` on every mode (CC3 — the
//!   fail-closed posture is kept).
//! * `wrapped_long_lived` delivers: `deliver_wrapped` registers the
//!   compensating revocation (`security.credential.wrapped{
//!   compensation_plan_id}`, durable-before-visible with `used`), and a
//!   revoked wrap probes `not_applied` (`probe_wrapped` — LT-05).
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

use hh_secrets::dpop::{self, fixture, DpopAlg};
use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-r37-{}-{tag}-{n}", std::process::id()));
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

/// A `dpop`-constrained channel over `api.github.com` (a `revocation_path`
/// is declared so the wrapped leg's compensator has a destination).
fn dpop_spec(modes: &[SecretTransport]) -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: gh_source(),
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            revocation_path: Some("/_hh/credentials/revoke".into()),
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
        allowed_env_names: Some(["GH_API_TOKEN".into()].into_iter().collect()),
        delivery_modes: modes.iter().copied().collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::Dpop,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "GitHub API token (dpop-bound)".into(),
    }
}

fn test_broker() -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(&gh_source(), "ghp_r37_0123456789abcdefghij0123456");
    CredentialBroker::new(Box::new(vault), "test-fp-key")
}

fn bind_req(mode: SecretTransport, decision_ref: String, proof: Option<DpopProof>) -> BindRequest {
    BindRequest {
        channel_id: "github".into(),
        holder: "agent.main".into(),
        env_handle_ref: "env-1".into(),
        env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
        mode,
        monitor_decision_ref: decision_ref,
        destinations: ["api.github.com".into()].into_iter().collect(),
        dpop_proof: proof,
    }
}

/// The request descriptor a holder presents — `dpop_proof` is the fixture
/// proof signed over `dpop::request_preimage`.
fn req(effect: &str, dest: &str, proof: Option<DpopProof>) -> RequestDescriptor {
    RequestDescriptor {
        effect_id: effect.into(),
        env_handle: "env-1".into(),
        destination: dest.into(),
        method: Some("GET".into()),
        path: Some("/repos".into()),
        dpop_proof: proof,
    }
}

/// Set up a live `dpop` binding under `alg` whose proofs are made by a
/// fixture key that *stays alive* — `bind`, `mediate`, `mint`,
/// `verify_minted` and `deliver_wrapped` proofs are all signatures by the
/// same key, as the PoP contract requires.
fn setup_keyed(
    tag: &str,
    alg: DpopAlg,
    modes: &[SecretTransport],
) -> (
    Store,
    String,
    Lease,
    CredentialBroker,
    CredentialBinding,
    fixture::FixtureKey,
) {
    let (mut store, run_id, lease) = open(tag);
    let mut broker = test_broker();
    broker
        .register_channel(
            "github",
            dpop_spec(modes),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-bind", "hnd-1");
    let mode = modes[0];
    let key = fixture::generate(alg);
    let proof = key.proof_for(&dpop::bind_preimage(
        "github",
        "agent.main",
        "env-1",
        mode.as_str(),
        &dref,
    ));
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            bind_req(mode, dref, Some(proof)),
        )
        .expect("a valid dpop bind proof admits");
    assert_eq!(
        binding.sender_key.as_deref(),
        Some(key.sender_key().as_str()),
        "bind pins the verified key coordinate"
    );
    (store, run_id, lease, broker, binding, key)
}

fn events_of(store: &Store, run_id: &str, class: &str) -> Vec<Json> {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .map(|e| e.payload.clone())
        .collect()
}

// ── `dpop` verifies: bind ────────────────────────────────────────────────

/// A `dpop` channel with a valid fixture-signed bind proof **verifies** on
/// every delivery mode — where R2.10 refused `SenderConstraintUnmet`, the
/// ADR-0359 `ring` verifier admits a real signature (both admitted
/// algorithms).
#[test]
fn r3_7_dpop_bind_verifies_fixture_proof_on_every_mode() {
    for alg in [DpopAlg::Ed25519, DpopAlg::Es256] {
        for mode in [
            SecretTransport::ProxyInjected,
            SecretTransport::MintedScoped,
            SecretTransport::WrappedLongLived,
        ] {
            let (mut store, run_id, lease) = open(&format!("bind-{:?}-{mode:?}", alg));
            let mut broker = test_broker();
            broker
                .register_channel(
                    "github",
                    dpop_spec(&[mode]),
                    ProvenanceRecord::kernel("kernel:test", 1_000),
                )
                .unwrap();
            grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
            let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
            let key = fixture::generate(alg);
            let proof = key.proof_for(&dpop::bind_preimage(
                "github",
                "agent.main",
                "env-1",
                mode.as_str(),
                &dref,
            ));
            let binding = broker
                .bind(
                    &mut store,
                    &run_id,
                    &lease,
                    bind_req(mode, dref, Some(proof)),
                )
                .unwrap_or_else(|e| panic!("{alg:?} {mode:?}: a valid proof binds — {e}"));
            assert_eq!(
                binding.sender_key.as_deref(),
                Some(key.sender_key().as_str()),
                "the bound key coordinate is pinned on the binding"
            );
            // The `bound` row records the verified key — audit-visible.
            let bound = events_of(&store, &run_id, "security.credential.bound");
            assert_eq!(
                bound
                    .last()
                    .and_then(|p| p.get("sender_key"))
                    .and_then(Json::as_str),
                Some(key.sender_key().as_str()),
                "the bound row carries the verified sender_key"
            );
        }
    }
}

/// `bind` still refuses `sender_constraint_unmet` when the proof is absent
/// — the fail-closed posture is kept, never a skipped check.
#[test]
fn r3_7_dpop_bind_absent_proof_refuses() {
    let (mut store, run_id, lease) = open("bind-absent");
    let mut broker = test_broker();
    broker
        .register_channel(
            "github",
            dpop_spec(&[SecretTransport::ProxyInjected]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        bind_req(SecretTransport::ProxyInjected, dref, None),
    );
    assert_eq!(
        r.err().map(|e| e.code),
        Some(RefusedCode::SenderConstraintUnmet),
        "a dpop channel without a proof must refuse"
    );
}

/// `bind` refuses `sender_constraint_unmet` when the signature does not
/// verify over the bind preimage — a proof signed over a *different*
/// context (the `mint` preimage) is proof of nothing here; and a proof
/// under a garbage key is unverifiable, never admissible.
#[test]
fn r3_7_dpop_bind_wrong_context_and_bad_key_refuse() {
    let (mut store, run_id, lease) = open("bind-bad");
    let mut broker = test_broker();
    broker
        .register_channel(
            "github",
            dpop_spec(&[SecretTransport::ProxyInjected]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");

    // A real signature over the wrong context — verified by construction,
    // refused here (the frame's per-verb tag makes replays impossible).
    let key = fixture::generate(DpopAlg::Ed25519);
    let wrong_ctx = key.proof_for(&dpop::mint_preimage("bnd-000001", "api.github.com"));
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        bind_req(
            SecretTransport::ProxyInjected,
            dref.clone(),
            Some(wrong_ctx),
        ),
    );
    assert_eq!(
        r.err().map(|e| e.code),
        Some(RefusedCode::SenderConstraintUnmet),
        "a proof over another verb's preimage refuses"
    );

    // Malformed key material — verification fails, not a crash.
    let malformed = DpopProof {
        alg: DpopAlg::Ed25519,
        public_key: vec![0xAA; 7],
        signature: vec![0xBB; 64],
    };
    let r = broker.bind(
        &mut store,
        &run_id,
        &lease,
        bind_req(SecretTransport::ProxyInjected, dref, Some(malformed)),
    );
    assert_eq!(
        r.err().map(|e| e.code),
        Some(RefusedCode::SenderConstraintUnmet),
        "a malformed key refuses"
    );
}

// ── `dpop` verifies: mediate / mediate_sentinel ─────────────────────────

/// A `dpop` binding + a request proof *by the bound key* stages a delivery
/// end-to-end through `mediate` (and the sentinel entry resolves the same
/// way — one verifier, CC1).
#[test]
fn r3_7_dpop_mediate_verifies_request_proof_end_to_end() {
    for alg in [DpopAlg::Ed25519, DpopAlg::Es256] {
        let (mut store, run_id, lease, mut broker, binding, key) = setup_keyed(
            &format!("med-{alg:?}"),
            alg,
            &[SecretTransport::ProxyInjected],
        );

        let preimage = dpop::request_preimage(
            &binding.binding_id,
            Some("GET"),
            "api.github.com",
            Some("/repos"),
            "eff-req",
        );
        let out = broker
            .mediate(
                &mut store,
                &run_id,
                &lease,
                &binding.binding_id,
                &req("eff-req", "api.github.com", Some(key.proof_for(&preimage))),
            )
            .unwrap();
        let delivery = match out {
            MediationOutcome::Staged(d) => d,
            MediationOutcome::Refused(r) => panic!("a valid proof stages: {r:?}"),
        };
        // The claim drains consume-once — the mediated value crossed.
        let (bid, value) = broker.drain_claim(&delivery.claim_id).unwrap();
        assert_eq!(bid, binding.binding_id);
        assert_eq!(value, "ghp_r37_0123456789abcdefghij0123456");
        assert!(broker.drain_claim(&delivery.claim_id).is_none());
        // `used` is durable — the delivery rode a recorded decision.
        let used = events_of(&store, &run_id, "security.credential.used");
        assert!(!used.is_empty(), "the used row landed before the delivery");

        // The sentinel entry — a placeholder spelling resolves to the
        // binding and the same proof verifies.
        let sentinel = broker
            .placeholder_for(&binding.binding_id)
            .unwrap()
            .spelling
            .clone();
        let out = broker
            .mediate_sentinel(
                &mut store,
                &run_id,
                &lease,
                &sentinel,
                &req("eff-req", "api.github.com", Some(key.proof_for(&preimage))),
            )
            .unwrap();
        assert!(
            matches!(out, MediationOutcome::Staged(_)),
            "sentinel path stages"
        );
    }
}

/// `mediate` on a `dpop` binding without a proof refuses
/// `sender_constraint_unmet` — and the `denied` row is durable *before*
/// the refusal is visible.
#[test]
fn r3_7_dpop_mediate_absent_proof_refuses_denied() {
    let (mut store, run_id, lease, mut broker, binding, _key) = setup_keyed(
        "med-absent",
        DpopAlg::Ed25519,
        &[SecretTransport::ProxyInjected],
    );
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-1", "api.github.com", None),
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => {
            assert_eq!(r.code, RefusedCode::SenderConstraintUnmet)
        }
        _ => panic!("an absent proof must refuse"),
    }
    let denied = events_of(&store, &run_id, "security.credential.denied");
    assert_eq!(
        denied
            .last()
            .and_then(|p| p.get("reason"))
            .and_then(Json::as_str),
        Some("sender_constraint_unmet"),
        "the denied row records the code durably before the refusal"
    );
}

/// A request proof by a *different* key than the bound `sender_key`, or a
/// bound-key signature over a *different* request, refuses
/// `sender_constraint_unmet` — the constraint binds the request to the
/// pinned key and to this request's coordinates.
#[test]
fn r3_7_dpop_mediate_wrong_key_or_wrong_request_refuses() {
    let (mut store, run_id, lease, mut broker, binding, key) = setup_keyed(
        "med-wrong",
        DpopAlg::Ed25519,
        &[SecretTransport::ProxyInjected],
    );
    let good = dpop::request_preimage(
        &binding.binding_id,
        Some("GET"),
        "api.github.com",
        Some("/repos"),
        "eff-req",
    );

    // Another holder's key — a real signature, but not the bound key's.
    let foreign = fixture::generate(DpopAlg::Ed25519).proof_for(&good);
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-req", "api.github.com", Some(foreign)),
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => {
            assert_eq!(r.code, RefusedCode::SenderConstraintUnmet)
        }
        _ => panic!("a foreign key must refuse"),
    }

    // The bound key over a different request's preimage — signed over
    // another effect, replayed here.
    let replay = key.proof_for(&dpop::request_preimage(
        &binding.binding_id,
        Some("GET"),
        "api.github.com",
        Some("/repos"),
        "eff-other",
    ));
    let out = broker
        .mediate(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &req("eff-req", "api.github.com", Some(replay)),
        )
        .unwrap();
    match out {
        MediationOutcome::Refused(r) => {
            assert_eq!(r.code, RefusedCode::SenderConstraintUnmet)
        }
        _ => panic!("a wrong-context signature must refuse"),
    }
}

// ── `dpop` verifies: mint / verify_minted ────────────────────────────────

/// `mint` on a `dpop` channel requires a bound-key signature over
/// `mint_preimage(binding, audience)`; `verify_minted` then requires a
/// presentation proof by that key over `presentation_preimage(token,
/// audience)` — the token is bound to the key it was minted under.
#[test]
fn r3_7_dpop_mint_and_presentation_verify() {
    let (mut store, run_id, lease, mut broker, binding, key) =
        setup_keyed("mint", DpopAlg::Es256, &[SecretTransport::MintedScoped]);

    // Absent proof — refuses, never mints.
    let r = broker.mint(
        &mut store,
        &run_id,
        &lease,
        &binding.binding_id,
        "api.github.com",
        60_000,
        None,
    );
    assert!(matches!(
        r.err().map(|e| {
            match e {
                BrokerError::Refused(r) => r.code,
                other => panic!("expected Refused, got {other:?}"),
            }
        }),
        Some(RefusedCode::SenderConstraintUnmet)
    ));

    // The bound key over the mint preimage — mints.
    let minted = broker
        .mint(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            "api.github.com",
            60_000,
            Some(&key.proof_for(&dpop::mint_preimage(&binding.binding_id, "api.github.com"))),
        )
        .expect("a valid mint proof mints");

    let now = store.now_ms();
    // Presentation by the bound key verifies.
    assert_eq!(
        broker.verify_minted(
            &minted.token,
            "api.github.com",
            now,
            Some(&key.proof_for(&dpop::presentation_preimage(
                &minted.token,
                "api.github.com"
            ))),
        ),
        MintedVerdict::Valid,
        "a bound-key presentation verifies"
    );
    // Absent / foreign-key / wrong-token presentations are `Invalid`.
    assert_eq!(
        broker.verify_minted(&minted.token, "api.github.com", now, None),
        MintedVerdict::Invalid,
        "a dpop-bound token without a proof is Invalid"
    );
    let foreign = fixture::generate(DpopAlg::Es256);
    assert_eq!(
        broker.verify_minted(
            &minted.token,
            "api.github.com",
            now,
            Some(&foreign.proof_for(&dpop::presentation_preimage(
                &minted.token,
                "api.github.com"
            ))),
        ),
        MintedVerdict::Invalid,
        "a foreign-key presentation is Invalid"
    );
    assert_eq!(
        broker.verify_minted(
            &minted.token,
            "api.github.com",
            now,
            Some(&key.proof_for(&dpop::presentation_preimage(
                "mh_mint:v1:forged:0:0",
                "api.github.com"
            ))),
        ),
        MintedVerdict::Invalid,
        "a proof bound to a different token is Invalid"
    );
}

// ── `wrapped_long_lived` delivery ────────────────────────────────────────

/// `deliver_wrapped` lands the wrap: the compensating revocation registers
/// durable-before-visible (`security.credential.wrapped{compensation_plan_
/// id}` in the same append as `used`), the wrapped value crosses to the
/// caller-held seam, and `probe_wrapped` reports `applied`.
#[test]
fn r3_7_wrapped_delivery_registers_compensation_and_probes_applied() {
    let (mut store, run_id, lease) = open("wrap");
    let mut broker = test_broker();
    broker
        .register_channel(
            "github",
            dpop_spec(&[SecretTransport::WrappedLongLived]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let key = fixture::generate(DpopAlg::Ed25519);
    let bind_proof = key.proof_for(&dpop::bind_preimage(
        "github",
        "agent.main",
        "env-1",
        "wrapped_long_lived",
        &dref,
    ));
    let binding = broker
        .bind(
            &mut store,
            &run_id,
            &lease,
            bind_req(SecretTransport::WrappedLongLived, dref, Some(bind_proof)),
        )
        .expect("dpop-wrapped bind verifies");

    // No wrap yet — nothing to probe.
    assert_eq!(
        broker.probe_wrapped(&binding.binding_id),
        WrapProbe::Undeterminable
    );

    let delivery = broker
        .deliver_wrapped(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &WrapDeliveryRequest {
                effect_id: "eff-wrap".into(),
                env_handle: "env-1".into(),
                destination: "api.github.com".into(),
                dpop_proof: Some(key.proof_for(&dpop::request_preimage(
                    &binding.binding_id,
                    None,
                    "api.github.com",
                    None,
                    "eff-wrap",
                ))),
            },
        )
        .expect("the wrap delivers");

    // The wrap itself crossed — caller-held.
    assert_eq!(delivery.value, "ghp_r37_0123456789abcdefghij0123456");
    // The registration + the delivery are one durable fact, in order.
    let evs = store.events(&run_id).unwrap();
    let pos = |class: &str| {
        evs.iter()
            .rposition(|e| e.class == class)
            .expect("row missing")
    };
    let wrapped_pos = pos("security.credential.wrapped");
    let used_pos = pos("security.credential.used");
    assert!(
        wrapped_pos < used_pos,
        "the registration lands before the delivery row"
    );
    let wrapped = &evs[wrapped_pos].payload;
    assert_eq!(
        wrapped.get("compensation_plan_id").and_then(Json::as_str),
        Some(delivery.compensation_plan_id.as_str()),
        "the registered plan is durable"
    );
    assert_eq!(
        wrapped.get("sender_key").and_then(Json::as_str),
        Some(key.sender_key().as_str()),
    );
    assert_eq!(
        wrapped.get("wrap_id").and_then(Json::as_str),
        Some(delivery.wrap_id.as_str()),
    );
    // A wrap that is applied probes `applied`.
    assert_eq!(
        broker.probe_wrapped(&binding.binding_id),
        WrapProbe::Applied
    );
}

/// A revoked wrap probes `not_applied` (LT-05's residual leg): `revoke`
/// produces the destination-side `RevokeIntent`s the registered
/// compensation plan names — `POST /_hh/credentials/revoke` on
/// `api.github.com` — and the wrap then probes `not_applied`.
#[test]
fn r3_7_revoked_wrap_probes_not_applied_and_intents_cover_destination() {
    let (mut store, run_id, lease, mut broker, binding, key) = setup_keyed(
        "wrap-rev",
        DpopAlg::Ed25519,
        &[SecretTransport::WrappedLongLived],
    );
    broker
        .deliver_wrapped(
            &mut store,
            &run_id,
            &lease,
            &binding.binding_id,
            &WrapDeliveryRequest {
                effect_id: "eff-wrap".into(),
                env_handle: "env-1".into(),
                destination: "api.github.com".into(),
                dpop_proof: Some(key.proof_for(&dpop::request_preimage(
                    &binding.binding_id,
                    None,
                    "api.github.com",
                    None,
                    "eff-wrap",
                ))),
            },
        )
        .unwrap();
    assert_eq!(
        broker.probe_wrapped(&binding.binding_id),
        WrapProbe::Applied
    );

    // Revoke — the compensator runs: intents name the destination's
    // revocation path, and the probe flips.
    let out = broker
        .revoke(
            &mut store,
            &run_id,
            &lease,
            RevokeTarget::Binding(binding.binding_id.clone()),
            "operator_revoked",
        )
        .unwrap();
    assert_eq!(out.revoked, vec![binding.binding_id.clone()]);
    assert_eq!(out.intents.len(), 1, "one revocation path declared");
    let intent = &out.intents[0];
    assert_eq!(intent.binding_id, binding.binding_id);
    assert_eq!(intent.host_pattern, "api.github.com");
    assert_eq!(intent.revocation_path, "/_hh/credentials/revoke");
    assert_eq!(
        broker.probe_wrapped(&binding.binding_id),
        WrapProbe::NotApplied,
        "the revoked wrap probes not_applied"
    );
    // A dead binding cannot deliver again.
    let r = broker.deliver_wrapped(
        &mut store,
        &run_id,
        &lease,
        &binding.binding_id,
        &WrapDeliveryRequest {
            effect_id: "eff-late".into(),
            env_handle: "env-1".into(),
            destination: "api.github.com".into(),
            dpop_proof: Some(key.proof_for(&dpop::request_preimage(
                &binding.binding_id,
                None,
                "api.github.com",
                None,
                "eff-late",
            ))),
        },
    );
    assert!(matches!(
        r.err().map(|e| match e {
            BrokerError::Refused(r) => r.code,
            other => panic!("expected Refused, got {other:?}"),
        }),
        Some(RefusedCode::Revoked)
    ));
}

/// `deliver_wrapped`'s typed refusals: non-wrap mode is `mode_not_allowed`,
/// an out-of-scope destination is `out_of_scope`, an absent dpop proof is
/// `sender_constraint_unmet` — and the `isolation ≥ process_sandbox` gate
/// still fires at `bind` (`EnvironmentNotIsolated`).
#[test]
fn r3_7_wrapped_delivery_refusals_and_isolation_gate() {
    // The isolation gate — a weaker env cannot even bind `wrapped_long_lived`.
    let (mut store, run_id, lease) = open("wrap-iso");
    let mut broker = test_broker();
    broker
        .register_channel(
            "github",
            dpop_spec(&[SecretTransport::WrappedLongLived]),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    grant_handle(&mut store, &run_id, &lease, "agent.main", "github", "hnd-1");
    let dref = decide_allow(&mut store, &run_id, &lease, "eff-1", "hnd-1");
    let key = fixture::generate(DpopAlg::Ed25519);
    let weak = BindRequest {
        env_isolation: hh_containment::policy::IsolationClass::None,
        ..bind_req(
            SecretTransport::WrappedLongLived,
            dref.clone(),
            Some(key.proof_for(&dpop::bind_preimage(
                "github",
                "agent.main",
                "env-1",
                "wrapped_long_lived",
                &dref,
            ))),
        )
    };
    let r = broker.bind(&mut store, &run_id, &lease, weak);
    assert_eq!(
        r.err().map(|e| e.code),
        Some(RefusedCode::EnvironmentNotIsolated),
        "wrapped_long_lived below process_sandbox refuses at bind"
    );

    // Mode/out_of_scope/proof refusals at delivery.
    let (_s2, run2, lease2, mut b2, binding, key2) = setup_keyed(
        "wrap-ref",
        DpopAlg::Ed25519,
        &[SecretTransport::WrappedLongLived],
    );
    let mut store = _s2;
    let nonwrap_req = WrapDeliveryRequest {
        effect_id: "eff-nw".into(),
        env_handle: "env-1".into(),
        destination: "api.github.com".into(),
        dpop_proof: Some(key2.proof_for(&dpop::request_preimage(
            &binding.binding_id,
            None,
            "api.github.com",
            None,
            "eff-nw",
        ))),
    };
    // Out-of-scope destination.
    let mut bad = nonwrap_req.clone();
    bad.destination = "evil.example.com".into();
    let r = broker_deliver(
        &mut b2,
        &mut store,
        &run2,
        &lease2,
        &binding.binding_id,
        &bad,
    );
    assert_eq!(r, Some(RefusedCode::OutOfScope));
    // Absent dpop proof.
    let mut bad = nonwrap_req.clone();
    bad.dpop_proof = None;
    let r = broker_deliver(
        &mut b2,
        &mut store,
        &run2,
        &lease2,
        &binding.binding_id,
        &bad,
    );
    assert_eq!(r, Some(RefusedCode::SenderConstraintUnmet));
    // Wrong env handle.
    let mut bad = nonwrap_req.clone();
    bad.env_handle = "env-2".into();
    let r = broker_deliver(
        &mut b2,
        &mut store,
        &run2,
        &lease2,
        &binding.binding_id,
        &bad,
    );
    assert_eq!(r, Some(RefusedCode::OutOfScope));

    // `deliver_wrapped` on a proxy_injected binding is `mode_not_allowed`.
    let (mut s3, run3, lease3) = open("wrap-mode");
    let mut b3 = test_broker();
    b3.register_channel(
        "github",
        dpop_spec(&[
            SecretTransport::ProxyInjected,
            SecretTransport::WrappedLongLived,
        ]),
        ProvenanceRecord::kernel("kernel:test", 1_000),
    )
    .unwrap();
    grant_handle(&mut s3, &run3, &lease3, "agent.main", "github", "hnd-1");
    let dref3 = decide_allow(&mut s3, &run3, &lease3, "eff-1", "hnd-1");
    let k3 = fixture::generate(DpopAlg::Ed25519);
    let b3nd = b3
        .bind(
            &mut s3,
            &run3,
            &lease3,
            bind_req(
                SecretTransport::ProxyInjected,
                dref3.clone(),
                Some(k3.proof_for(&dpop::bind_preimage(
                    "github",
                    "agent.main",
                    "env-1",
                    "proxy_injected",
                    &dref3,
                ))),
            ),
        )
        .unwrap();
    let r = broker_deliver(
        &mut b3,
        &mut s3,
        &run3,
        &lease3,
        &b3nd.binding_id,
        &WrapDeliveryRequest {
            effect_id: "eff-m".into(),
            env_handle: "env-1".into(),
            destination: "api.github.com".into(),
            dpop_proof: Some(k3.proof_for(&dpop::request_preimage(
                &b3nd.binding_id,
                None,
                "api.github.com",
                None,
                "eff-m",
            ))),
        },
    );
    assert_eq!(r, Some(RefusedCode::ModeNotAllowed));
}

fn broker_deliver(
    broker: &mut CredentialBroker,
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    binding_id: &str,
    req: &WrapDeliveryRequest,
) -> Option<RefusedCode> {
    broker
        .deliver_wrapped(store, run_id, lease, binding_id, req)
        .err()
        .map(|e| match e {
            BrokerError::Refused(r) => r.code,
            other => panic!("expected Refused, got {other:?}"),
        })
}

/// A `dpop`-constrained wrap refuses `sender_constraint_unmet` when the
/// proof is by another key — the sender constraint binds wrapped delivery
/// too, not only injected/minted modes.
#[test]
fn r3_7_wrapped_delivery_foreign_key_proof_refuses() {
    let (mut store, run_id, lease, mut broker, binding, _key) = setup_keyed(
        "wrap-fk",
        DpopAlg::Ed25519,
        &[SecretTransport::WrappedLongLived],
    );
    let foreign = fixture::generate(DpopAlg::Ed25519);
    let r = broker.deliver_wrapped(
        &mut store,
        &run_id,
        &lease,
        &binding.binding_id,
        &WrapDeliveryRequest {
            effect_id: "eff-fk".into(),
            env_handle: "env-1".into(),
            destination: "api.github.com".into(),
            dpop_proof: Some(foreign.proof_for(&dpop::request_preimage(
                &binding.binding_id,
                None,
                "api.github.com",
                None,
                "eff-fk",
            ))),
        },
    );
    assert!(matches!(
        r.err().map(|e| match e {
            BrokerError::Refused(r) => r.code,
            other => panic!("expected Refused, got {other:?}"),
        }),
        Some(RefusedCode::SenderConstraintUnmet)
    ));
}
