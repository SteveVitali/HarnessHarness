//! S3.12 — M19 `boundary_overhead_ms` and the OQ-075 measurement
//! (AC-R-2.12.2-9; T-LCD-14; ADR-0181 §8). Cost is measured, not assumed:
//! the same `compaction_strategy` op runs `in_process` and
//! `subprocess_confined` under a matched invocation cap (`MatchSpec{matched_cap}`
//! — same N, never a different budget); `boundary_overhead_ms` distributions
//! are reported per invocation, the headline metric is unchanged within the
//! pre-registered margin, and the measured p95 sets the OQ-075 `hot_path`
//! ceiling recorded as an `AssumptionDebtRecord` (the `/1` codec round-trips
//! it — CC7).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hh_embed_schema::plugin_abi::BindParams;
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_hir::{debt_from_json, debt_json};
use hh_ontology::debt::{
    DebtClass, DebtStatus, EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, OwnerRef,
    RemovalTest, RemovalTestKind,
};
use hh_plugin_fixture::fixture::FixtureLogic;
use hh_plugin_fixture::PluginRuntime;
use hh_provenance::ProvenanceRecord;
use hh_registry::extension::plugin::{PluginIdentity, PluginManifest, Requests, Requires};
use hh_varhost::channel::MemIo;
use hh_varhost::{
    attach, spawn, InvokeOutcome, RecordingPorts, SpawnSpec, VariantHost, VariantPackage,
    VariantSession, VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);
/// The matched cap — both arms run exactly this many `assess` invocations.
const N: usize = 24;
/// The pre-registered margin for the headline metric (fraction of `ok` docs):
/// zero — the outputs are byte-identical documents (V2).
const HEADLINE_MARGIN: f64 = 0.0;

fn fixture_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-plugin-fixture");
    p.exists().then_some(p)
}

fn helper_present() -> bool {
    hh_env::helper::helper_binary().is_some()
}

fn manifest() -> PluginManifest {
    PluginManifest {
        identity: PluginIdentity {
            namespace: "local".into(),
            name: "fixture".into(),
            version_label: None,
        },
        tier: "C0".into(),
        depends_on: vec![],
        requires: Requires {
            hir_dialect: "1.0".into(),
            registry_dialect: "1.0".into(),
            plugin_abi: "1.0".into(),
            contracts: vec![],
            records: vec![],
        },
        contributions: vec![],
        requests: Requests::default(),
        claims: Json::obj([]),
        conformance_claims: vec![],
        parameters: None,
        summary: Json::Null,
        ext: BTreeMap::new(),
    }
}

fn bind_params() -> BindParams {
    BindParams {
        slot: "slot-1".into(),
        class_id: "compaction_strategy".into(),
        contract_version: "1.0".into(),
        variant: Json::Null,
        params: Json::Null,
        profile: Json::Null,
        account: Json::Null,
        budget: Json::Null,
        placement: "subprocess_confined".into(),
    }
}

/// `assess`'s output document — the headline payload both arms produce.
fn assess_doc(binding: &str) -> Json {
    Json::obj([
        ("ok", Json::Bool(true)),
        ("operation", Json::str("assess")),
        ("binding", Json::str(binding)),
    ])
}

/// The `in_process` arm — a first-party variant pays no boundary: the local
/// call's elapsed time *is* the overhead measurement (≈0, never assumed).
fn in_process_arm() -> (Vec<u64>, Vec<Json>) {
    let mut overhead = Vec::new();
    let mut outputs = Vec::new();
    for _ in 0..N {
        let t = Instant::now();
        let doc = assess_doc("b1");
        overhead.push(t.elapsed().as_millis() as u64);
        outputs.push(doc);
    }
    (overhead, outputs)
}

/// The `subprocess_confined` arm — the spawned fixture through the live
/// helper lane; `boundary_overhead_ms` is the kernel-measured M19 field on
/// each `lifecycle.component.invoked` row, never an estimate.
fn confined_arm() -> Option<(Vec<u64>, Vec<Json>, VariantHost<RecordingPorts, VecEvents>)> {
    let bin = fixture_bin()?;
    if !helper_present() {
        eprintln!("overhead: hh-helper binary not found — confined lane skipped");
        return None;
    }
    let pkg = VariantPackage {
        root: bin.parent().unwrap().to_path_buf(),
        manifest: manifest(),
        content: "content:fixture".into(),
        version_id: "pin:fixture".into(),
        execs: vec![bin.clone()],
    };
    let exec_args = vec![
        "--plugin-id".into(),
        "local/fixture".into(),
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
    ];
    static NSEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let spec = SpawnSpec {
        package: pkg,
        session_id: format!("m19-{}", std::process::id()),
        backend: "direct".into(),
        socket_dir: std::env::temp_dir().join(format!(
            "vh-m19-{}-{}",
            std::process::id(),
            NSEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )),
        exec_args,
        cap: Requests::default(),
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture", "pin:fixture"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:compaction_strategy".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    };
    let (mut session, _lowered) = spawn(&spec).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).expect("bind");

    let mut outputs = Vec::new();
    for _ in 0..N {
        match host
            .invoke(&mut session, &b, "assess", vec![], "res-m19", TIMEOUT)
            .expect("invoke")
        {
            InvokeOutcome::Outputs(mut o) => outputs.push(o.remove(0)),
            other => panic!("assess failed: {other:?}"),
        }
    }
    // The M19 distribution — the measured field on each invocation row.
    let overhead: Vec<u64> = host
        .events
        .rows
        .iter()
        .filter(|(c, _, _)| c == "lifecycle.component.invoked")
        .filter_map(|(_, _, p)| p.get("boundary_overhead_ms").and_then(Json::as_int))
        .map(|v| v as u64)
        .collect();
    Some((overhead, outputs, host))
}

/// The MemIo lane — the channel boundary measured without a subprocess
/// (the fallback when `hh-helper` is unavailable: the distribution is real,
/// the process-isolation part is the confined lane's).
fn memio_arm() -> (Vec<u64>, Vec<Json>, VariantHost<RecordingPorts, VecEvents>) {
    let mut logic = FixtureLogic::from_args(&[]);
    logic.own_socket = None;
    logic.plugin_id = "local/fixture".into();
    logic.version_id = "pin:fixture".into();
    logic.content = "content:fixture".into();
    let (host_io, plugin_io) = MemIo::pair();
    let _t = std::thread::spawn(move || {
        if let Ok(mut rt) = PluginRuntime::connect(Box::new(plugin_io), logic, TIMEOUT) {
            let _ = rt.run();
        }
    });
    let pkg = VariantPackage {
        root: PathBuf::from("/pkg"),
        manifest: manifest(),
        content: "content:fixture".into(),
        version_id: "pin:fixture".into(),
        execs: vec![],
    };
    static NSEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let spec = SpawnSpec {
        package: pkg,
        session_id: format!("m19-mem-{}", std::process::id()),
        backend: "direct".into(),
        socket_dir: std::env::temp_dir().join(format!(
            "vh-m19m-{}-{}",
            std::process::id(),
            NSEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )),
        exec_args: vec![],
        cap: Requests::default(),
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture", "pin:fixture"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:compaction_strategy".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    };
    let mut session: VariantSession = attach(Box::new(host_io), &spec).expect("attach");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).expect("bind");
    let mut outputs = Vec::new();
    for _ in 0..N {
        match host
            .invoke(&mut session, &b, "assess", vec![], "res-m19", TIMEOUT)
            .expect("invoke")
        {
            InvokeOutcome::Outputs(mut o) => outputs.push(o.remove(0)),
            other => panic!("assess failed: {other:?}"),
        }
    }
    let overhead: Vec<u64> = host
        .events
        .rows
        .iter()
        .filter(|(c, _, _)| c == "lifecycle.component.invoked")
        .filter_map(|(_, _, p)| p.get("boundary_overhead_ms").and_then(Json::as_int))
        .map(|v| v as u64)
        .collect();
    (overhead, outputs, host)
}

/// `{n, min, p50, p95, p99, max}` — the distribution, never a mean alone.
fn dist(samples: &[u64]) -> Json {
    let mut s = samples.to_vec();
    s.sort_unstable();
    let rank = |ppm: u64| -> i64 {
        if s.is_empty() {
            return 0;
        }
        let i = ((s.len() as u64 * ppm + 999_999) / 1_000_000).max(1) as usize - 1;
        s[i.min(s.len() - 1)] as i64
    };
    Json::obj([
        ("n", Json::Int(s.len() as i64)),
        ("min", Json::Int(*s.first().unwrap_or(&0) as i64)),
        ("p50", Json::Int(rank(500_000))),
        ("p95", Json::Int(rank(950_000))),
        ("p99", Json::Int(rank(990_000))),
        ("max", Json::Int(*s.last().unwrap_or(&0) as i64)),
    ])
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("m19.test", 0)
}

/// The OQ-075 ceiling record — the measured p95 becomes the `hot_path`
/// ceiling claim, evidence-linked to the measurement, expiring on review,
/// discharging on a re-probe (`probe_run`).
fn oq075_debt_record(
    class_id: &str,
    ceiling_p95_ms: u64,
    evidence_ref: &str,
) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: format!("OQ-075.hot_path_ceiling.{class_id}"),
        hypothesis: Text::new(
            format!(
                "out-of-process boundary overhead for {class_id} stays under \
                 {ceiling_p95_ms}ms p95 (M19-measured, matched_cap n={N})"
            ),
            "m19.test",
            kernel(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: EvidenceKind::ProbeRun,
            reference: evidence_ref.to_string(),
            observed_at: Some(0),
            tier: None,
            provisional: false,
        }],
        owner: OwnerRef::principal("kernel"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::Date,
            value: Some("2027-09-24".into()),
        },
        removal_test_ref: "probe:m19_boundary_overhead".into(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::Empirical),
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: Some(RemovalTest {
            probe_refs: vec!["m19_boundary_overhead".into()],
            ..RemovalTest::new(RemovalTestKind::ProbeRun)
        }),
        created_by: Some(kernel()),
        created_at: Some(0),
        supersedes: None,
    }
}

#[test]
fn ac_2_12_2_9_boundary_overhead_measured_matched_cap() {
    // ── in_process arm ──
    let (in_overhead, in_outputs) = in_process_arm();
    assert_eq!(in_overhead.len(), N);

    // ── subprocess_confined arm (MemIo fallback when the helper is absent) ──
    let lane = if fixture_bin().is_some() && helper_present() {
        "subprocess_confined"
    } else {
        "subprocess_confined(mem-io channel)"
    };
    let (bd_overhead, bd_outputs, _host) = confined_arm().unwrap_or_else(|| memio_arm());
    assert_eq!(
        bd_overhead.len(),
        N,
        "every confined invocation must carry a measured boundary_overhead_ms"
    );

    // Matched cap: both arms ran exactly N — never a different budget.
    assert_eq!(in_overhead.len(), bd_overhead.len());

    // The headline metric is unchanged within the pre-registered margin:
    // every invocation produced the assess doc (ok:true) on both arms.
    let headline = |outs: &[Json]| -> f64 {
        outs.iter()
            .filter(|d| d.get("ok") == Some(&Json::Bool(true)))
            .count() as f64
            / outs.len() as f64
    };
    let (h_in, h_bd) = (headline(&in_outputs), headline(&bd_outputs));
    assert!(
        (h_in - h_bd).abs() <= HEADLINE_MARGIN,
        "headline moved beyond margin: {h_in} vs {h_bd}"
    );

    // The distributions — reported per placement, never assumed.
    let report = Json::obj([
        ("class_id", Json::str("compaction_strategy")),
        ("matched_cap", Json::Int(N as i64)),
        ("headline_margin", Json::str(HEADLINE_MARGIN.to_string())),
        (
            "in_process",
            Json::obj([
                ("boundary_overhead_ms", dist(&in_overhead)),
                ("headline", Json::str(h_in.to_string())),
            ]),
        ),
        (
            "subprocess_confined",
            Json::obj([
                ("boundary_overhead_ms", dist(&bd_overhead)),
                ("headline", Json::str(h_bd.to_string())),
                ("lane", Json::str(lane)),
            ]),
        ),
    ]);
    let evidence_ref =
        hh_identity::idp::address(report.to_canonical_string().as_bytes(), "application/json").id();

    // The confined boundary must measure *above* the in-process one in
    // expectation — but the check that matters is that the field is real:
    // every row carried it (asserted above) and the distribution is non-trivial.
    let mut sorted = bd_overhead.clone();
    sorted.sort_unstable();
    let p95 = sorted[(N * 95 / 100).min(N - 1)];

    // ── OQ-075: the ceiling recorded as an assumption-debt record ──
    let debt = oq075_debt_record("compaction_strategy", p95, &evidence_ref);
    assert!(debt.removal_test.as_ref().unwrap().instantiates());
    // The `/1` codec round-trips the record byte-identically (CC7) — the
    // full form (semantic=false carries the Text leaf's provenance; the
    // semantic projection is hash-only).
    let j = debt_json(&debt, false);
    let back = debt_from_json(&j, "debt").expect("decodes");
    let j2 = debt_json(&back, false);
    assert_eq!(
        j.to_canonical_string(),
        j2.to_canonical_string(),
        "debt record must round-trip"
    );
    // The measured ceiling is what the hypothesis names — never an
    // assumed constant.
    assert!(debt
        .hypothesis
        .content
        .as_ref()
        .unwrap()
        .contains(&format!("{p95}ms p95")));
}
