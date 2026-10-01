//! S5.3 — Adapters B and C (§6.6 §5; R-2.10.6 C2; ADR-0164/0166):
//!
//! - **B** (`model_boundary_intercept`): the proxy rides inside the
//!   environment — intercepted model rows lift `mediated`/`proxy`/
//!   `intercept`; hooks lift `observed`/`hook`/`participant`; a
//!   hook-reported call the proxy missed reconstructs from the
//!   transcript as `unobserved`/`log`/`adapter` (never `mediated`,
//!   never dropped); `set_coordinate{model}` lands on the proxy
//!   override; the generation-parameter policy is a declared adapter
//!   parameter on the record.
//! - **C** (`container_installed`): `install`/`run`/`kill`/`snapshot`
//!   verbs; `end_state`-only observability; per-format lifters at
//!   close — every reconstructed row `unobserved`/`log`/`adapter`;
//!   unmapped log kinds land on `native_record` (I-3 — never dropped);
//!   a session without a terminal row gets a synthesized one.

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_hir::records::{AssumptionDebtRecord, EvidenceRef, ExpiryCondition, OwnerRef};
use hh_hosting::{
    adapter_b_record, adapter_c_record, AdapterB, AdapterC, ContainerDriver, EndStateSnapshot,
    EventChannel, GenParamPolicy, HostedOrigin, InterceptTransport, LifterTable, LogLifter,
    Mediation, ADAPTER_B_ID, ADAPTER_C_ID,
};
use hh_ontology::debt::{DebtStatus, ExpiryKind};
use hh_ontology::participant::HostingMechanism;
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_wire::Json;

fn debt(id: &str) -> AssumptionDebtRecord {
    let provenance = ProvenanceRecord::kernel("hh-hosting/test", 0);
    AssumptionDebtRecord {
        rule_id: id.into(),
        hypothesis: Text::new(
            "the declared generation-parameter policy governs the proxy",
            id,
            provenance.clone(),
        ),
        evidence_refs: vec![EvidenceRef::legacy("s5_3")],
        owner: OwnerRef::principal(id),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ProbeFailure,
            value: Some("P0 dimension drift".into()),
        },
        removal_test_ref: "tests/s5_3.rs".into(),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: None,
        created_by: Some(provenance),
        created_at: Some(0),
        supersedes: None,
    }
}

// ── Adapter B ────────────────────────────────────────────────────────────────

struct MockIntercept {
    hooks: Vec<Json>,
    proxy: Vec<Json>,
    transcript: Vec<Json>,
    override_model: Option<String>,
}

impl InterceptTransport for MockIntercept {
    fn drain_hooks(&mut self) -> Vec<Json> {
        std::mem::take(&mut self.hooks)
    }
    fn drain_model_observations(&mut self) -> Vec<Json> {
        std::mem::take(&mut self.proxy)
    }
    fn set_model_override(&mut self, model_ref: &str) -> Result<(), String> {
        self.override_model = Some(model_ref.to_string());
        Ok(())
    }
    fn transcript_rows(&mut self, _from: u64, _to: u64) -> Vec<Json> {
        self.transcript.clone()
    }
}

fn policy() -> GenParamPolicy {
    GenParamPolicy {
        forward: vec!["temperature".into(), "max_tokens".into()],
        default: [("top_p".to_string(), Json::Int(1_000_000))]
            .into_iter()
            .collect(),
    }
}

#[test]
fn adapter_b_lifts_proxy_hooks_and_transcript_fallback() {
    let transport = MockIntercept {
        hooks: vec![
            Json::obj([
                ("hook", Json::str("pre_model")),
                ("payload", Json::obj([("model_call_id", Json::str("mc-1"))])),
            ]),
            Json::obj([
                ("hook", Json::str("post_model")),
                ("payload", Json::obj([("model_call_id", Json::str("mc-2"))])),
            ]),
        ],
        // The proxy intercepted mc-1 only — mc-2 is the fallback's.
        proxy: vec![Json::obj([
            ("kind", Json::str("model.call.completed")),
            ("model_call_id", Json::str("mc-1")),
            ("usage", Json::obj([("total", Json::Int(40))])),
        ])],
        transcript: vec![
            Json::obj([
                ("kind", Json::str("model.call.completed")),
                ("model_call_id", Json::str("mc-1")),
                ("note", Json::str("proxied — never re-lifted")),
            ]),
            Json::obj([
                ("kind", Json::str("model.call.completed")),
                ("model_call_id", Json::str("mc-2")),
                ("note", Json::str("the proxy missed this one")),
            ]),
        ],
        override_model: None,
    };
    let mut adapter = AdapterB::new(
        Box::new(transport),
        adapter_b_record("adapter-b/1", &policy(), debt("hh.hosting.adapter_b")),
        policy(),
    );

    // The proxy lift — `intercept` origin, `mediated` on the proxy
    // channel (the kernel-gated channel the stamp is lawful on).
    let proxied = adapter.lift_proxy();
    assert_eq!(proxied.len(), 1);
    assert_eq!(proxied[0].kind, "model.call.completed");
    assert_eq!(proxied[0].mediation, Mediation::Mediated);
    assert_eq!(proxied[0].event_channel, EventChannel::Proxy);
    assert_eq!(proxied[0].origin, HostedOrigin::Intercept);
    assert_eq!(proxied[0].authority, AuthorityClass::Environment);
    assert!(proxied[0].raw_ref.is_some());

    // The hook lift — `observed` participant rows on the `hook` channel.
    let hooks = adapter.lift_hooks();
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0].kind, "hook.pre_model");
    assert_eq!(hooks[1].kind, "hook.post_model");
    assert!(hooks.iter().all(|h| {
        h.mediation == Mediation::Observed
            && h.event_channel == EventChannel::Hook
            && h.origin == HostedOrigin::Participant
    }));

    // The transcript fallback — mc-2 reconstructs as `unobserved`/`log`;
    // mc-1 was proxied (one channel of record per fact) and never
    // re-lifts.
    let reconstructed = adapter.reconstruct_transcript(2);
    assert_eq!(reconstructed.len(), 1);
    let r = &reconstructed[0];
    assert_eq!(r.mediation, Mediation::Unobserved);
    assert_eq!(r.event_channel, EventChannel::Log);
    assert_eq!(r.origin, HostedOrigin::Adapter);
    assert_eq!(r.authority, AuthorityClass::Unverified);
    assert_eq!(
        r.payload.get("model_call_id").and_then(Json::as_str),
        Some("mc-2")
    );
    assert!(r
        .payload
        .get("reconstructed_from")
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("transcript"));
}

#[test]
fn adapter_b_model_coordinate_by_proxy_override_and_gen_policy() {
    let transport = MockIntercept {
        hooks: vec![],
        proxy: vec![],
        transcript: vec![],
        override_model: None,
    };
    let policy = policy();
    let record = adapter_b_record("adapter-b/1", &policy, debt("hh.hosting.adapter_b"));
    // The declared generation-parameter policy rides the record.
    assert_eq!(
        record
            .ext
            .get("gen_param_policy")
            .and_then(|p| p.get("forward"))
            .and_then(|f| match f {
                Json::Arr(v) => Some(v.len()),
                _ => None,
            }),
        Some(2)
    );
    assert_eq!(record.adapter_id, ADAPTER_B_ID);
    assert_eq!(
        record.hosting_mechanism,
        HostingMechanism::ModelBoundaryIntercept
    );
    assert_eq!(
        record
            .declaration_defaults
            .get("coordinate_model")
            .and_then(Json::as_str),
        Some("supported")
    );

    let mut adapter = AdapterB::new(Box::new(transport), record, policy.clone());
    // `set_coordinate{model}` — the proxy override path.
    let obs = adapter
        .set_model("idp:model:other")
        .expect("override lands");
    assert_eq!(obs.kind, "lifecycle.hosted.coordinate_set");
    assert_eq!(
        obs.payload.get("via").and_then(Json::as_str),
        Some("proxy_override")
    );

    // The policy filters requested parameters — forward-listed pass,
    // others strip (listed, never silently applied), defaults pin.
    let applied = policy.apply(&Json::obj([
        ("temperature", Json::Int(500_000)),
        ("seed", Json::Int(42)),
    ]));
    assert_eq!(
        applied.get("forwarded").and_then(|f| f.get("temperature")),
        Some(&Json::Int(500_000))
    );
    assert_eq!(
        applied.get("stripped"),
        Some(&Json::Arr(vec![Json::str("seed")]))
    );
    assert_eq!(
        applied.get("defaulted"),
        Some(&Json::Arr(vec![Json::str("top_p")]))
    );
    assert_eq!(
        applied.get("forwarded").and_then(|f| f.get("top_p")),
        Some(&Json::Int(1_000_000))
    );
}

// ── Adapter C ────────────────────────────────────────────────────────────────

struct MockContainer {
    installed: Vec<String>,
    running: Vec<String>,
    killed: Vec<String>,
    logs: Vec<Json>,
    snapshot: EndStateSnapshot,
}

impl ContainerDriver for MockContainer {
    fn install(&mut self, image_ref: &str) -> Result<String, String> {
        let id = format!("ctr-{}", self.installed.len());
        self.installed.push(format!("{id}@{image_ref}"));
        Ok(id)
    }
    fn run(&mut self, container: &str, _spec: &Json) -> Result<(), String> {
        self.running.push(container.to_string());
        Ok(())
    }
    fn kill(&mut self, container: &str) -> Result<(), String> {
        self.killed.push(container.to_string());
        Ok(())
    }
    fn snapshot(&mut self, _container: &str) -> Result<EndStateSnapshot, String> {
        Ok(self.snapshot.clone())
    }
    fn log_rows(&mut self, _container: &str, _from: u64) -> Vec<Json> {
        self.logs.clone()
    }
}

fn lifters() -> LifterTable {
    let mut formats = BTreeMap::new();
    formats.insert(
        "acme_jsonl".to_string(),
        LogLifter {
            format: "acme_jsonl".into(),
            kind_map: [
                ("turn".to_string(), "lifecycle.turn.finished".to_string()),
                ("call".to_string(), "model.call.completed".to_string()),
                ("done".to_string(), "session.finished".to_string()),
            ]
            .into_iter()
            .collect(),
        },
    );
    LifterTable { formats }
}

#[test]
fn adapter_c_lifecycle_end_state_and_lifters() {
    let driver = MockContainer {
        installed: vec![],
        running: vec![],
        killed: vec![],
        logs: vec![
            Json::obj([("seq", Json::Int(1)), ("type", Json::str("turn"))]),
            Json::obj([("seq", Json::Int(2)), ("type", Json::str("call"))]),
            // An unmapped kind → `native_record` (I-3).
            Json::obj([("seq", Json::Int(3)), ("type", Json::str("exotic"))]),
            Json::obj([("seq", Json::Int(4)), ("type", Json::str("done"))]),
        ],
        snapshot: EndStateSnapshot {
            exit_code: Some(0),
            artifacts: vec!["out.patch".into()],
            outcome: Some(Json::obj([("status", Json::str("ok"))])),
            raw_ref: Some("snap:1".into()),
        },
    };
    let record = adapter_c_record("adapter-c/1", &lifters(), debt("hh.hosting.adapter_c"));
    assert_eq!(record.adapter_id, ADAPTER_C_ID);
    assert_eq!(
        record.hosting_mechanism,
        HostingMechanism::ContainerInstalled
    );
    // `end_state`-only observability — the record claims exactly that.
    assert_eq!(
        record
            .declaration_defaults
            .get("end_state")
            .and_then(Json::as_str),
        Some("supported")
    );
    assert_eq!(
        record
            .declaration_defaults
            .get("streaming")
            .and_then(Json::as_str),
        Some("unsupported")
    );
    // The lifter table rides `ext.lifters`.
    match record.ext.get("lifters") {
        Some(Json::Arr(v)) => assert_eq!(v, &vec![Json::str("acme_jsonl")]),
        other => panic!("ext.lifters: {other:?}"),
    }

    let mut adapter = AdapterC::new(Box::new(driver), record, lifters(), "acme_jsonl");
    let install = adapter.install("sha256:img").expect("install");
    assert_eq!(install.kind, "lifecycle.hosted.attached");
    assert_eq!(install.event_channel, EventChannel::Handle);

    let run = adapter
        .run(&Json::obj([("cmd", Json::str("solve"))]))
        .expect("run");
    assert_eq!(run.kind, "lifecycle.hosted.run_started");

    let (snap, snap_obs) = adapter.snapshot().expect("snapshot");
    assert_eq!(snap.exit_code, Some(0));
    assert_eq!(snap_obs.kind, "session.finished");
    assert_eq!(snap_obs.mediation, Mediation::Observed);

    // `close` — the reconstruction: every lifted row is
    // `unobserved`/`log`/`adapter` (the Lab scraped a log, it observed
    // no channel); the mapped kinds lift through the `acme_jsonl`
    // lifter; `exotic` falls to `native_record`; the `done` row's
    // terminal means no synthesized one is appended.
    let lifted = adapter.close_and_reconstruct().expect("close");
    let kinds: Vec<&str> = lifted.iter().map(|o| o.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "lifecycle.turn.finished",
            "model.call.completed",
            "lifecycle.hosted.native_record",
            "session.finished",
        ]
    );
    for o in &lifted {
        assert_eq!(o.mediation, Mediation::Unobserved);
        assert_eq!(o.event_channel, EventChannel::Log);
        assert_eq!(o.origin, HostedOrigin::Adapter);
        assert!(o.raw_ref.is_some());
        // The source format + native kind ride the payload.
        assert_eq!(
            o.payload.get("source_format").and_then(Json::as_str),
            Some("acme_jsonl")
        );
    }
    let kill = adapter.kill().expect("kill");
    assert_eq!(kill.kind, "lifecycle.hosted.detached");
}

#[test]
fn adapter_c_synthesizes_terminal_when_none_lifted() {
    let driver = MockContainer {
        installed: vec![],
        running: vec![],
        killed: vec![],
        logs: vec![Json::obj([
            ("seq", Json::Int(1)),
            ("type", Json::str("turn")),
        ])],
        snapshot: EndStateSnapshot {
            exit_code: Some(137),
            artifacts: vec![],
            outcome: None,
            raw_ref: None,
        },
    };
    let mut adapter = AdapterC::new(
        Box::new(driver),
        adapter_c_record("adapter-c/1", &lifters(), debt("hh.hosting.adapter_c")),
        lifters(),
        "acme_jsonl",
    );
    adapter.install("sha256:img").unwrap();
    adapter.run(&Json::obj([])).unwrap();
    let lifted = adapter.close_and_reconstruct().expect("close");
    // One lifted row + the synthesized terminal (unobserved, adapter).
    assert_eq!(lifted.len(), 2);
    let terminal = lifted.last().unwrap();
    assert_eq!(terminal.kind, "session.finished");
    assert_eq!(terminal.payload.get("synthesized"), Some(&Json::Bool(true)));
    assert_eq!(terminal.mediation, Mediation::Unobserved);
    assert_eq!(terminal.authority, AuthorityClass::Unverified);
}
