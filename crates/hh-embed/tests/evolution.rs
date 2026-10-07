//! S6.1a boundary tests — the `lab.evolution.*` surface over `hh-embed`
//! (records in, records out; one Store; §05h R-2.9.5). The gating legs
//! (experimental opt-in → `serves_measurement` capability → strict codec
//! → `evidence_stale` refusal) run on every build; the campaign legs
//! ride `tier-c4`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-evo-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> (PathBuf, EmbedService) {
    let root = test_dir("svc");
    let svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap();
    (root, svc)
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn err_kind(r: &Json) -> String {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .or_else(|| {
            r.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Json::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn hello(svc: &mut EmbedService, caps: &[(&str, bool)]) {
    let mut m = BTreeMap::new();
    for (k, v) in caps {
        m.insert(k.to_string(), Json::Bool(*v));
    }
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(m)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn hello_evo(svc: &mut EmbedService) {
    hello(svc, &[("experimental", true), ("serves_measurement", true)]);
}

/// The boundary is experimental-gated like every Group L op.
#[test]
fn evolution_ops_require_experimental_optin() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("serves_measurement", true)]);
    let r = call(
        &mut svc,
        "lab.evolution.view",
        Json::obj([("run", Json::str("evo-x"))]),
    );
    assert_eq!(err_kind(&r), "ExperimentalRequired", "{r:?}");
}

/// …and capability-gated (`serves_measurement`) like the measurement
/// family.
#[test]
fn evolution_ops_require_measurement_capability() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("experimental", true)]);
    let r = call(
        &mut svc,
        "lab.evolution.view",
        Json::obj([("run", Json::str("evo-x"))]),
    );
    assert_eq!(err_kind(&r), "CapabilityNotDeclared", "{r:?}");
}

/// An unknown `lab.evolution.*` member is a typed refusal — the dispatch
/// arm is closed over the declared op set.
#[cfg(feature = "tier-c4")]
#[test]
fn evolution_unknown_op_refuses() {
    let (_root, mut svc) = service();
    hello_evo(&mut svc);
    // The op catalogue is closed at the schema gate — an undeclared
    // member is `SchemaViolation{unknown_method}` before dispatch.
    let r = call(
        &mut svc,
        "lab.evolution.not_a_stage",
        Json::obj([("run", Json::str("evo-x"))]),
    );
    assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    assert_eq!(
        r.get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("code"))
            .and_then(Json::as_str),
        Some("unknown_method"),
        "{r:?}"
    );
}

/// `lab.evolution.rebase` (S6.3a) — the op is declared, capability/experimental
/// gated like the rest of the surface, and strict-decodes its members
/// (`candidate_id` is required before dispatch resolves the run).
#[cfg(feature = "tier-c4")]
#[test]
fn evolution_rebase_is_a_declared_op() {
    let (_root, mut svc) = service();
    hello_evo(&mut svc);
    // Missing `candidate_id` → a strict-decode SchemaViolation (the op
    // decoded — an undeclared op would be `unknown_method` instead).
    let r = call(
        &mut svc,
        "lab.evolution.rebase",
        Json::obj([("run", Json::str("evo-x"))]),
    );
    assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    assert_ne!(
        r.get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("code"))
            .and_then(Json::as_str),
        Some("unknown_method"),
        "{r:?}"
    );
    // An unknown run with a full envelope → the campaign fence refuses.
    let r = call(
        &mut svc,
        "lab.evolution.rebase",
        Json::obj([
            ("run", Json::str("evo-absent")),
            ("candidate_id", Json::str("cand:x")),
            ("proposal", Json::obj([])),
            ("base_doc", Json::obj([])),
        ]),
    );
    assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
}

/// A malformed spec is a strict-codec `SchemaViolation` at the boundary —
/// never a partial decode.
#[cfg(feature = "tier-c4")]
#[test]
fn campaign_open_rejects_malformed_spec() {
    let (_root, mut svc) = service();
    hello_evo(&mut svc);
    let r = call(
        &mut svc,
        "lab.evolution.campaign_open",
        Json::obj([(
            "spec",
            Json::obj([
                ("protocol", Json::str("human_proposed")),
                ("bogus_member", Json::Bool(true)),
            ]),
        )]),
    );
    assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
}

/// A valid spec whose L3 split pin does not resolve refuses
/// `Refused{reason: evidence_stale}` — the refusal table's codes cross
/// the boundary verbatim.
#[cfg(feature = "tier-c4")]
#[test]
fn campaign_open_requires_the_split_pin() {
    use hh_evolution::records::{CorpusSpec, EvolutionCampaignSpec, SlotAllocation, StopRule};
    use hh_ontology::lab::EnvironmentFamily;

    let (_root, mut svc) = service();
    hello_evo(&mut svc);
    let spec = EvolutionCampaignSpec {
        campaign_id: String::new(),
        protocol: "human_proposed".into(),
        base_definition_ref: "def:base".into(),
        service_definition_ref: "def:svc".into(),
        corpus: CorpusSpec {
            environments: vec![EnvironmentFamily::CodingTerminal],
            suite_refs: vec!["suite:t".into()],
            metric_refs: vec!["metric:m".into()],
            task_ids: vec!["task:t".into()],
            evidence_refs: vec!["ev:1".into()],
            split_assignment_ref: "no-such-pin".into(),
            layers: None,
        },
        exclusion_targets: vec![],
        must_code_targets: vec![],
        allowed_target_kinds: None,
        semantic_ops_bound: 8,
        min_flip_share_ppm: 500_000,
        min_replicates: 1,
        retention_margin_ppm: 50_000,
        veto_metrics: vec![],
        slot_allocation: SlotAllocation::Uniform { min_share_ppm: 0 },
        stop_rule: StopRule {
            max_candidates: Some(4),
            stagnation_window: None,
            budget_cap_ref: None,
            no_addressable_failure: true,
        },
        maturity_flags: vec![],
        proposer_family: "human".into(),
        hosted_participants: false,
        reported_only_dimensions: vec![],
        proposer_variant_ref: None,
        target_class: None,
        target_classes: Vec::new(),
        hosted_coordinates: Vec::new(),
        hosted_descriptor_refs: Vec::new(),
        authority_cap: None,
        rollout_policy: None,
        judge_policy: None,
    };
    let r = call(
        &mut svc,
        "lab.evolution.campaign_open",
        Json::obj([("spec", spec.to_json())]),
    );
    assert_eq!(err_kind(&r), "Refused", "{r:?}");
    let reason = r
        .get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or_default();
    assert_eq!(reason, "evidence_stale", "{r:?}");
}

/// Open → view → ensure → close through the boundary (the split pin is
/// deposited into the same `LabDocs` root the service resolves against —
/// records-in, one store).
#[cfg(feature = "tier-c4")]
#[test]
fn campaign_open_view_ensure_close() {
    use hh_evolution::campaign::doc_kind;
    use hh_evolution::records::{CorpusSpec, EvolutionCampaignSpec, SlotAllocation, StopRule};
    use hh_experiment::docs::LabDocs;
    use hh_lab::bench::SplitAssignmentRecord;
    use hh_ontology::lab::{EnvironmentFamily, SplitLabel};

    let (root, mut svc) = service();
    hello_evo(&mut svc);
    // Deposit the L3 pin at the service's store root.
    let docs = LabDocs::open(&root.join("store")).unwrap();
    let split = SplitAssignmentRecord {
        suite_id: "suite:t".into(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: BTreeMap::from([("task:t".to_string(), SplitLabel::Dev)]),
        split_hash: "sha256:split".into(),
        registered_at: 1,
    };
    docs.put_named(doc_kind::SPLIT, "split:t", &split.to_json())
        .unwrap();

    let spec = EvolutionCampaignSpec {
        campaign_id: String::new(),
        protocol: "human_proposed".into(),
        base_definition_ref: "def:base".into(),
        service_definition_ref: "def:svc".into(),
        corpus: CorpusSpec {
            environments: vec![EnvironmentFamily::CodingTerminal],
            suite_refs: vec!["suite:t".into()],
            metric_refs: vec!["metric:m".into()],
            task_ids: vec!["task:t".into()],
            evidence_refs: vec!["ev:1".into()],
            split_assignment_ref: "split:t".into(),
            layers: None,
        },
        exclusion_targets: vec![],
        must_code_targets: vec![],
        allowed_target_kinds: None,
        semantic_ops_bound: 8,
        min_flip_share_ppm: 500_000,
        min_replicates: 1,
        retention_margin_ppm: 50_000,
        veto_metrics: vec![],
        slot_allocation: SlotAllocation::Uniform { min_share_ppm: 0 },
        stop_rule: StopRule {
            max_candidates: Some(4),
            stagnation_window: None,
            budget_cap_ref: None,
            no_addressable_failure: true,
        },
        maturity_flags: vec![],
        proposer_family: "human".into(),
        hosted_participants: false,
        reported_only_dimensions: vec![],
        proposer_variant_ref: None,
        target_class: None,
        target_classes: Vec::new(),
        hosted_coordinates: Vec::new(),
        hosted_descriptor_refs: Vec::new(),
        authority_cap: None,
        rollout_policy: None,
        judge_policy: None,
    };
    let r = call(
        &mut svc,
        "lab.evolution.campaign_open",
        Json::obj([("spec", spec.to_json())]),
    );
    let run_id = r
        .get("result")
        .and_then(|x| x.get("run_id"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("campaign_open: {r:?}"))
        .to_string();
    assert!(run_id.starts_with("evo-"), "{run_id}");

    // `view` — the candidate projection crosses the boundary.
    let r = call(
        &mut svc,
        "lab.evolution.view",
        Json::obj([("run", Json::str(&run_id))]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|x| x.get("status"))
            .and_then(Json::as_str),
        Some("open"),
        "{r:?}"
    );

    // `ensure` — the engine is already cached; the op is idempotent.
    let r = call(
        &mut svc,
        "lab.evolution.campaign_ensure",
        Json::obj([("run", Json::str(&run_id))]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|x| x.get("run_id"))
            .and_then(Json::as_str),
        Some(run_id.as_str()),
        "{r:?}"
    );

    // `close` — the lifecycle row lands.
    let r = call(
        &mut svc,
        "lab.evolution.close",
        Json::obj([("run", Json::str(&run_id))]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|x| x.get("status"))
            .and_then(Json::as_str),
        Some("closed"),
        "{r:?}"
    );
}

/// `propose`'s strict decoding — a malformed `base_doc` is a typed
/// `SchemaViolation`, never a panic or a partial record.
#[cfg(feature = "tier-c4")]
#[test]
fn propose_rejects_malformed_records() {
    let (_root, mut svc) = service();
    hello_evo(&mut svc);
    let r = call(
        &mut svc,
        "lab.evolution.propose",
        Json::obj([
            ("run", Json::str("evo-x")),
            ("proposal", Json::obj([("not", Json::str("a proposal"))])),
            ("base_doc", Json::obj([("not", Json::str("a doc"))])),
        ]),
    );
    assert!(matches!(
        err_kind(&r).as_str(),
        "SchemaViolation" | "Refused"
    ));
}
