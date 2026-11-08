//! R6.22 AC-6 legs (REQ-XC-301; ADR-0436): adapter C's
//! `placement_supported` carries `lab_host`; the `container_installed`
//! declaration with `HostingExt{model_io_intercept: base_url,
//! process_placement}` validates through the real codecs; a native-log
//! lift never sets `mediation: mediated` (`model_io` is earned only from
//! proxy receipts — design/34 §1.1).

use hharness_hir::leaves::Text;
use hharness_hir::records::{AssumptionDebtRecord, EvidenceRef, ExpiryCondition, OwnerRef};
use hharness_hosting::adapter_c::{
    adapter_c_record, AdapterC, ContainerDriver, EndStateSnapshot, LifterTable,
};
use hharness_hosting::events::{EventChannel, HostedOrigin, Mediation};
use hharness_hosting::records::{HostingExt, ModelIoIntercept, ProcessPlacement, HOSTING_EXT_KEY};
use hharness_ontology::debt::{DebtStatus, ExpiryKind};
use hharness_ontology::participant::{HostingMechanism, Observability};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::Json;

fn debt() -> AssumptionDebtRecord {
    let provenance = ProvenanceRecord::kernel("hh-hosting/test", 0);
    AssumptionDebtRecord {
        rule_id: "hh.hosting.adapter_c".into(),
        hypothesis: Text::new("placement declaration", "hh.adapter.c", provenance.clone()),
        evidence_refs: vec![EvidenceRef::legacy("r6.22-fixture")],
        owner: OwnerRef::principal("hh.adapter.c"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ProbeFailure,
            value: Some("placement drift".into()),
        },
        removal_test_ref: "tests/r6_22.rs".into(),
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

#[test]
fn adapter_c_placement_supported_includes_lab_host() {
    let debt = debt();
    let rec = adapter_c_record("r6.22", &LifterTable::default(), debt);
    assert!(rec
        .placement_supported
        .contains(&ProcessPlacement::InEnvironment));
    assert!(rec.placement_supported.contains(&ProcessPlacement::LabHost));
}

#[test]
fn container_installed_declaration_validates() {
    // The shape `hharness_runner::admission::harness_declaration` emits;
    // decoded here through the hosting/ontology codecs (CC1 — one impl).
    let decl = Json::obj([
        ("kind", Json::str("participant")),
        (
            "descriptor",
            Json::obj([
                ("class", Json::str("hosted")),
                ("hosting_mechanism", Json::str("container_installed")),
                (
                    "observability_level",
                    Json::Arr(vec![Json::str("end_state"), Json::str("model_io")]),
                ),
            ]),
        ),
        (
            "ext",
            Json::Obj(std::collections::BTreeMap::from([(
                HOSTING_EXT_KEY.to_string(),
                Json::obj([
                    ("model_io_intercept", Json::str("base_url")),
                    ("process_placement", Json::str("in_environment")),
                ]),
            )])),
        ),
        ("model_io_evidence", Json::str("proxy_receipts_only")),
    ]);
    let d = decl.get("descriptor").unwrap();
    assert_eq!(
        HostingMechanism::parse(d.get("hosting_mechanism").unwrap().as_str().unwrap()),
        Some(HostingMechanism::ContainerInstalled)
    );
    let obs: Vec<Observability> = d
        .get("observability_level")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>()),
            _ => None,
        })
        .unwrap()
        .iter()
        .filter_map(|s| Observability::parse(s))
        .collect();
    assert!(obs.contains(&Observability::EndState));
    assert!(obs.contains(&Observability::ModelIo));
    // The ext block decodes through HostingExt's strict codec.
    let ext = HostingExt::from_json(
        decl.get("ext").unwrap().get(HOSTING_EXT_KEY).unwrap(),
        "ext",
    )
    .unwrap();
    assert_eq!(ext.model_io_intercept, Some(ModelIoIntercept::BaseUrl));
    assert_eq!(ext.process_placement, Some(ProcessPlacement::InEnvironment));
    // `lab_host` decodes as a declared placement (a slot still refuses typed until AR-19).
    let mut e2 = decl
        .get("ext")
        .unwrap()
        .get(HOSTING_EXT_KEY)
        .unwrap()
        .clone();
    if let Json::Obj(ref mut m) = e2 {
        m.insert("process_placement".into(), Json::str("lab_host"));
    }
    let ext2 = HostingExt::from_json(&e2, "ext").unwrap();
    assert_eq!(ext2.process_placement, Some(ProcessPlacement::LabHost));
}

struct Drv;
impl ContainerDriver for Drv {
    fn install(&mut self, _i: &str) -> Result<String, String> {
        Ok("c1".into())
    }
    fn run(&mut self, _c: &str, _s: &Json) -> Result<(), String> {
        Ok(())
    }
    fn kill(&mut self, _c: &str) -> Result<(), String> {
        Ok(())
    }
    fn snapshot(&mut self, _c: &str) -> Result<EndStateSnapshot, String> {
        Ok(EndStateSnapshot {
            exit_code: Some(0),
            artifacts: vec![],
            outcome: None,
            raw_ref: None,
        })
    }
    fn log_rows(&mut self, _c: &str, _w: u64) -> Vec<Json> {
        vec![
            Json::obj([("type", Json::str("tool.call")), ("n", Json::Int(1))]),
            Json::obj([("type", Json::str("session.end")), ("n", Json::Int(2))]),
        ]
    }
}

#[test]
fn native_log_lift_never_mediates() {
    let debt = debt();
    let mut a = AdapterC::new(
        Box::new(Drv),
        adapter_c_record("v", &LifterTable::default(), debt),
        LifterTable::default(),
        "stderr_lines",
    );
    let _ = a.install("img");
    let _ = a.run(&Json::obj([("x", Json::Int(1))]));
    let rows = a.close_and_reconstruct().unwrap();
    assert!(rows.len() >= 2);
    let mut saw_log = false;
    for r in &rows {
        assert_ne!(r.mediation, Mediation::Mediated, "{}", r.kind);
        if r.event_channel == EventChannel::Log {
            saw_log = true;
            assert_eq!(r.mediation, Mediation::Unobserved);
            assert_eq!(r.origin, HostedOrigin::Adapter);
        }
    }
    assert!(saw_log);
}
