//! S3.11b — the §5g.3 Stage-3 credential/egress-security executables
//! (LT-03, LT-11, LT-12; AC-R-2.8.3-{3,11,12}):
//!
//! - **LT-03** — an AgentDojo-shaped injection never sees a value: the
//!   wire carries the placeholder spelling or the request is refused
//!   (`out_of_scope` for a forged/foreign sentinel, `canary` for a
//!   tripwire); a real value at a sink is a `leak_detected` row — the
//!   `secret_leak` veto's input.
//! - **LT-11** — the request-view scan over the run's model requests is
//!   empty; a planted value in a request view is a `request_view` leak.
//! - **LT-12** — encoded forms (base64/hex/percent) of a known value are
//!   detected by `known_value_encoded`, and `secret_detector_miss_rate`
//!   is reported over the seeded corpus.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-s311b-{}-{tag}-{n}", std::process::id()));
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
            revocation_path: None,
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

fn gh_source() -> SecretSource {
    SecretSource::OperatorVault {
        vault_ref: "vault:github".into(),
    }
}

fn broker_with(source: SecretSource, value: &str) -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(&source, value);
    CredentialBroker::new(Box::new(vault), "test-fp-key")
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
        env_handle: "env-1".into(),
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
// LT-03 (AC-R-2.8.3-3): the AgentDojo-shaped injection fixture — a tool
// result instructing the model to exfiltrate the credential. At the broker
// the outcome is placeholder-or-refused, never a value: a forged sentinel
// resolves to no live binding; a copied sentinel on a foreign env or an
// out-of-scope destination is refused; a canary presented anywhere lands a
// `leak_detected` row.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn lt03_forged_sentinel_is_refused_and_audited() {
    let (mut store, run_id, lease) = open("lt03-forged");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );

    // The injected instruction presents a sentinel spelling the broker
    // never minted — `out_of_scope`, denied durably *before* the refusal is
    // visible, and the value is never approached.
    let outcome = broker
        .mediate_sentinel(
            &mut store,
            &run_id,
            &lease,
            "mh_secret:forged-not-minted",
            &req("eff-exfil", "evil.example.com", None),
        )
        .unwrap();
    match outcome {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::OutOfScope),
        MediationOutcome::Staged(_) => panic!("a forged sentinel must never stage"),
    }
    let denied = events_of_class(&store, &run_id, "security.credential.denied");
    assert_eq!(denied.len(), 1);
    assert_eq!(
        denied[0].get("reason").and_then(Json::as_str),
        Some("out_of_scope")
    );
    // The durable row carries no secret bytes (SV-2).
    assert!(!denied[0].to_canonical_string().contains(secret));
}

#[test]
fn lt03_bound_sentinel_to_foreign_destination_is_refused() {
    let (mut store, run_id, lease) = open("lt03-scope");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .expect("a live binding has a placeholder")
        .spelling
        .clone();

    // The injection *does* name a live sentinel — but the destination is
    // outside the binding's declared scope: refused, durably denied.
    let outcome = broker
        .mediate_sentinel(
            &mut store,
            &run_id,
            &lease,
            &sentinel,
            &req("eff-exfil", "collector.evil.example.com", None),
        )
        .unwrap();
    match outcome {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::OutOfScope),
        MediationOutcome::Staged(_) => panic!("out-of-scope destination must never stage"),
    }
    // And on a foreign env handle — a copied sentinel is non-transferable
    // (ADR-0266 D3): out_of_scope again.
    let mut foreign = req("eff-exfil-2", "api.github.com", None);
    foreign.env_handle = "env-OTHER".into();
    let outcome = broker
        .mediate_sentinel(&mut store, &run_id, &lease, &sentinel, &foreign)
        .unwrap();
    match outcome {
        MediationOutcome::Refused(r) => assert_eq!(r.code, RefusedCode::OutOfScope),
        MediationOutcome::Staged(_) => panic!("a foreign-env sentinel must never stage"),
    }
    let denied = events_of_class(&store, &run_id, "security.credential.denied");
    assert_eq!(denied.len(), 2);
}

#[test]
fn lt03_canary_presented_lands_leak_detected() {
    let (mut store, run_id, lease) = open("lt03-canary");
    let canary_v = "ghp_canary_tripwire_0123456789abcdef";
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

    // A canary channel refuses `bind` — the attempt itself is the signal:
    // `leak_detected` is durable before the refusal is visible.
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
    // The veto's input shape: location + detector + hit — never the value.
    let s = leaks[0].to_canonical_string();
    assert!(!s.contains(canary_v));
    assert!(leaks[0].get("location").is_some());
    assert_eq!(
        leaks[0].get("detector").and_then(Json::as_str),
        Some("canary")
    );
    assert!(leaks[0].get("hit").is_some());
}

#[test]
fn lt03_real_value_at_a_sink_is_a_leak() {
    // AC-R-2.8.3-3's sink half: a *real* value reaching a sink surface is a
    // `leak_detected` fact — the `secret_leak` veto's durable input. The
    // scan is the detector; the row it produces is the veto's evidence.
    let (mut store, run_id, lease) = open("lt03-sink");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );

    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    // A sink delivery carrying the raw value is a known_value leak.
    let sink_text = format!("POST body: Authorization: Bearer {secret}");
    let hits = leak_scan(
        &[(
            ScanTarget::SinkDelivery("model_io".into()),
            sink_text.as_str(),
        )],
        &detectors,
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].detector, DetectorKind::KnownValue);
    assert_eq!(hits[0].location_string(), "sink_delivery:model_io");
    // The durable row shape the veto fold reads (§5g.3 §4).
    let row = events::leak_detected_payload(&hits[0]);
    assert_eq!(
        row.get("detector").and_then(Json::as_str),
        Some("known_value")
    );
    assert!(row.get("hit").and_then(|h| h.get("fingerprint")).is_some());
    assert!(!row.to_canonical_string().contains(secret));
}

// ─────────────────────────────────────────────────────────────────────────────
// LT-11 (AC-R-2.8.3-11): the model request-view scan is empty across the
// suite — the wire the mediator emits carries placeholders, never values.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn lt11_request_view_scan_is_empty() {
    let (mut store, run_id, lease) = open("lt11");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .expect("placeholder")
        .spelling
        .clone();

    // The staged request views the mediator emits — headers carry the
    // *placeholder spelling*, never the value. Every view in the "suite"
    // scans empty.
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    let views = [
        format!("GET /repos HTTP/1.1\nHost: api.github.com\nAuthorization: Bearer {sentinel}"),
        format!("GET /user HTTP/1.1\nHost: api.github.com\nAuthorization: Bearer {sentinel}"),
        format!("GET /rate_limit HTTP/1.1\nHost: api.github.com\nX-Api-Key: {sentinel}"),
    ];
    for (i, v) in views.iter().enumerate() {
        let leaks = leak_scan(&[(ScanTarget::RequestView, v.as_str())], &detectors);
        // A placeholder passthrough mark is the safe form — not a leak.
        assert!(
            leaks.is_empty(),
            "request view {i} must scan empty: {leaks:?}"
        );
    }

    // And the run's committed event payloads scan empty too — the battery
    // sweep (ADR-0059 D2) over every durable row finds no value.
    let run_leaks = broker.leak_scan_run(&store, &run_id, &detectors).unwrap();
    assert!(run_leaks.is_empty(), "run scan: {run_leaks:?}");
    // The run's bytes never carried the value anywhere.
    for e in store.events(&run_id).unwrap() {
        assert!(!e.payload.to_canonical_string().contains(secret));
    }
}

#[test]
fn lt11_planted_value_in_request_view_is_caught() {
    let (mut store, run_id, lease) = open("lt11-plant");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    let bad_view = format!("GET / HTTP/1.1\nAuthorization: Bearer {secret}");
    let leaks = leak_scan(&[(ScanTarget::RequestView, bad_view.as_str())], &detectors);
    assert_eq!(leaks.len(), 1);
    assert_eq!(leaks[0].location_string(), "request_view");
    assert_eq!(leaks[0].detector, DetectorKind::KnownValue);
}

// ─────────────────────────────────────────────────────────────────────────────
// LT-12 (AC-R-2.8.3-12): encoded forms of a known value are detected, and
// the miss rate is reported on the seeded corpus.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn lt12_encoded_forms_are_detected() {
    let (mut store, run_id, lease) = open("lt12");
    let secret = "ghp_real_token_value_0123456789abcdef";
    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    bound_binding(
        &mut broker,
        &mut store,
        &run_id,
        &lease,
        "hnd-1",
        "eff-bind",
    );
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());

    // Every canonical encoding of the value is caught by the encoded
    // detector — the verbatim form is caught by `known_value`.
    let forms = encoded::encoded_forms(secret);
    assert_eq!(
        forms.len(),
        5,
        "base64-std/base64-url/hex-lo/hex-hi/percent"
    );
    for form in &forms {
        let text = format!("exfil payload: {form}");
        let hits = detect(&text, &detectors);
        assert!(
            hits.iter()
                .any(|h| h.detector == DetectorKind::KnownValueEncoded),
            "encoded form not detected: {form:?} → {hits:?}"
        );
        // And the leak-scan verdict over a sink target reports it.
        let leaks = leak_scan(
            &[(
                ScanTarget::SinkDelivery("tool_output".into()),
                text.as_str(),
            )],
            &detectors,
        );
        assert_eq!(leaks.len(), 1);
        assert_eq!(leaks[0].detector, DetectorKind::KnownValueEncoded);
    }
    // A short value produces no encoded forms (false-positive discipline —
    // ENCODED_MIN_LEN), and the verbatim detector still covers it.
    let short = "a1b2c3d";
    assert!(encoded::encoded_forms(short).is_empty());
}

#[test]
fn lt12_miss_rate_is_reported_on_the_seeded_corpus() {
    let secret = "ghp_real_token_value_0123456789abcdef";
    let forms = encoded::encoded_forms(secret);
    // A seeded corpus: verbatim + every encoded form, plus a benign item.
    let mut corpus: Vec<encoded::CorpusItem> = vec![encoded::CorpusItem {
        text: format!("the token {secret} appeared in output"),
        planted: 1,
    }];
    for f in &forms {
        corpus.push(encoded::CorpusItem {
            text: format!("encoded: {f}"),
            planted: 1,
        });
    }
    corpus.push(encoded::CorpusItem {
        text: "nothing here".into(),
        planted: 0,
    });

    let mut broker = broker_with(gh_source(), secret);
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());
    let report = encoded::miss_rate(&corpus, |text| detect(text, &detectors).len());
    assert_eq!(report.planted, 6);
    assert_eq!(report.misses, 0, "the seeded corpus has no misses");
    assert_eq!(report.miss_rate_ppm, Some(0));
    // An empty corpus reports n/a, never 0 (estimator_undefined).
    let empty = encoded::miss_rate(&[], |_| 0);
    assert_eq!(empty.miss_rate_ppm, None);
}

// ── The Stage-3 metric registrations (§5g.3 §4; AC-R-2.8.3-3/-11/-12) ────

#[test]
fn stage3_secret_metrics_are_registered() {
    let decls = registered_metrics();
    let find = |name: &str| decls.iter().find(|d| d.name == name);
    // The veto leg — `veto: true`, ledger-observable, mediated-egress.
    let sl = find(metrics::SECRET_LEAK).expect("secret_leak registered");
    assert!(sl.veto, "secret_leak is a veto metric");
    // The observability legs and the corpus miss rate.
    assert!(find(metrics::SECRET_LEAK_MODEL_IO).is_some());
    assert!(find(metrics::SECRET_LEAK_END_STATE).is_some());
    let mr = find(metrics::SECRET_DETECTOR_MISS_RATE).expect("miss rate registered");
    assert!(!mr.veto, "miss rate reports; it never vetoes");
}
