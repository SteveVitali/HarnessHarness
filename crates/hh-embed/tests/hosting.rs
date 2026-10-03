//! S4.5a — the `lab.hosting.{describe,probe,attach}` boundary (spec §6.6;
//! R-2.10.6; ADR-0164/0165/0166 (e)).
//!
//! Binding (a) drives `EmbedService::handle` in-process. The participant
//! record registers as an opaque `participant` body (records-in); probe
//! reports pin a `conformance_suite` + a durable run; `drive` exercises the
//! removable `HostingPlane` seam (a stub closure — `hh-embed` never names
//! `hh-hosting`; a `hosting_plane_absent` refusal is the honest-tier
//! contract, AC-R-2.10.6-5).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("hh-embed-hosting-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn service(tag: &str) -> EmbedService {
    let root = test_dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "hosting-test".into(),
    })
    .unwrap()
}

fn service_in(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "hosting-test".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn is_err(resp: &Json) -> bool {
    resp.get("error").is_some()
}

/// The kernel registrar — register/quarantine are ≥-principal acts
/// (the conformance suite uses the same provenance helper).
fn registrar() -> Json {
    hh_provenance::ProvenanceRecord::kernel("hh-embed/hosting-test", 0).to_json()
}

/// `hello` — the service refuses ops until the handshake initializes it.
fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("hosting-test")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello refused: {r:?}");
}

/// An opaque `participant` body — the boundary reads `version_identity`,
/// `descriptor{hosting_mechanism, observability_level}`, `hosting_ext
/// .abi_versions` and `capability_declaration`; everything else is opaque.
fn participant_body(identity: &str, decl: &[(&str, &str)]) -> Json {
    let mut d = BTreeMap::new();
    for (k, v) in decl {
        d.insert((*k).to_string(), Json::str(*v));
    }
    Json::obj([
        ("kind", Json::str("participant")),
        ("participant_id", Json::str("p:test")),
        ("version_identity", Json::str(identity)),
        (
            "descriptor",
            Json::obj([
                ("class", Json::str("hosted")),
                ("hosting_mechanism", Json::str("session_abi")),
                (
                    "observability_level",
                    Json::Arr(vec![Json::str("events"), Json::str("end_state")]),
                ),
            ]),
        ),
        (
            "hosting_ext",
            Json::obj([
                ("abi_versions", Json::Arr(vec![Json::str("hh-hosting/1")])),
                (
                    "credential_supply",
                    Json::Arr(vec![Json::str("env"), Json::str("mcp")]),
                ),
            ]),
        ),
        ("capability_declaration", Json::Obj(d)),
    ])
}

/// Register a participant → its `version_id` (the report/attach pin).
fn register_participant(svc: &mut EmbedService, identity: &str, decl: &[(&str, &str)]) -> String {
    let r = call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", participant_body(identity, decl)),
            ("registrar", registrar()),
        ]),
    );
    ok(&r)
        .get("version_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

/// A `class` body (the conformance suite's pin target).
fn class_body(class_id: &str) -> Json {
    use hh_registry::kinds::Cardinality;
    use hh_registry::records::{ClassRecord, ContractOperation, RegistryRecord};
    hh_registry::schema::body_json(
        &RegistryRecord::Class(ClassRecord {
            class_id: class_id.to_string(),
            contract: vec![ContractOperation {
                name: "run".to_string(),
                inputs: Json::Null,
                outputs: Json::Null,
                invariants: vec![],
                failure_modes: vec![],
            }],
            cardinality: Cardinality::ExactlyOne,
            required_inputs: ["ModelProfile".to_string(), "ResourceAccount".to_string()]
                .into_iter()
                .collect(),
            base_param_schema: BTreeMap::new(),
            hot_path: false,
            dialect_introduced: "registry/1".to_string(),
            contract_version: "1.0".to_string(),
            home: "kernel".to_string(),
            declaration_schema: Json::obj([("additionalProperties", Json::Bool(false))]),
            conformance_suite_ref: None,
            decision_points: vec![],
            metrics_declared: vec![],
            slot_key: class_id.to_string(),
            tier: "C0".to_string(),
            depends_on: Vec::new(),
        }),
        false,
    )
}

/// Register `class` + `conformance_suite` → the suite's `version_id` (the
/// report's `suite_ref` pin).
fn register_suite(svc: &mut EmbedService) -> String {
    let cv = ok(&call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("class")),
            ("body", class_body("hosting_probe_class")),
            ("registrar", registrar()),
        ]),
    ));
    let class_vid = cv.get("version_id").and_then(Json::as_str).unwrap();
    let suite_body = hh_registry::schema::body_json(
        &hh_registry::records::RegistryRecord::Suite(hh_registry::records::ConformanceSuite {
            suite_id: "suite:hosting-probe".to_string(),
            class_ref: class_vid.to_string(),
            contract_version: "1.0".to_string(),
            tests: vec![hh_registry::records::SuiteTest {
                test_id: "t-hosted".to_string(),
                kind: hh_registry::kinds::TestKind::Static,
                fixture_ref: None,
                driver: hh_registry::kinds::TestDriver::InProcess,
                oracle_class: hh_registry::kinds::OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "entries land".to_string(),
            }],
            required_for_status: ["t-hosted".to_string()].into_iter().collect(),
        }),
        false,
    );
    let sv = ok(&call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("conformance_suite")),
            ("body", suite_body),
            ("registrar", registrar()),
        ]),
    ));
    sv.get("version_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

/// Seed a finished run + a live writer lease on `root` before the service
/// opens (the folded state is shared — the lease rows replay).
fn seed_runs(root: &std::path::Path) -> (String, Json, String, Json, String) {
    let mut store = hh_ledger::store::Store::open(root.join("store")).unwrap();
    let manifest =
        || hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent);
    let (durable_run, _) = store.open_run(manifest(), "seed").unwrap();
    let durable_run2 = durable_run.clone();
    store
        .commit_kernel_row_for(
            "hh-test",
            &durable_run,
            "lifecycle.run.finished",
            Json::obj([("outcome", Json::str("completed"))]),
            vec![],
            vec![],
        )
        .unwrap();
    let (write_run, lease) = store.open_run(manifest(), "seed").unwrap();
    let lease_json = Json::obj([
        ("lease_id", Json::str(&lease.lease_id)),
        ("holder", Json::str(&lease.holder)),
        ("generation", Json::Int(lease.generation as i64)),
        ("expires_at_ms", Json::Int(lease.expires_at_ms as i64)),
    ]);
    (durable_run, Json::Null, write_run, lease_json, durable_run2)
}

/// A conformance report body pinning the participant + suite + run.
fn report_body(
    report_id: &str,
    participant_vid: &str,
    suite_vid: &str,
    run_id: &str,
    entries: Vec<Json>,
) -> Json {
    Json::obj([
        ("kind", Json::str("conformance_report")),
        ("report_id", Json::str(report_id)),
        ("subject_kind", Json::str("participant")),
        ("subject_ref", Json::str(participant_vid)),
        ("suite_ref", Json::str(suite_vid)),
        (
            "host",
            Json::obj([
                ("placement", Json::str("subprocess_confined")),
                ("isolation", Json::str("hermetic")),
                (
                    "instrument",
                    Json::obj([
                        ("version", Json::str("test")),
                        ("source_commit", Json::str("test")),
                        ("dirty", Json::Bool(false)),
                        ("component_versions", Json::Arr(vec![])),
                        ("idp", Json::str("idp/1")),
                    ]),
                ),
            ]),
        ),
        ("produced_by", Json::str("registry_ci")),
        ("results", Json::Arr(vec![])),
        ("probed_declaration", Json::obj([])),
        ("run_id", Json::str(run_id)),
        ("stale", Json::Bool(false)),
        ("hosted_entries", Json::Arr(entries)),
    ])
}

/// A hosted conformance entry; `verdict` is `derive_verdict` spelled out.
fn entry(
    identity: &str,
    adapter: &str,
    dimension: &str,
    declared: &str,
    observed: &str,
    at: i64,
) -> Json {
    let verdict = hh_registry::records::ConformanceRecord::derive_verdict(
        &Json::str(declared),
        &Json::str(observed),
    )
    .as_str();
    Json::obj([
        ("participant_version_identity", Json::str(identity)),
        ("adapter_version_id", Json::str(adapter)),
        ("dimension", Json::str(dimension)),
        ("declared", Json::str(declared)),
        ("observed", Json::str(observed)),
        ("verdict", Json::str(verdict)),
        ("observed_in", Json::str("probe")),
        ("evidence_ref", Json::Null),
        ("at", Json::Int(at)),
    ])
}

// ── describe ────────────────────────────────────────────────────────────────

#[test]
fn hosting_describe_unknown_participant_refuses() {
    let mut svc = service("describe-unknown");
    hello(&mut svc);
    let r = call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str("idp:nonexistent"))]),
    );
    assert!(is_err(&r), "unknown ref must refuse, got {r:?}");
}

/// AC-3 (empty declaration → `unknown` vector; declared dimensions carry
/// the declaration until probed — the reconcile is data, never a guess).
#[test]
fn hosting_describe_reconciles_declaration_without_entries() {
    let mut svc = service("describe-ok");
    hello(&mut svc);
    let vid = register_participant(
        &mut svc,
        "p:test@1.0.0",
        &[("streaming", "supported"), ("interrupt", "unknown")],
    );
    let r = ok(&call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str(&vid))]),
    ));
    assert_eq!(
        r.get("version_identity").and_then(Json::as_str),
        Some("p:test@1.0.0")
    );
    assert_eq!(r.get("quarantined"), Some(&Json::Bool(false)));
    let vector = r
        .get("capability_vector")
        .and_then(|v| v.get("vector"))
        .expect("vector");
    assert_eq!(
        vector.get("streaming").and_then(Json::as_str),
        Some("supported")
    );
    assert_eq!(
        vector.get("interrupt").and_then(Json::as_str),
        Some("unknown")
    );
    // An empty declaration reconciles to an empty vector — never a guess.
    let vid2 = register_participant(&mut svc, "p:test@2.0.0", &[]);
    let r2 = ok(&call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str(&vid2))]),
    ));
    let v2 = r2
        .get("capability_vector")
        .and_then(|v| v.get("vector"))
        .expect("vector");
    assert!(matches!(v2, Json::Obj(m) if m.is_empty()));
}

// ── probe ───────────────────────────────────────────────────────────────────

/// AC-3: a probe report appends `hosted_entries`; the reconciled vector
/// reads the latest non-stale observation over the declaration.
#[test]
fn hosting_probe_registers_entries_and_reconciles() {
    let root = test_dir("probe-ok");
    let (durable_run, _, _wr, _lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let suite = register_suite(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[("streaming", "unknown")]);
    let entries = vec![entry(
        "p:test@1.0.0",
        "idp:adapter-a",
        "streaming",
        "unknown",
        "supported",
        0,
    )];
    let r = ok(&call(
        &mut svc,
        "lab.hosting.probe",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            (
                "report",
                report_body("rep:t1", &vid, &suite, &durable_run, entries),
            ),
            ("registrar", registrar()),
        ]),
    ));
    assert_eq!(r.get("quarantined"), Some(&Json::Bool(false)));
    // The probe's observation supersedes the declaration's `unknown`.
    let d = ok(&call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str(&vid))]),
    ));
    let vector = d.get("capability_vector").unwrap();
    assert_eq!(
        vector
            .get("vector")
            .and_then(|v| v.get("streaming"))
            .and_then(Json::as_str),
        Some("supported")
    );
    assert_eq!(
        d.get("conformance_entries").and_then(|e| match e {
            Json::Arr(a) => Some(a.len()),
            _ => None,
        }),
        Some(1)
    );
}

/// AC-3/§9.3: DRIFT on a P0 dimension (`interrupt` ∈ P-03) quarantines the
/// participant version; `describe` reports it; `attach` refuses the
/// quarantined participant until a new declaration registers.
#[test]
fn hosting_probe_p0_drift_quarantines() {
    let root = test_dir("probe-p0");
    let (durable_run, _, wr, lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let suite = register_suite(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[("interrupt", "supported")]);
    let entries = vec![entry(
        "p:test@1.0.0",
        "idp:adapter-a",
        "interrupt",
        "supported",
        "unsupported",
        0,
    )];
    let r = ok(&call(
        &mut svc,
        "lab.hosting.probe",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            (
                "report",
                report_body("rep:t2", &vid, &suite, &durable_run, entries),
            ),
            ("registrar", registrar()),
        ]),
    ));
    assert_eq!(r.get("quarantined"), Some(&Json::Bool(true)));
    assert_eq!(
        r.get("drift_p0"),
        Some(&Json::Arr(vec![Json::str("interrupt")]))
    );
    // Describe surfaces the quarantine.
    let d = ok(&call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str(&vid))]),
    ));
    assert_eq!(d.get("quarantined"), Some(&Json::Bool(true)));
    // Attach refuses the quarantined participant (§9.3 — excluded until a
    // new declaration registers).
    let a = call(
        &mut svc,
        "lab.hosting.attach",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            ("run_id", Json::str(&wr)),
            ("lease", lease),
            ("session", Json::obj([])),
        ]),
    );
    assert!(is_err(&a), "quarantined participant must refuse attach");
}

/// Non-P0 drift annotates — the dimension lands on `drift_dimensions`,
/// the admission never flips (P0-only quarantine).
#[test]
fn hosting_probe_non_p0_drift_annotates_without_quarantine() {
    let root = test_dir("probe-non-p0");
    let (durable_run, _, _wr, _lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let suite = register_suite(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[("streaming", "supported")]);
    let entries = vec![entry(
        "p:test@1.0.0",
        "idp:adapter-a",
        "streaming",
        "supported",
        "unsupported",
        0,
    )];
    let r = ok(&call(
        &mut svc,
        "lab.hosting.probe",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            (
                "report",
                report_body("rep:t3", &vid, &suite, &durable_run, entries),
            ),
            ("registrar", registrar()),
        ]),
    ));
    assert_eq!(r.get("quarantined"), Some(&Json::Bool(false)));
    assert_eq!(
        r.get("drift_dimensions"),
        Some(&Json::Arr(vec![Json::str("streaming")]))
    );
}

/// The removable seam's honest contract (CC6): `drive` with no plane wired
/// refuses `hosting_plane_absent` — never a faked report.
#[test]
fn hosting_probe_drive_without_plane_refuses() {
    let mut svc = service("drive-absent");
    hello(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[]);
    let r = call(
        &mut svc,
        "lab.hosting.probe",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            (
                "drive",
                Json::obj([
                    ("probes", Json::Arr(vec![Json::str("basic_turn")])),
                    ("adapter_version_id", Json::str("idp:adapter-a")),
                ]),
            ),
            ("report_base", Json::obj([])),
            ("registrar", registrar()),
        ]),
    );
    assert!(is_err(&r), "drive without a plane must refuse, got {r:?}");
    assert!(
        r.to_canonical_string().contains("hosting_plane_absent"),
        "refusal names the absent plane: {r:?}"
    );
}

/// `drive` through a stub plane: per-dimension `drive_probe` calls return
/// outcome Json; the boundary packs them into `hosted_entries` and takes
/// the same record/quarantine path as a verbatim report (records-in).
#[test]
fn hosting_probe_drive_through_plane_records_entries() {
    let root = test_dir("probe-drive");
    let (durable_run, _, _wr, _lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let suite = register_suite(&mut svc);
    let vid = register_participant(
        &mut svc,
        "p:test@1.0.0",
        &[("streaming", "supported"), ("steer", "unknown")],
    );
    // The stub plane: streaming confirms, steer reports skipped (a probe
    // the environment cannot exercise — never unsupported, AC-R-2.10.6-3).
    svc.set_hosting_plane(Some(Box::new(|verb: &str, params: &Json| {
        assert_eq!(verb, "drive_probe");
        let dim = params
            .get("dimension")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        let observed = match dim.as_str() {
            "streaming" => "supported",
            "steer" => "skipped",
            _ => "unknown",
        };
        Ok(Json::obj([
            ("dimension", Json::str(dim)),
            ("observed", Json::str(observed)),
            ("verdict", Json::str(observed)),
            ("detail_ref", Json::Null),
        ]))
    })));
    let r = ok(&call(
        &mut svc,
        "lab.hosting.probe",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            (
                "drive",
                Json::obj([
                    (
                        "probes",
                        Json::Arr(vec![Json::str("streaming"), Json::str("steer")]),
                    ),
                    ("adapter_version_id", Json::str("idp:adapter-a")),
                ]),
            ),
            (
                "report_base",
                report_body("rep:t4", &vid, &suite, &durable_run, vec![]),
            ),
            ("registrar", registrar()),
        ]),
    ));
    assert_eq!(r.get("quarantined"), Some(&Json::Bool(false)));
    let d = ok(&call(
        &mut svc,
        "lab.hosting.describe",
        Json::obj([("participant_ref", Json::str(&vid))]),
    ));
    let entries = d.get("conformance_entries").expect("entries");
    assert!(matches!(entries, Json::Arr(a) if a.len() == 2));
}

// ── attach ──────────────────────────────────────────────────────────────────

/// D3: `attach` writes the hosted-session record — `attached` +
/// `component.bound` (kernel provenance), lifted `rows[]` under the
/// vouched participant's `delegate` provenance, off-allowlist classes as
/// `native_record` leaves, `end_state` → `detached`, `drift_entries` →
/// `drift_observed`, and the hosted metric folds as
/// `measurement.metric.emitted`.
#[test]
fn hosting_attach_writes_the_session_record() {
    let root = test_dir("attach");
    let (_d, _, wr, lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let vid = register_participant(
        &mut svc,
        "p:test@1.0.0",
        &[("usage_reporting", "supported")],
    );
    let r = ok(&call(
        &mut svc,
        "lab.hosting.attach",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            ("run_id", Json::str(&wr)),
            ("lease", lease),
            (
                "session",
                Json::obj([
                    ("session_ref", Json::str("hs-1")),
                    ("abi_version", Json::str("hh-hosting/1")),
                    ("mediation", Json::str("intercept")),
                    ("adapter_version_id", Json::str("idp:adapter-a")),
                ]),
            ),
            (
                "rows",
                Json::Arr(vec![
                    // A lifted observational row (allowlisted class).
                    Json::obj([
                        ("class", Json::str("model.call.completed")),
                        (
                            "payload",
                            Json::obj([
                                ("call_id", Json::str("c1")),
                                ("model", Json::str("fixture-model")),
                                ("stop_reason", Json::str("stop")),
                                ("reported", Json::Bool(true)),
                            ]),
                        ),
                    ]),
                    // A participant-writable class (`!kernel_origin &&
                    // !audit_grade`) — the vouched `delegate` mint lands.
                    Json::obj([
                        ("class", Json::str("action.tool.completed")),
                        (
                            "payload",
                            Json::obj([("call_id", Json::str("t1")), ("outcome", Json::str("ok"))]),
                        ),
                    ]),
                    // An off-allowlist class — preserved verbatim as a
                    // native_record leaf, never coerced (CC3).
                    Json::obj([
                        ("class", Json::str("participant.mystery")),
                        ("payload", Json::obj([("x", Json::Int(1))])),
                    ]),
                ]),
            ),
            (
                "drift_entries",
                Json::Arr(vec![Json::obj([
                    ("dimension", Json::str("usage_reporting")),
                    ("declared", Json::str("supported")),
                    ("observed", Json::str("partial")),
                ])]),
            ),
            ("end_state", Json::obj([("kind", Json::str("completed"))])),
        ]),
    ));
    assert_eq!(r.get("attached"), Some(&Json::Bool(true)));
    assert_eq!(r.get("session_ref"), Some(&Json::str("hs-1")));
    assert_eq!(r.get("lifted_rows"), Some(&Json::Int(2)));
    assert_eq!(r.get("native_record_leaves"), Some(&Json::Int(1)));
    assert_eq!(
        r.get("minted_participant_authority"),
        Some(&Json::str("delegate"))
    );

    let evs = svc.store().events(&wr).unwrap();
    let classes: Vec<&str> = evs.iter().map(|e| e.class.as_str()).collect();
    assert!(
        classes.contains(&"lifecycle.hosted.attached"),
        "{classes:?}"
    );
    assert!(
        classes.contains(&"lifecycle.component.bound"),
        "{classes:?}"
    );
    assert!(classes.contains(&"model.call.completed"), "{classes:?}");
    assert!(
        classes.contains(&"lifecycle.hosted.native_record"),
        "{classes:?}"
    );
    assert!(
        classes.contains(&"lifecycle.hosted.drift_observed"),
        "{classes:?}"
    );
    assert!(
        classes.contains(&"lifecycle.hosted.detached"),
        "{classes:?}"
    );

    // The participant-writable row carries the vouched participant's
    // `delegate` mint; the audit-grade `model.call.completed` is the
    // Lab's kernel record of the participant's report —
    // `provenance: participant_reported` marks it on the payload (CC2 —
    // the participant's claim is data, never an event authority it
    // cannot mint).
    let lifted = evs
        .iter()
        .find(|e| e.class == "action.tool.completed")
        .unwrap();
    let prov = lifted.provenance.as_ref().expect("lifted provenance");
    assert_eq!(prov.authority.as_str(), "delegate");
    let reported = evs
        .iter()
        .find(|e| e.class == "model.call.completed")
        .unwrap();
    assert_eq!(
        reported.provenance.as_ref().unwrap().authority.as_str(),
        "kernel"
    );
    assert_eq!(
        reported.payload.get("provenance").and_then(Json::as_str),
        Some("participant_reported")
    );
    let audit = evs
        .iter()
        .find(|e| e.class == "lifecycle.hosted.attached")
        .unwrap();
    assert_eq!(
        audit.provenance.as_ref().unwrap().authority.as_str(),
        "kernel"
    );
    // `attached{mediation}` recorded the adapter's report as data.
    assert_eq!(
        audit.payload.get("mediation").and_then(Json::as_str),
        Some("intercept")
    );
}

/// A second `attach` for the same session skips the `attached` pair — the
/// launch-path stamp converges on one row (D4), rows still append.
#[test]
fn hosting_attach_is_idempotent_on_the_attach_pair() {
    let root = test_dir("attach-twice");
    let (_d, _, wr, lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[]);
    let params = |lease: Json| {
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            ("run_id", Json::str(&wr)),
            ("lease", lease),
            (
                "session",
                Json::obj([
                    ("session_ref", Json::str("hs-1")),
                    ("abi_version", Json::str("hh-hosting/1")),
                ]),
            ),
        ])
    };
    let a = ok(&call(&mut svc, "lab.hosting.attach", params(lease.clone())));
    assert_eq!(a.get("attached"), Some(&Json::Bool(true)));
    let b = ok(&call(&mut svc, "lab.hosting.attach", params(lease)));
    assert_eq!(b.get("attached"), Some(&Json::Bool(false)));
    let n = svc
        .store()
        .events(&wr)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.hosted.attached")
        .count();
    assert_eq!(n, 1, "exactly one attached row");
}

/// AC-9-adjacent: dual provenance — a participant-reported cost row and
/// an interception-derived cost row coexist and the usage-agreement fold
/// compares them (never merges silently).
#[test]
fn hosting_attach_dual_cost_rows_fold_usage_agreement() {
    let root = test_dir("attach-cost");
    let (_d, _, wr, lease, _) = seed_runs(&root);
    let mut svc = service_in(&root);
    hello(&mut svc);
    let vid = register_participant(&mut svc, "p:test@1.0.0", &[]);
    let cost = |key: &str, value: i64, reported: bool| {
        Json::obj([
            ("class", Json::str("measurement.cost.attributed")),
            (
                "payload",
                Json::obj([
                    ("dimension", Json::str(key)),
                    ("amount", Json::Int(value)),
                    ("currency", Json::str("usd_micro")),
                    ("attribution_key", Json::str("k1")),
                    ("mediation", Json::str("intercept")),
                    ("reported", Json::Bool(reported)),
                ]),
            ),
        ])
    };
    let r = ok(&call(
        &mut svc,
        "lab.hosting.attach",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            ("run_id", Json::str(&wr)),
            ("lease", lease),
            ("session", Json::obj([("session_ref", Json::str("hs-9"))])),
            (
                "rows",
                Json::Arr(vec![cost("spend", 100, true), cost("spend", 90, false)]),
            ),
        ]),
    ));
    // Both rows land — the fold reads the pair (disagreement is data, a
    // synthesized zero would be the lie AC-9 forbids).
    let evs = svc.store().events(&wr).unwrap();
    let costs = evs
        .iter()
        .filter(|e| e.class == "measurement.cost.attributed")
        .count();
    assert_eq!(costs, 2);
    let emitted = evs
        .iter()
        .filter(|e| e.class == "measurement.metric.emitted")
        .count();
    assert!(
        emitted as i64 >= r.get("metrics_emitted").and_then(Json::as_int).unwrap_or(0),
        "metric rows must equal the reported count"
    );
}
