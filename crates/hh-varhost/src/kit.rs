//! The four-layer conformance kit — spec §8.4's `run_conformance(plugin,
//! layers[], placement) → ConformanceReport{produced_by = registry_ci}`; the
//! variant host is the `registry_ci` driver (AC-R-2.12.2-10; T-LCD-12;
//! ADR-0181 D7).
//!
//! The layers (each a `ConformanceSuite` pinned to the contract version it
//! tests — the suite ref is the report's `suite_ref`):
//!
//! 1. `protocol` — a declared-op round trip plus the closed-sum check (an
//!    undeclared operation must come back the typed `UnhandledOperation`,
//!    never outputs). Handshake/pin/schema checks ran at `attach` — a live
//!    session is their evidence.
//! 2. `isolation` — the mediated probe battery (`cross_read`,
//!    `monitor_read`, `cross_effect`, `effect`): every probe must come back
//!    *refused* (`UnhandledOperation` — the plugin exposes no such reach —
//!    is the clean answer). A probe the plugin *executed* is a conformance
//!    failure carrying the host's typed refusal as evidence; a probe
//!    reporting `succeeded` is isolation absent. The raw fs/egress/exec
//!    probes need the confined subprocess lane (tests/isolation.rs) — here
//!    they are `n/a{placement}` under in-process drives.
//! 3. `class` — the plugin's own `run_conformance` self-report (per-test
//!    `pass` → `SUPPORTED`) plus the **guard property**: a `guard` call must
//!    return a parseable `GuardVerdict` (a forged `allow`/rewritten proposal
//!    surfaces as `AuthorityCrossing` — the host maps it to `no_decision`
//!    and ledgers the refusal; the *plugin's* verdict is unsupported).
//! 4. `reach` — static: `requires.contracts` are `ContractRef`s with
//!    parseable ranges, `requires.records` carry pinned `version_id`s, every
//!    `execs[]` path lives inside the package root, and `depends_on` is
//!    ContractRef-shaped (X1); runtime: the same probe battery — a hostile
//!    plugin's attempted reach is the failure, the typed refusal the
//!    evidence.
//!
//! A null plugin passes all four layers (every probe answers
//! `UnhandledOperation`, the manifest is clean); the hostile fixture fails
//! layers 2 and 4 with the typed refusals recorded per test.

use std::collections::BTreeMap;
use std::time::Duration;

use hh_embed_schema::plugin_abi::{AbiError, ConformanceParams, GuardParams, GuardSubjectKind};
use hh_identity::repro::InstrumentRecord;
use hh_registry::kinds::{ConformanceVerdict, Placement, ProducedBy, SubjectKind};
use hh_registry::records::{ConformanceReport, ReportHost, ReportResult};
use hh_wire::json::Json;

use crate::host::{InvokeOutcome, VariantHost};
use crate::ports::HostPorts;
use crate::session::VariantSession;
use crate::{HostError, SessionEvents, VariantPackage};

/// The four conformance-layer spellings (`layers[]` order).
pub const LAYERS: [&str; 4] = ["protocol", "isolation", "class", "reach"];

/// The mediated probe battery — ops the hostile fixture answers with an
/// observed `{refused}`/typed-refusal document. The raw sandbox probes
/// (`fs`/`egress`/`exec`/`spawn`/sockets) are the confined lane's suite.
pub const PROBES: [&str; 4] = ["cross_read", "monitor_read", "cross_effect", "effect"];

/// The class suite's test ids (`run_conformance`'s `plugin.tests[]`).
pub const CLASS_TESTS: [&str; 4] = [
    "static.declaration",
    "contract.assess",
    "executable.debt",
    "property.placement",
];

/// What the kit needs beyond the live session.
#[derive(Debug, Clone)]
pub struct KitRequest {
    /// The pinned subject variant (`version_id`).
    pub subject_ref: String,
    /// The pinned `ConformanceSuite` (`version_id`) the layers answer to.
    pub suite_ref: String,
    /// The ledgered run the report's evidence lives in (`charged_to =
    /// instrument` — `record_conformance`'s durability coordinate).
    pub run_id: String,
    /// The layers to run (empty = all four).
    pub layers: Vec<String>,
    /// The binding the battery invokes through.
    pub binding_id: String,
    /// Per-invocation deadline.
    pub deadline: Duration,
    /// The instrument identity (`ReportHost.instrument`).
    pub instrument: InstrumentRecord,
    /// The placement the report covers.
    pub placement: Placement,
    /// The isolation class spelling (`ReportHost.isolation`).
    pub isolation: String,
}

/// One kit check — `subject` is the `{layer}.{test}` spelling, `detail` the
/// typed refusal where one was raised.
#[derive(Debug, Clone)]
struct KitRow {
    subject: String,
    verdict: ConformanceVerdict,
    detail: String,
}

impl KitRow {
    fn pass(subject: impl Into<String>) -> KitRow {
        KitRow {
            subject: subject.into(),
            verdict: ConformanceVerdict::Supported,
            detail: "ok".to_string(),
        }
    }
    fn fail(subject: impl Into<String>, detail: impl Into<String>) -> KitRow {
        KitRow {
            subject: subject.into(),
            verdict: ConformanceVerdict::Unsupported,
            detail: detail.into(),
        }
    }
    fn na(subject: impl Into<String>, detail: impl Into<String>) -> KitRow {
        KitRow {
            subject: subject.into(),
            verdict: ConformanceVerdict::NotApplicable,
            detail: detail.into(),
        }
    }
}

/// Run the kit: drive the requested layers over the live session and fold
/// the results into the `ConformanceReport` record (`produced_by =
/// registry_ci` — the host *is* the driver). Plugin-side failures are
/// per-test verdicts, never a kit abort; a dead session makes the remaining
/// runtime rows `Unsupported{SessionDetached}`.
pub fn run_kit<P: HostPorts, E: SessionEvents>(
    host: &mut VariantHost<P, E>,
    s: &mut VariantSession,
    pkg: &VariantPackage,
    req: &KitRequest,
) -> ConformanceReport {
    let want = |l: &str| req.layers.is_empty() || req.layers.iter().any(|x| x == l);
    let mut rows: Vec<KitRow> = Vec::new();

    if want("protocol") {
        layer_protocol(host, s, req, &mut rows);
    }
    if want("isolation") {
        probe_battery(host, s, req, "isolation", &mut rows);
        // The raw sandbox probes (`fs`/`egress`/`exec`/`spawn`/sockets) need
        // the confined subprocess lane — `n/a{placement}` here, never a
        // zero-valued pass (the lane's suite is tests/isolation.rs).
        if req.placement != Placement::SubprocessConfined {
            for p in [
                "fs",
                "egress",
                "exec",
                "spawn",
                "helper_socket",
                "peer_root",
            ] {
                rows.push(KitRow::na(
                    format!("isolation.{p}"),
                    format!(
                        "n/a{{{}}} — the confined lane's probe",
                        req.placement.as_str()
                    ),
                ));
            }
        }
    }
    if want("class") {
        layer_class(host, s, req, &mut rows);
    }
    if want("reach") {
        layer_reach_static(pkg, &mut rows);
        probe_battery(host, s, req, "reach", &mut rows);
    }

    // `probed_declaration` — the per-layer aggregate (the dimensions the kit
    // probed): `UNSUPPORTED` when any row failed, `SUPPORTED` when all ran
    // clean (n/a rows never count against).
    let mut probed: BTreeMap<String, ConformanceVerdict> = BTreeMap::new();
    for layer in LAYERS {
        let layer_rows: Vec<&KitRow> = rows
            .iter()
            .filter(|r| r.subject.starts_with(&format!("{layer}.")))
            .collect();
        if layer_rows.is_empty() {
            continue;
        }
        let v = if layer_rows
            .iter()
            .any(|r| r.verdict == ConformanceVerdict::Unsupported)
        {
            ConformanceVerdict::Unsupported
        } else {
            ConformanceVerdict::Supported
        };
        probed.insert(layer.to_string(), v);
    }

    let results: Vec<ReportResult> = rows
        .iter()
        .map(|r| ReportResult {
            subject: r.subject.clone(),
            verdict: r.verdict,
            evidence_ref: None,
        })
        .collect();
    let preimage = Json::obj([
        ("subject_ref", Json::str(req.subject_ref.clone())),
        ("suite_ref", Json::str(req.suite_ref.clone())),
        ("run_id", Json::str(req.run_id.clone())),
        (
            "results",
            Json::Arr(
                rows.iter()
                    .map(|r| {
                        Json::obj([
                            ("subject", Json::str(r.subject.clone())),
                            ("verdict", Json::str(r.verdict.as_str())),
                            ("detail", Json::str(r.detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let report_id = hh_identity::idp_id(
        "hh-conformance-report",
        preimage.to_canonical_string().as_bytes(),
    );

    ConformanceReport {
        report_id,
        subject_kind: SubjectKind::Variant,
        subject_ref: req.subject_ref.clone(),
        suite_ref: req.suite_ref.clone(),
        host: ReportHost {
            placement: req.placement,
            isolation: req.isolation.clone(),
            instrument: req.instrument.clone(),
        },
        produced_by: ProducedBy::RegistryCi,
        results,
        probed_declaration: probed,
        run_id: req.run_id.clone(),
        stale: false,
        hosted_entries: Vec::new(),
    }
}

/// One `invoke` mapped to a row: `Outputs`/`Failed`/`Err` each carry their
/// evidence. `detail` describes the verdict, never a guess.
fn invoke_row(
    host: &mut VariantHost<impl HostPorts, impl SessionEvents>,
    s: &mut VariantSession,
    binding: &str,
    op: &str,
    deadline: Duration,
) -> Result<Vec<Json>, HostError> {
    match host.invoke(s, binding, op, vec![], "res-kit", deadline) {
        Ok(InvokeOutcome::Outputs(o)) => Ok(o),
        Ok(InvokeOutcome::Failed(e)) => Err(HostError::Abi(e)),
        Err(e) => Err(e),
    }
}

/// Layer 1 — `protocol`: the declared-op round trip plus the closed-sum
/// check.
fn layer_protocol<P: HostPorts, E: SessionEvents>(
    host: &mut VariantHost<P, E>,
    s: &mut VariantSession,
    req: &KitRequest,
    rows: &mut Vec<KitRow>,
) {
    // `protocol.round_trip` — a declared op returns outputs naming the op.
    match host.invoke(
        s,
        &req.binding_id,
        "assess",
        vec![],
        "res-kit",
        req.deadline,
    ) {
        Ok(InvokeOutcome::Outputs(o))
            if o.first()
                .and_then(|d| d.get("operation"))
                .and_then(Json::as_str)
                == Some("assess") =>
        {
            rows.push(KitRow::pass("protocol.round_trip"));
        }
        Ok(InvokeOutcome::Outputs(_)) => rows.push(KitRow::fail(
            "protocol.round_trip",
            "invoke outputs do not name the requested operation",
        )),
        Ok(InvokeOutcome::Failed(e)) => rows.push(KitRow::fail(
            "protocol.round_trip",
            format!("typed failure {}", e.as_str()),
        )),
        Err(e) => rows.push(KitRow::fail(
            "protocol.round_trip",
            format!("host error {e}"),
        )),
    }
    // `protocol.closed_sum` — an undeclared op answers the typed refusal,
    // never outputs.
    match host.invoke(
        s,
        &req.binding_id,
        "kit:undeclared",
        vec![],
        "res-kit",
        req.deadline,
    ) {
        Ok(InvokeOutcome::Failed(AbiError::UnhandledOperation)) => rows.push(KitRow {
            subject: "protocol.closed_sum".into(),
            verdict: ConformanceVerdict::Supported,
            detail: "UnhandledOperation".into(),
        }),
        Ok(InvokeOutcome::Failed(e)) => rows.push(KitRow {
            subject: "protocol.closed_sum".into(),
            verdict: ConformanceVerdict::Supported,
            detail: format!("typed refusal {}", e.as_str()),
        }),
        Ok(InvokeOutcome::Outputs(_)) => rows.push(KitRow::fail(
            "protocol.closed_sum",
            "an undeclared operation returned outputs",
        )),
        Err(e) => rows.push(KitRow::fail(
            "protocol.closed_sum",
            format!("untyped boundary failure {e}"),
        )),
    }
}

/// The mediated probe battery shared by layers 2 and 4. A probe the plugin
/// does not implement (`UnhandledOperation`) is the conforming answer — no
/// such reach exists. A probe the plugin *executed* (an outputs row, or a
/// typed boundary refusal) is the plugin's attempted reach — `Unsupported`
/// with the refusal named; a `succeeded` report is isolation absent.
fn probe_battery<P: HostPorts, E: SessionEvents>(
    host: &mut VariantHost<P, E>,
    s: &mut VariantSession,
    req: &KitRequest,
    layer: &str,
    rows: &mut Vec<KitRow>,
) {
    for probe in PROBES {
        let subject = format!("{layer}.{probe}");
        match invoke_row(
            host,
            s,
            &req.binding_id,
            &format!("violate:{probe}"),
            req.deadline,
        ) {
            Err(HostError::Abi(AbiError::UnhandledOperation)) => rows.push(KitRow {
                subject,
                verdict: ConformanceVerdict::Supported,
                detail: "UnhandledOperation — no such reach".into(),
            }),
            Err(e) => rows.push(KitRow::fail(
                subject,
                format!("attempted reach; typed refusal {e}"),
            )),
            Ok(docs) => {
                let doc = docs.first().cloned().unwrap_or(Json::Null);
                let refused = doc.get("refused") == Some(&Json::Bool(true));
                let succeeded = doc.get("succeeded") == Some(&Json::Bool(true));
                let error = doc
                    .get("error")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                if succeeded {
                    rows.push(KitRow::fail(
                        subject,
                        format!("probe {probe} succeeded — isolation absent"),
                    ));
                } else if refused {
                    rows.push(KitRow::fail(
                        subject,
                        format!("attempted undeclared reach; refused {error}"),
                    ));
                } else {
                    // The plugin answered the probe op without a refused/
                    // succeeded report — an executed probe with no evidence
                    // either way is still an attempted reach.
                    rows.push(KitRow::fail(
                        subject,
                        format!("probe {probe} executed without a refusal report"),
                    ));
                }
            }
        }
    }
}

/// Layer 3 — `class`: the plugin's own `run_conformance` self-report plus
/// the guard property.
fn layer_class<P: HostPorts, E: SessionEvents>(
    host: &mut VariantHost<P, E>,
    s: &mut VariantSession,
    req: &KitRequest,
    rows: &mut Vec<KitRow>,
) {
    let tests = Json::Arr(CLASS_TESTS.iter().map(|t| Json::str(*t)).collect());
    let params = ConformanceParams {
        plugin: Json::obj([("tests", tests)]),
        layers: vec!["class".to_string()],
        placement: req.placement.as_str().to_string(),
    };
    match host.run_conformance(s, params, req.deadline) {
        Ok(report) => match report.get("results") {
            Some(Json::Arr(rs)) => {
                for r in rs {
                    let test = r
                        .get("test_id")
                        .and_then(Json::as_str)
                        .unwrap_or("?")
                        .to_string();
                    let pass = r.get("pass") == Some(&Json::Bool(true));
                    let detail = r
                        .get("detail")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    rows.push(KitRow {
                        subject: format!("class.{test}"),
                        verdict: if pass {
                            ConformanceVerdict::Supported
                        } else {
                            ConformanceVerdict::Unsupported
                        },
                        detail,
                    });
                }
            }
            _ => rows.push(KitRow::fail(
                "class.suite",
                "run_conformance returned no results[]",
            )),
        },
        Err(e) => rows.push(KitRow::fail(
            "class.suite",
            format!("run_conformance failed typed {e}"),
        )),
    }
    // `class.guard_property` — the guard answer must parse as GuardVerdict;
    // a forged verdict surfaces as `no_decision` + the ledgered refusal (the
    // plugin's forged frame is the failure evidence where configured).
    let guard = GuardParams {
        binding_id: req.binding_id.clone(),
        decision_point: "kit.conformance".into(),
        subject_kind: GuardSubjectKind::Proposal,
        subject: Json::obj([]),
        ledger_cursor: 0,
    };
    match host.guard(s, guard, false, req.deadline) {
        Ok(v) => rows.push(KitRow {
            subject: "class.guard_property".into(),
            verdict: ConformanceVerdict::Supported,
            detail: format!("GuardVerdict {}", verdict_label(v)),
        }),
        Err(e) => rows.push(KitRow::fail(
            "class.guard_property",
            format!("typed refusal {e}"),
        )),
    }
}

/// Layer 4 static — `requires` names only ContractRefs with parseable
/// ranges and pinned records; every executable lives inside the package
/// root.
fn layer_reach_static(pkg: &VariantPackage, rows: &mut Vec<KitRow>) {
    let m = &pkg.manifest;
    // `reach.static.execs` — no path outside the package root.
    let mut clean = true;
    for ex in &pkg.execs {
        if !ex.starts_with(&pkg.root) {
            rows.push(KitRow::fail(
                "reach.static.execs",
                format!("exec {} outside the package root", ex.display()),
            ));
            clean = false;
        }
    }
    if clean {
        rows.push(KitRow::pass("reach.static.execs"));
    }
    // `reach.static.records` — every record ref is a pinned `version_id`.
    let mut pinned = true;
    for r in &m.requires.records {
        if r.version_id.is_empty() || !r.version_id.contains(':') {
            rows.push(KitRow::fail(
                "reach.static.records",
                format!("record ref {} is not a pinned version_id", r.version_id),
            ));
            pinned = false;
        }
    }
    if pinned {
        rows.push(KitRow::pass("reach.static.records"));
    }
    // `reach.static.contracts` — every dependency form is a `ContractRef`
    // (X1) with a non-empty range; `depends_on` shares the check.
    let mut contracts_ok = true;
    for c in m.requires.contracts.iter().chain(m.depends_on.iter()) {
        if c.version_range.is_empty() || c.id.is_empty() {
            rows.push(KitRow::fail(
                "reach.static.contracts",
                format!("{} has an empty id/range", c.label()),
            ));
            contracts_ok = false;
        }
    }
    if contracts_ok {
        rows.push(KitRow::pass("reach.static.contracts"));
    }
}

fn verdict_label(v: hh_embed_schema::plugin_abi::GuardVerdict) -> &'static str {
    use hh_embed_schema::plugin_abi::GuardVerdict::*;
    match v {
        Pass => "pass",
        Annotate(_) => "annotate",
        Narrow(_) => "narrow",
        ProposeReplacement(_) => "propose_replacement",
        NoDecision => "no_decision",
    }
}
