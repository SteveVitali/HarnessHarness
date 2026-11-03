//! R5.16 — the `lab.analysis.analyze` boundary battery for the
//! `finite_manifest_contrast` kind: declared numeric params that are
//! not integers are nonfinite input and refuse (never a silent
//! default); an unbound selection supplies no experiment view and the
//! kernel refuses through the typed surface. Fails-without: the
//! strict-decode gate and the `view` field do not exist at the R5.15b
//! base — this file compiles red there.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ontology::eval::{EstimatorSelection, IntervalMethod};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-r5-16-embed-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn svc(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.to_path_buf(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.to_path_buf(),
        holder: "r5-16-analysis".to_string(),
    })
    .unwrap()
}

fn call(s: &mut EmbedService, method: &str, params: Json) -> Json {
    s.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn fmc_params() -> Json {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: "finite_manifest_contrast".into(),
        query: QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(Json::obj([
                ("test_arm", Json::str("B")),
                ("control_arm", Json::str("A")),
            ])),
            grain: None,
        },
        spec_ref: None,
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::FiniteManifestContrast("beta{1,1}".into()),
            selection_rule: "padr-r5-4.finite_manifest".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    Json::obj([
        ("spec", s.to_json()),
        ("rows", Json::Arr(vec![])),
        ("tasks", Json::Arr(vec![])),
    ])
}

fn hello(s: &mut EmbedService) {
    let r = call(
        s,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r5-16")),
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
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn err_kind(r: &Json) -> &str {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or("")
}

// AC-2 — nonfinite input at the boundary: a present-but-non-integer
// numeric param refuses (SchemaViolation), never a silent default.
#[test]
fn nonint_numeric_param_refuses() {
    let root = tmp("nonint");
    let mut s = svc(&root);
    hello(&mut s);
    let mut p = fmc_params();
    if let Json::Obj(m) = &mut p {
        m.insert("confidence_ppm".into(), Json::str("not-a-number"));
    }
    let r = call(&mut s, "lab.analysis.analyze", p);
    assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    // A valid integer member still reaches the kernel (a different,
    // downstream refusal — the bound-experiment view is absent).
    let r2 = call(&mut s, "lab.analysis.analyze", fmc_params());
    assert_eq!(err_kind(&r2), "Refused", "{r2:?}");
    let reason = r2
        .get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(
        reason.contains("spec_ref") || reason.contains("view"),
        "the unbound-selection refusal is typed and names the missing view: {r2:?}"
    );
}
