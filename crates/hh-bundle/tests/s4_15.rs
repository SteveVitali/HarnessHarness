//! S4.15 — the bundle-side slice of measurement C1 + identity C1
//! (R-2.9.1¹ interchange cost_view with declared unit-conversion loss;
//! R-2.12.1¹ hosted-participant bundles — `participant_version` as a
//! claim, DRIFT as data — and lineage-aware export).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::attribution::{Attribution, ModelRef};
use hh_budget::pricing::{CostProvenance, Derivation, ProvenanceClass, SpendRow};
use hh_budget::quantity::Money;
use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::codec::Decoded;
use hh_bundle::export::{export_target, PublicationPolicy};
use hh_bundle::manifest::BundlePolicy;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{EventRef, ParticipantClass, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-s415-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A kernel-stamped event (the audit-grade `lifecycle.hosted.*` /
/// `measurement.cost.attributed` classes require kernel provenance).
fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn spend_payload(run: &str, src_event: &str, micro: i64) -> Json {
    hh_budget::events::cost_attributed_payload(&SpendRow {
        resource: hh_ontology::dimensions::DimensionId::Spend,
        money: Money {
            micro_units: micro,
            currency: "USD".into(),
        },
        provenance: CostProvenance::ParticipantReported,
        provenance_class: ProvenanceClass::Reported,
        derivation: Derivation::ProviderPriced {
            unit: "call".into(),
            rate_ref: None,
        },
        confidence: hh_budget::pricing::Confidence::Estimate,
        coverage_ppm: hh_budget::quantity::PPM_SCALE,
        pricing_ref: None,
        source_event: EventRef {
            run_id: run.into(),
            event_id: src_event.into(),
        },
        model_ref: ModelRef {
            profile_ref: "profile:test".into(),
            provider_model_id: "m-1".into(),
            serving_route: "route-a".into(),
            effort: None,
        },
        attribution: Attribution::subject(run, "budget-1", "participant-0"),
        roles: BTreeMap::new(),
    })
}

struct Rig {
    store: Store,
    run: String,
    _lease: Lease,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

/// One hosted run with `attached` + `drift_observed` + a cost row.
fn hosted_rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.participant_class = ParticipantClass::Hosted;
    m.hosting_mechanism = Some("session_abi".into());
    let (run, lease) = store.open_run(m, "s415.test").unwrap();
    store
        .append(
            &run,
            &lease,
            vec![
                ev(
                    "attach-1",
                    "lifecycle.hosted.attached",
                    Json::obj([
                        ("session_ref", Json::str("session:hosted-1")),
                        (
                            "participant_version_identity",
                            Json::str("idp:participant:acme-v9"),
                        ),
                        ("participant_ref", Json::str("idp:participant:acme")),
                        ("hosting_mechanism", Json::str("session_abi")),
                    ]),
                ),
                ev(
                    "drift-1",
                    "lifecycle.hosted.drift_observed",
                    Json::obj([
                        ("dimension", Json::str("streaming")),
                        ("declared", Json::str("supported")),
                        ("observed", Json::str("unsupported")),
                    ]),
                ),
                ev(
                    "cost-1",
                    "measurement.cost.attributed",
                    spend_payload(&run, "src-1", 1_500_000),
                ),
            ],
        )
        .unwrap();
    Rig {
        store,
        run,
        _lease: lease,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn inputs<'a>(rig: &'a Rig) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    // `artifact_bytes` must live as long as `inputs` — leak a small
    // closure into the test scope.
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("s415.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        nondeterminism: vec![],
        participant_class: None,
    }
}

/// The hosted bundle carries `participant_version` as a *claim*
/// (authority `unverified` — never a native fact, CC2/spec §8.3 failure
/// row) and `drift_observed` rows as `conformance_drift` claim data.
#[test]
fn hosted_bundle_carries_participant_version_claim_and_drift_data() {
    let rig = hosted_rig("hosted");
    let a = assemble(&inputs(&rig)).unwrap();
    assert_eq!(a.manifest.participant_class, "hosted");
    let pv = a
        .manifest
        .claims
        .iter()
        .find(|c| c.role == "participant_version")
        .expect("participant_version claim on a hosted bundle");
    assert_eq!(
        pv.value.get("participant_version_identity"),
        Some(&Json::str("idp:participant:acme-v9"))
    );
    assert_eq!(
        pv.value.get("hosting_mechanism"),
        Some(&Json::str("session_abi"))
    );
    // The claim is participant-reported — `unverified`, never the Lab's
    // authority (CC2: the content never vouches itself).
    assert_eq!(
        pv.provenance.get("authority").and_then(Json::as_str),
        Some("unverified")
    );
    assert!(
        pv.provenance
            .get("source_event")
            .and_then(Json::as_str)
            .unwrap_or("")
            .ends_with(":attach-1"),
        "the claim cites its evidence row"
    );
    // DRIFT is recorded as data.
    let drifts: Vec<_> = a
        .manifest
        .claims
        .iter()
        .filter(|c| c.role == "conformance_drift")
        .collect();
    assert_eq!(drifts.len(), 1);
    assert_eq!(
        drifts[0].value.get("dimension"),
        Some(&Json::str("streaming"))
    );
    // A native run picks up no hosted claims.
    let mut store = Store::open(dir("native")).unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(rig.def_addr.clone());
    let (run, _lease) = store.open_run(m, "s415.test").unwrap();
    let rig2 = Rig {
        store,
        run,
        _lease,
        policy: BundlePolicy::default(),
        def_addr: rig.def_addr.clone(),
        def_bytes: rig.def_bytes.clone(),
    };
    let a2 = assemble(&inputs(&rig2)).unwrap();
    assert!(a2
        .manifest
        .claims
        .iter()
        .all(|c| c.role != "participant_version"));
}

/// Interchange export carries the `cost_view` fields per run and declares
/// the micro → unit conversion as loss (R-2.9.1¹).
#[test]
fn interchange_exports_cost_view_fields_with_declared_unit_loss() {
    let rig = hosted_rig("interchange");
    let a = assemble(&inputs(&rig)).unwrap();
    let decoded = Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let out = export_target(&decoded, "interchange_trajectory", &PublicationPolicy::default())
        .unwrap();
    assert_eq!(out.granularity_ceiling, "product");
    let header_text = String::from_utf8(out.files["interchange.json"].clone()).unwrap();
    let header = hh_wire::json::parse(&header_text).unwrap();
    let cv = header
        .get("cost_views")
        .and_then(|c| c.get(&rig.run))
        .expect("cost_views names the subject run");
    assert_eq!(
        cv.get("total_spend_micro").and_then(|t| t.get("USD")),
        Some(&Json::Int(1_500_000)),
        "the exact micro-unit total rides verbatim"
    );
    assert_eq!(
        cv.get("total_spend").and_then(|t| t.get("USD")),
        Some(&Json::Int(1)),
        "the unit conversion is applied and declared"
    );
    // The conversion is a *declared* loss entry — never silent.
    let entries: Vec<Json> = match out.loss_report.get("entries") {
        Some(Json::Arr(v)) => v.clone(),
        _ => vec![],
    };
    assert!(
        entries.iter().any(|e: &Json| {
            e.get("class").and_then(Json::as_str) == Some("unit_conversion")
                && e.get("member").and_then(Json::as_str)
                    == Some(format!("cost_view:{}", rig.run).as_str())
        }),
        "the unit conversion lands a loss row: {entries:?}"
    );
    // Lineage members ride the header (empty here — present, never absent).
    assert!(header.get("subject_lineage").is_some());
    assert!(header.get("lineage_prefixes").is_some());
    // The trajectory still carries the raw event rows (the exact source).
    let traj = String::from_utf8(out.files["trajectory.jsonl"].clone()).unwrap();
    assert!(traj.contains("measurement.cost.attributed"));
}

/// Harbor job-dir export carries lineage prefixes + the signed audit head
/// per run — lineage-aware export (R-2.12.1¹).
#[test]
fn harbor_export_carries_lineage_prefixes_and_audit_head() {
    let rig = hosted_rig("harbor");
    let a = assemble(&inputs(&rig)).unwrap();
    let decoded = Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let out =
        export_target(&decoded, "harbor_job_dir", &PublicationPolicy::default()).unwrap();
    let job_text = String::from_utf8(out.files["job.json"].clone()).unwrap();
    let job = hh_wire::json::parse(&job_text).unwrap();
    let run_row: Json = match job.get("runs") {
        Some(Json::Arr(r)) => r.first().cloned().expect("one run row"),
        _ => panic!("job.json carries no runs[]"),
    };
    assert_eq!(
        run_row.get("lineage_prefixes"),
        Some(&Json::Arr(vec![])),
        "no fork/continuation anchors → an empty prefix list, present"
    );
    // No signed checkpoint in the fixture → `null`, an honest absence
    // (never a fabricated head).
    assert_eq!(run_row.get("audit_tree_head"), Some(&Json::Null));
    // The subject lineage member is present and carries its anchors.
    let lineage = job.get("subject_lineage").expect("subject_lineage present");
    match lineage {
        Json::Arr(entries) => {
            assert!(
                entries
                    .iter()
                    .all(|e| e.get("run_id").is_some() && e.get("head_hash").is_some()),
                "every lineage entry is a run_id/up_to_seq/head_hash anchor"
            );
        }
        other => panic!("subject_lineage must be an array: {other:?}"),
    }
}
