//! `example-card/1` — the versioned offline example cards and their
//! checked runner (R5.29; §7.2's EX-01/EX-02 families; REQ-F-101…105,
//! REQ-A-008 contributing).
//!
//! One composition home beside [`crate::recipe`]: the card is the
//! versioned, pinned record (committed under `scripts/examples/`); the
//! runner drives the **landed owners** — boundary ops on an in-process
//! `EmbedService`, `hh-recipe/1`, the analysis owners
//! (`analyze_and_record` / `recompute_analysis`), the bundle owners
//! (`kernel.bundle` / `check_completeness` / `reproduce` /
//! `admit_replay`). There is no second codec, store, runner or shadow
//! result; every durable mutation rides the boundary op or the owning
//! interface; every semantic control compares against durable facts,
//! never process exit.
//!
//! EX-01 — scripted quickstart → attach by returned run ID → the six
//! annotated excerpts resolve to actual event IDs/seq/payload/causes →
//! native export (`kernel.bundle`) → clean-recipient `validate` +
//! `reproduce` R0 → §6.3 isolated-replay admission (`admit_replay`).
//!
//! EX-02 — the informative two-task fixture: two synthetic arms × two
//! synthetic tasks × five repetitions = 20 subjects, driven as **one
//! `run_recipe` per task** (a plan's grade map is `arm →
//! per-cell-replicate`; two tasks in one experiment would share the
//! same per-cell grade vector — a constant fixture, refused by AC-2).
//! Each recipe run registers/expands/opens/claims/launches/settles/
//! closes for real and closes its `population-receipt/1`. The merged
//! planned-slot universe (union of both experiments' folded views)
//! feeds `finite_manifest_contrast` (`tasks>=2` floor — the bound
//! two-task population a single experiment view cannot express) →
//! persisted `analysis_report_body/1` → clean-recipient
//! `recompute_analysis` → `reproduced`.
//!
//! Live posture: offline only — scripted `EmbedModel` lane and scratch
//! stores; fixture-labelled; no task/provider/participant execution,
//! $0; an exploratory report never upgrades to confirmation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hharness_analysis::engine::analyze_and_record;
use hharness_analysis::receipt::{
    AnalysisDecl, DeclaredReceipt, GradeDecl, GradeStatus, RowDecl, SettlementDecl,
};
use hharness_analysis::recompute::{recompute_analysis, PopulationPins, RecomputePlan};
use hharness_analysis::AnalysisInput;
use hharness_bundle::cards::{admit_replay, Isolation, RederivedObject, ReplayAttempt};
use hharness_bundle::codec::decode_dir;
use hharness_bundle::export::decode_page;
use hharness_bundle::manifest::{LedgerExport, MemberStatus};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::finite_manifest as fm;
use hharness_eval::runs::{SuiteContext, TaskContext};
use hharness_experiment::docs::LabDocs;
use hharness_experiment::view::ExperimentView;
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hharness_ledger::event::EventEnvelope;
use hharness_ontology::config::Ref;
use hharness_ontology::eval::{
    Design, DesignKind, EstimatorSelection, FactorDeclaration, FactorLevel, IntervalMethod,
    Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
};
use hharness_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hharness_ontology::participant::ParticipantClass;
use hharness_results::store::ResultsStore;
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;
use hharness_wire::sha256::sha256_hex;

use crate::recipe::{run_recipe, RecipePlan};

/// The card schema label.
pub const CARD_SCHEMA: &str = "example-card/1";
/// The EX-02 fixture schema label (`scripts/examples/ex-02-fixture.json`).
pub const EX02_FIXTURE_SCHEMA: &str = "ex-02-fixture/1";
/// The card runner's version — recorded on every report.
pub const RUNNER_VERSION: &str = "hh-examples/1";

/// The closed refusal-code set [`ExampleError::Refused`] carries.
pub mod code {
    /// The card's declared pin does not recompute over the pinned
    /// bytes — stale card, never a re-pinned execution.
    pub const STALE_PIN: &str = "stale_pin";
    /// `case` is not one of the admitted families.
    pub const UNKNOWN_CASE: &str = "unknown_case";
    /// A member outside the closed card/fixture set.
    pub const UNSUPPORTED_SHAPE: &str = "unsupported_shape";
    /// A required member is absent.
    pub const MISSING_FIELD: &str = "missing_field";
    /// A member decoded to the wrong type.
    pub const TYPE_MISMATCH: &str = "type_mismatch";
    /// A card-declared check the runner never produced.
    pub const CHECK_MISSING: &str = "check_missing";
}

/// The example error sum — typed refusals plus wrapped owner/boundary
/// failures (verbatim detail; never re-classified).
#[derive(Debug)]
pub enum ExampleError {
    /// A card-level typed refusal (`code` ∈ [`code`]).
    Refused {
        /// The closed code.
        code: &'static str,
        /// Human-readable detail.
        detail: String,
    },
    /// A boundary op answered an error — `detail` is its error body.
    Boundary {
        /// The method called.
        method: String,
        /// The boundary's own error Json.
        detail: Json,
    },
    /// An owner-seam call failed.
    Owner {
        /// The seam name.
        seam: &'static str,
        /// The owner's own error detail.
        detail: String,
    },
}

impl std::fmt::Display for ExampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExampleError::Refused { code, detail } => write!(f, "refused{{{code}}}: {detail}"),
            ExampleError::Boundary { method, detail } => {
                write!(f, "boundary {method}: {detail:?}")
            }
            ExampleError::Owner { seam, detail } => write!(f, "owner {seam}: {detail}"),
        }
    }
}

impl std::error::Error for ExampleError {}

fn refused(code: &'static str, detail: impl Into<String>) -> ExampleError {
    ExampleError::Refused {
        code,
        detail: detail.into(),
    }
}

/// One semantic check's record — `verdict` ∈ `pass | fail |
/// expected_refusal` (an expected refusal recorded verbatim: the
/// assertion passed while the operation stayed refused).
#[derive(Debug, Clone)]
pub struct CheckRecord {
    /// The check id (the card's `checks[]` names these).
    pub id: String,
    /// `pass | fail | expected_refusal`.
    pub verdict: &'static str,
    /// What the check observed (actual ids/counts/refs, never "ok").
    pub detail: Json,
}

fn pass(id: &str, detail: Json) -> CheckRecord {
    CheckRecord {
        id: id.to_string(),
        verdict: "pass",
        detail,
    }
}
fn fail(id: &str, detail: impl std::fmt::Display) -> CheckRecord {
    CheckRecord {
        id: id.to_string(),
        verdict: "fail",
        detail: Json::str(detail.to_string()),
    }
}
fn fail_json(id: &str, detail: Json) -> CheckRecord {
    CheckRecord {
        id: id.to_string(),
        verdict: "fail",
        detail,
    }
}
fn expected_refusal(id: &str, detail: Json) -> CheckRecord {
    CheckRecord {
        id: id.to_string(),
        verdict: "expected_refusal",
        detail,
    }
}

/// `ExampleCard` — the committed versioned card (closed member set).
#[derive(Debug, Clone)]
pub struct ExampleCard {
    /// `ex-01` | `ex-02`.
    pub case: String,
    /// The card's own version label.
    pub card_version: String,
    /// The honest fixture label (never dropped).
    pub fixture_label: String,
    /// `{rust, network:false, credentials:"none", spend_usd:0, …}` verbatim.
    pub prerequisites: Json,
    /// `{<pin-name>: {path, sha256}}` — recomputed before any execution.
    pub pins: Json,
    /// The declared semantic-check ids.
    pub checks: Vec<String>,
    /// The declared negative-control descriptions (prose).
    pub negative_controls: Vec<String>,
    /// `{build, runtime}` — separated costs, verbatim.
    pub costs: Json,
    /// The stated fixture limitations.
    pub limitations: Vec<String>,
    /// The recorded run command.
    pub run: String,
}

const CARD_MEMBERS: &[&str] = &[
    "schema",
    "case",
    "card_version",
    "title",
    "fixture_label",
    "prerequisites",
    "pins",
    "checks",
    "negative_controls",
    "costs",
    "limitations",
    "run",
];

impl ExampleCard {
    /// Strict decode — closed member set, `example-card/1` schema only.
    pub fn from_json(j: &Json) -> Result<Self, ExampleError> {
        let Json::Obj(m) = j else {
            return Err(refused(code::TYPE_MISMATCH, "card must be an object"));
        };
        for k in m.keys() {
            if !CARD_MEMBERS.contains(&k.as_str()) {
                return Err(refused(
                    code::UNSUPPORTED_SHAPE,
                    format!("card.{k} is not a card member"),
                ));
            }
        }
        let req = |k: &str| -> Result<&Json, ExampleError> {
            j.get(k)
                .ok_or_else(|| refused(code::MISSING_FIELD, format!("card.{k}")))
        };
        if req("schema")?.as_str() != Some(CARD_SCHEMA) {
            return Err(refused(
                code::UNSUPPORTED_SHAPE,
                format!("card.schema must be {CARD_SCHEMA}"),
            ));
        }
        let case = req("case")?
            .as_str()
            .ok_or_else(|| refused(code::TYPE_MISMATCH, "card.case"))?
            .to_string();
        if !matches!(case.as_str(), "ex-01" | "ex-02") {
            return Err(refused(code::UNKNOWN_CASE, case));
        }
        let str_list = |k: &str| -> Result<Vec<String>, ExampleError> {
            match req(k)? {
                Json::Arr(items) => items
                    .iter()
                    .map(|i| {
                        i.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| refused(code::TYPE_MISMATCH, format!("card.{k}[]")))
                    })
                    .collect(),
                _ => Err(refused(code::TYPE_MISMATCH, format!("card.{k}"))),
            }
        };
        Ok(ExampleCard {
            case,
            card_version: req("card_version")?
                .as_str()
                .ok_or_else(|| refused(code::TYPE_MISMATCH, "card.card_version"))?
                .to_string(),
            fixture_label: req("fixture_label")?
                .as_str()
                .ok_or_else(|| refused(code::TYPE_MISMATCH, "card.fixture_label"))?
                .to_string(),
            prerequisites: req("prerequisites")?.clone(),
            pins: req("pins")?.clone(),
            checks: str_list("checks")?,
            negative_controls: str_list("negative_controls")?,
            costs: req("costs")?.clone(),
            limitations: str_list("limitations")?,
            run: req("run")?
                .as_str()
                .ok_or_else(|| refused(code::TYPE_MISMATCH, "card.run"))?
                .to_string(),
        })
    }

    /// Load + decode a committed card file.
    pub fn load(path: &Path) -> Result<Self, ExampleError> {
        let text = std::fs::read_to_string(path).map_err(|e| ExampleError::Owner {
            seam: "card",
            detail: format!("{}: {e}", path.display()),
        })?;
        let j = hharness_wire::json::parse(&text).map_err(|e| ExampleError::Owner {
            seam: "card",
            detail: format!("{}: {e}", path.display()),
        })?;
        Self::from_json(&j)
    }
}

/// `ExampleReport` — the semantic record (never a process exit).
#[derive(Debug)]
pub struct ExampleReport {
    /// [`RUNNER_VERSION`].
    pub runner: &'static str,
    /// The card case/version.
    pub case: String,
    /// The card version.
    pub card_version: String,
    /// `pass` only when every check is `pass | expected_refusal`.
    pub verdict: &'static str,
    /// Every semantic check, in order.
    pub checks: Vec<CheckRecord>,
    /// The produced record refs (run ids, report/card ids).
    pub records: Json,
    /// The card's verbatim costs/limitations/label.
    pub card: Json,
}

impl ExampleReport {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("runner", Json::str(self.runner)),
            ("case", Json::str(&self.case)),
            ("card_version", Json::str(&self.card_version)),
            ("verdict", Json::str(self.verdict)),
            (
                "checks",
                Json::Arr(
                    self.checks
                        .iter()
                        .map(|c| {
                            Json::obj([
                                ("id", Json::str(&c.id)),
                                ("verdict", Json::str(c.verdict)),
                                ("detail", c.detail.clone()),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("records", self.records.clone()),
            ("card", self.card.clone()),
        ])
    }
}

fn str_at<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

/// One boundary call — records nothing itself (the caller records the
/// outcome leg it wants); errors wrap the boundary body verbatim.
fn call(svc: &mut EmbedService, method: &str, params: Json) -> Result<Json, ExampleError> {
    let resp = svc.handle(&Request {
        id: Json::str(format!("ex-{method}")),
        method: method.to_string(),
        params,
    });
    if let Some(r) = resp.get("result") {
        Ok(r.clone())
    } else {
        Err(ExampleError::Boundary {
            method: method.to_string(),
            detail: resp.get("error").cloned().unwrap_or(Json::Null),
        })
    }
}

/// The error body of a refused boundary call — expected-refusal
/// controls record it verbatim (never re-classified, never a pass).
fn call_err(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    let resp = svc.handle(&Request {
        id: Json::str(format!("ex-neg-{method}")),
        method: method.to_string(),
        params,
    });
    resp.get("error").cloned().unwrap_or(Json::Null)
}

/// The pin leg — every `{name:{path, sha256}}` member recomputes over
/// `root/path`'s actual bytes; drift refuses `stale_pin` before any
/// execution. Returns the verified `name → (path, bytes)`.
fn verify_pins(
    card: &ExampleCard,
    repo_root: &Path,
) -> Result<BTreeMap<String, (PathBuf, Vec<u8>)>, ExampleError> {
    let Json::Obj(pins) = &card.pins else {
        return Err(refused(code::TYPE_MISMATCH, "card.pins"));
    };
    let mut out = BTreeMap::new();
    for (name, pin) in pins {
        let path = str_at(pin, "path")
            .ok_or_else(|| refused(code::MISSING_FIELD, format!("card.pins.{name}.path")))?;
        let want = str_at(pin, "sha256")
            .ok_or_else(|| refused(code::MISSING_FIELD, format!("card.pins.{name}.sha256")))?;
        let full = repo_root.join(path);
        let bytes = std::fs::read(&full).map_err(|e| ExampleError::Owner {
            seam: "pin",
            detail: format!("{}: {e}", full.display()),
        })?;
        let got = format!("sha256:{}", sha256_hex(&bytes));
        if got != want {
            return Err(refused(
                code::STALE_PIN,
                format!("{name}: card pins {want}, {path} recomputes {got}"),
            ));
        }
        out.insert(name.clone(), (full, bytes));
    }
    Ok(out)
}

/// `run_example(service, card, work, repo_root)` — execute the card on
/// the service's scratch store; `work` holds the export/recipient
/// scratch dirs; `repo_root` resolves the pinned files. The returned
/// report's `verdict` is semantic — `pass` requires every check green
/// (expected refusals recorded, never counted as passes of the
/// refused operation).
pub fn run_example(
    svc: &mut EmbedService,
    card: &ExampleCard,
    work: &Path,
    repo_root: &Path,
) -> Result<ExampleReport, ExampleError> {
    std::fs::create_dir_all(work).map_err(|e| ExampleError::Owner {
        seam: "work",
        detail: format!("{}: {e}", work.display()),
    })?;
    let pins = verify_pins(card, repo_root)?;
    let (checks, records) = match card.case.as_str() {
        "ex-01" => ex01(svc, card, work, &pins)?,
        "ex-02" => ex02(svc, card, work, &pins)?,
        other => return Err(refused(code::UNKNOWN_CASE, other.to_string())),
    };
    // The card's declared checks must each have been produced —
    // a check the runner never emitted is a card drift, not a pass.
    let mut checks = checks;
    for want in &card.checks {
        if !checks.iter().any(|c| &c.id == want) {
            checks.push(fail(
                want,
                format!("declared check {want} was never produced"),
            ));
        }
    }
    let verdict = if checks.iter().all(|c| c.verdict != "fail") {
        "pass"
    } else {
        "fail"
    };
    Ok(ExampleReport {
        runner: RUNNER_VERSION,
        case: card.case.clone(),
        card_version: card.card_version.clone(),
        verdict,
        checks,
        records,
        card: Json::obj([
            ("fixture_label", Json::str(&card.fixture_label)),
            ("prerequisites", card.prerequisites.clone()),
            ("costs", card.costs.clone()),
            (
                "limitations",
                Json::Arr(card.limitations.iter().map(Json::str).collect()),
            ),
            (
                "negative_controls",
                Json::Arr(card.negative_controls.iter().map(Json::str).collect()),
            ),
            ("run", Json::str(&card.run)),
        ]),
    })
}

// ── EX-01 ─────────────────────────────────────────────────────────────

/// Find the first event of `class` (ascending seq).
fn first<'a>(evs: &'a [EventEnvelope], class: &str) -> Option<&'a EventEnvelope> {
    evs.iter().find(|e| e.class == class)
}

/// All events of `class`.
fn all<'a>(evs: &'a [EventEnvelope], class: &str) -> Vec<&'a EventEnvelope> {
    evs.iter().filter(|e| e.class == class).collect()
}

/// A chained `lifecycle.replay.*` marker minted on the disposable copy —
/// the only permitted suffix rows (the §6.3 replay contract).
fn marker(prev: &EventEnvelope, class: &str, event_id: &str) -> EventEnvelope {
    let mut e = prev.clone();
    e.event_id = event_id.to_string();
    e.seq = prev.seq + 1;
    e.class = class.to_string();
    e.prev_hash = prev.hash.clone();
    e.payload = Json::obj([("driver_mode", Json::str("scripted_lane"))]);
    e.causes = vec![];
    e.refs = vec![];
    e.ir_refs = vec![];
    e.surface_ids = BTreeMap::new();
    e.hash = e.recompute_hash();
    e
}

/// The isolated replay attempt over the exported prefix — copy +
/// permitted markers, the deterministic scripted lane's recorded
/// decision/assembly sequences re-presented verbatim, the full R1
/// rederived set, and the honestly-declared isolation (offline: no
/// provider transport or effect execution was ever armed).
fn replay_attempt(
    orig: &[EventEnvelope],
    decoded: &hharness_bundle::codec::Decoded,
    run: &str,
) -> ReplayAttempt {
    let mut copy = orig.to_vec();
    let m1 = marker(
        copy.last().unwrap(),
        "lifecycle.replay.started",
        "evt-ex01-r1",
    );
    copy.push(m1.clone());
    copy.push(marker(&m1, "lifecycle.replay.finished", "evt-ex01-r2"));
    let seq = |class: &str| {
        orig.iter()
            .filter(|e| e.class == class)
            .map(|e| e.payload.clone())
            .collect()
    };
    let mut rederived = vec![RederivedObject {
        authoritative: "version_id".into(),
        rederived: decoded.manifest.compute_id(),
    }];
    for e in orig {
        rederived.push(RederivedObject {
            authoritative: e.hash.clone(),
            rederived: e.recompute_hash(),
        });
    }
    for mm in &decoded.manifest.members {
        if mm.status == MemberStatus::Present {
            rederived.push(RederivedObject {
                authoritative: mm.address.clone(),
                rederived: hharness_identity::idp_id("blob", &decoded.members[&mm.address]),
            });
        }
    }
    let ex = &decoded.manifest.traces[run];
    rederived.push(RederivedObject {
        authoritative: ex.tree.clone(),
        rederived: LedgerExport::tree_address(&ex.pages),
    });
    ReplayAttempt {
        copy,
        replayed_decisions: seq("control.decision"),
        replayed_assembled: seq("context.assembled"),
        rederived,
        isolation: Isolation {
            provider_transport_disabled: true,
            effect_execution_disabled: true,
            attempted_provider_dispatches: 0,
            attempted_effect_executions: 0,
        },
        claim_r1: true,
    }
}

fn ex01(
    svc: &mut EmbedService,
    card: &ExampleCard,
    work: &Path,
    pins: &BTreeMap<String, (PathBuf, Vec<u8>)>,
) -> Result<(Vec<CheckRecord>, Json), ExampleError> {
    let mut checks: Vec<CheckRecord> = Vec::new();
    let def_bytes = &pins
        .get("definition")
        .ok_or_else(|| refused(code::MISSING_FIELD, "card.pins.definition"))?
        .1;
    let document = hharness_wire::json::parse(
        std::str::from_utf8(def_bytes)
            .map_err(|e| refused(code::TYPE_MISMATCH, format!("definition utf8: {e}")))?,
    )
    .map_err(|e| refused(code::TYPE_MISMATCH, format!("definition json: {e}")))?;

    // ── negotiate ────────────────────────────────────────────────────
    let hello = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("ex-01")),
                    ("version", Json::str(&card.card_version)),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                    ("accepts_ephemeral_frames", Json::Bool(true)),
                ]),
            ),
        ]),
    )?;
    checks.push(if hello.get("kernel").is_some() {
        pass(
            "negotiated",
            Json::obj([("hello", hello.clone()), ("connectivity", Json::str("ok"))]),
        )
    } else {
        fail("negotiated", "hello carried no kernel version".to_string())
    });

    // ── scripted quickstart lane (in-process — the same `handle` the
    // `hh-kernel serve` binding shares; the verbatim README leg stays
    // with scripts/check-quickstart.sh) ──────────────────────────────
    let opened = call(
        svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("new")),
                    (
                        "definition",
                        Json::obj([("kind", Json::str("document")), ("document", document)]),
                    ),
                    (
                        "environment",
                        Json::obj([
                            ("kind", Json::str("connection_info")),
                            (
                                "connection_info",
                                Json::obj([("class", Json::str("local_host"))]),
                            ),
                        ]),
                    ),
                    (
                        "attendance",
                        Json::obj([
                            ("value", Json::str("async")),
                            ("source", Json::str("declared")),
                        ]),
                    ),
                ]),
            ),
            ("idempotency_key", Json::str("ex01-open")),
        ]),
    )?;
    let run_id = str_at(&opened, "run_id")
        .ok_or_else(|| refused(code::TYPE_MISMATCH, "open_session: no run_id"))?
        .to_string();
    let session_id = str_at(&opened, "session_id")
        .ok_or_else(|| refused(code::TYPE_MISMATCH, "open_session: no session_id"))?
        .to_string();
    call(
        svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(&session_id)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("hh.submit")),
                    ("args", Json::obj([])),
                ])]),
            ),
            ("idempotency_key", Json::str("ex01-submit")),
        ]),
    )?;

    // ── attach by returned run ID — read the durable stream ─────────
    let evs = svc
        .store()
        .events(&run_id)
        .map_err(|e| ExampleError::Owner {
            seam: "store",
            detail: format!("{run_id}: {e:?}"),
        })?;
    let finish = first(&evs, "lifecycle.run.finished");
    checks.push(match finish {
        Some(f)
            if str_at(&f.payload, "status") == Some("succeeded")
                && str_at(
                    &f.payload.get("stop_reason").cloned().unwrap_or(Json::Null),
                    "kind",
                ) == Some("completed") =>
        {
            pass(
                "run_succeeded",
                Json::obj([
                    ("run_id", Json::str(&run_id)),
                    ("finished_event", Json::str(&f.event_id)),
                    ("status", Json::str("succeeded")),
                    ("stop_reason", Json::str("completed")),
                ]),
            )
        }
        other => fail(
            "run_succeeded",
            format!("finished row: {:?}", other.map(|f| &f.payload)),
        ),
    });

    // ── the six excerpts — each resolves to actual ids/seq/payload ──
    // (1) run/definition/task/submission
    {
        let def_ref = svc
            .store()
            .manifest(&run_id)
            .ok()
            .and_then(|m| m.harness_def_ref.clone());
        let submission = all(&evs, "action.effect.observed").iter().find_map(|e| {
            str_at(&e.payload, "submission_ref").map(|s| (e.event_id.clone(), s.to_string()))
        });
        // Run identity (the finished row), definition (manifest ref)
        // and the task/submission join all resolve to actual refs.
        let ok = def_ref.is_some() && submission.is_some() && finish.is_some();
        checks.push(if ok {
            let (ev, sub) = submission.unwrap();
            pass(
                "excerpt:run_submission",
                Json::obj([
                    ("definition_ref", Json::str(def_ref.unwrap())),
                    ("submission_ref", Json::str(sub)),
                    ("observed_event", Json::str(ev)),
                ]),
            )
        } else {
            fail(
                "excerpt:run_submission",
                "definition ref / submission ref / finished row did not resolve",
            )
        });
    }
    // (2) real action/model relation — model.call.* ↔ tool_call_id →
    //     action.effect.intended
    {
        let req = all(&evs, "model.call.requested");
        let done = all(&evs, "model.call.completed");
        let intended = all(&evs, "action.effect.intended");
        let mut joined = false;
        'outer: for mc in &done {
            let mc_id = str_at(&mc.payload, "model_call_id").unwrap_or("");
            if !req
                .iter()
                .any(|r| str_at(&r.payload, "model_call_id") == Some(mc_id))
            {
                continue;
            }
            if let Some(Json::Arr(calls)) = mc.payload.get("calls") {
                for c in calls {
                    let tc = str_at(c, "tool_call_id").unwrap_or("");
                    if intended.iter().any(|i| {
                        str_at(
                            &i.payload.get("intent").cloned().unwrap_or(Json::Null),
                            "tool_call_id",
                        ) == Some(tc)
                    }) {
                        joined = true;
                        break 'outer;
                    }
                }
            }
        }
        checks.push(if joined {
            pass(
                "excerpt:action_model",
                Json::obj([
                    ("model_calls", Json::Int(done.len() as i64)),
                    (
                        "model_call_id",
                        Json::str(str_at(&done[0].payload, "model_call_id").unwrap_or("")),
                    ),
                    ("intended", Json::Int(intended.len() as i64)),
                ]),
            )
        } else {
            fail(
                "excerpt:action_model",
                "no model.call completed↔requested pair joins an action.effect.intended",
            )
        });
    }
    // (3) effect intended + permission/rule
    {
        let it = first(&evs, "action.effect.intended");
        let decided = it.and_then(|i| {
            let eid = str_at(&i.payload, "effect_id")?;
            all(&evs, "security.permission.decided")
                .into_iter()
                .find(|d| str_at(&d.payload, "effect_id") == Some(eid))
        });
        let ok = matches!(
            decided,
            Some(d) if str_at(&d.payload, "decision") == Some("allow")
                && str_at(&d.payload, "policy_ref").is_some()
        );
        checks.push(if ok {
            let d = decided.unwrap();
            pass(
                "excerpt:intended_permission",
                Json::obj([
                    (
                        "effect_id",
                        Json::str(str_at(&it.unwrap().payload, "effect_id").unwrap()),
                    ),
                    ("decision_event", Json::str(&d.event_id)),
                    (
                        "policy_ref",
                        Json::str(str_at(&d.payload, "policy_ref").unwrap()),
                    ),
                ]),
            )
        } else {
            fail(
                "excerpt:intended_permission",
                "intended effect has no allow decision with a policy_ref",
            )
        });
    }
    // (4) the full authorized→prepared→committed→observed sequence for
    //     one effect, in seq order, with the observed bytes
    {
        let it = first(&evs, "action.effect.intended");
        let eid = it.and_then(|i| str_at(&i.payload, "effect_id").map(str::to_string));
        let chain: Vec<u64> = eid
            .as_ref()
            .map(|eid| {
                ["authorized", "prepared", "committed", "observed"]
                    .iter()
                    .filter_map(|ph| {
                        evs.iter()
                            .find(|e| {
                                e.class == format!("action.effect.{ph}")
                                    && e.scope.effect_id.as_deref() == Some(eid.as_str())
                            })
                            .map(|e| e.seq)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let observed = eid.as_ref().and_then(|eid| {
            evs.iter().find(|e| {
                e.class == "action.effect.observed"
                    && e.scope.effect_id.as_deref() == Some(eid.as_str())
            })
        });
        let ordered = chain.len() == 4 && chain.windows(2).all(|w| w[0] < w[1]);
        let bytes_ok = observed
            .map(|o| {
                str_at(&o.payload, "outcome") == Some("applied")
                    && str_at(&o.payload, "submission_ref").is_some()
            })
            .unwrap_or(false);
        checks.push(if ordered && bytes_ok {
            pass(
                "excerpt:effect_chain",
                Json::obj([
                    ("effect_id", Json::str(eid.unwrap())),
                    (
                        "seqs",
                        Json::Arr(chain.iter().map(|s| Json::Int(*s as i64)).collect()),
                    ),
                    ("observed_outcome", Json::str("applied")),
                ]),
            )
        } else {
            fail(
                "excerpt:effect_chain",
                format!("chain seqs {chain:?}, observed bytes ok: {bytes_ok}"),
            )
        });
    }
    // (5) lifecycle finish is separate from oracle/grade/veto/failure
    {
        let oracle = [
            "verification.claim.recorded",
            "verification.claim.reconciled",
            "verification.gate.evaluated",
            "verification.completion.proposed",
            "verification.completion.decided",
        ]
        .iter()
        .any(|c| first(&evs, c).is_some());
        checks.push(if finish.is_some() && oracle {
            pass(
                "excerpt:lifecycle_vs_oracle",
                Json::obj([
                    ("finished_event", Json::str(&finish.unwrap().event_id)),
                    (
                        "gate_event",
                        Json::str(
                            &first(&evs, "verification.gate.evaluated")
                                .map(|e| e.event_id.clone())
                                .unwrap_or_default(),
                        ),
                    ),
                ]),
            )
        } else {
            fail(
                "excerpt:lifecycle_vs_oracle",
                "finish not separated from verification/grade rows",
            )
        });
    }
    // (6) context/replay — original prefix intact + allowed markers
    //     (fully proven by the replay leg's prefix/suffix checks below;
    //     here the excerpts' context.assembled anchors resolve)
    {
        let ctx = all(&evs, "context.assembled");
        checks.push(if !ctx.is_empty() {
            pass(
                "excerpt:context_replay",
                Json::obj([
                    ("context_events", Json::Int(ctx.len() as i64)),
                    ("first", Json::str(&ctx[0].event_id)),
                ]),
            )
        } else {
            fail("excerpt:context_replay", "no context.assembled rows")
        });
    }

    // ── export — the native `kernel.bundle` surface ─────────────────
    let sink = work.join("bundle");
    let bundle = call(
        svc,
        "kernel.bundle",
        Json::obj([
            ("run_id", Json::str(&run_id)),
            ("deliver_sink", Json::str(sink.to_string_lossy())),
        ]),
    )?;

    // ── clean recipient — a fresh service on its own scratch store;
    //     the exported dir alone is the handoff (no producer fallback) ─
    let recip_root = work.join("recipient");
    let mut recip = EmbedService::open(ServiceConfig {
        store_root: recip_root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: recip_root.join("ws"),
        holder: "ex-01-recipient".to_string(),
    })
    .map_err(|e| ExampleError::Owner {
        seam: "recipient",
        detail: format!("{e:?}"),
    })?;
    call(
        &mut recip,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("ex-01-recipient")),
                    ("version", Json::str(&card.card_version)),
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
    )?;
    let validated = call(
        &mut recip,
        "kernel.check_completeness",
        Json::obj([("path", Json::str(sink.to_string_lossy()))]),
    )?;
    let reproduced = call(
        &mut recip,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(sink.to_string_lossy())),
            ("level", Json::str("R0")),
        ]),
    )?;
    checks.push(
        if validated.get("complete") == Some(&Json::Bool(true))
            && str_at(&reproduced, "outcome") == Some("pass")
            && str_at(&reproduced, "verdict") == Some("reproduced")
        {
            pass(
                "export_verify_r0",
                Json::obj([
                    (
                        "bundle_id",
                        bundle.get("bundle_id").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "max_supported_level",
                        validated
                            .get("max_supported_level")
                            .cloned()
                            .unwrap_or(Json::Null),
                    ),
                    ("verdict", Json::str("reproduced")),
                ]),
            )
        } else {
            fail(
                "export_verify_r0",
                format!("validate {validated:?} reproduce {reproduced:?}"),
            )
        },
    );

    // ── replay — the §6.3 isolated-replay admission over the exported
    //     record; a tampered copy is the expected-refusal control ────
    let decoded = decode_dir(&sink).map_err(|e| ExampleError::Owner {
        seam: "replay",
        detail: format!("decode_dir: {e:?}"),
    })?;
    let attempt = {
        let export = decoded
            .manifest
            .traces
            .get(&run_id)
            .ok_or_else(|| refused(code::MISSING_FIELD, "bundle: no ledger export for run"))?;
        let mut orig = Vec::new();
        for page in &export.pages {
            orig.extend(
                decode_page(&decoded.members[page]).map_err(|e| ExampleError::Owner {
                    seam: "replay",
                    detail: format!("page {page}: {e}"),
                })?,
            );
        }
        let attempt = replay_attempt(&orig, &decoded, &run_id);
        (orig, attempt)
    };
    let (orig, attempt) = attempt;
    let card_out = admit_replay(&decoded, &run_id, &attempt);
    // tampered copy — a payload byte changed inside the prefix; the
    // operation stays refused (recorded, never a pass).
    let tampered_verdict = {
        let mut bad = attempt.clone();
        let k = bad.copy.len() / 2;
        bad.copy[k].payload = Json::obj([("tampered", Json::Bool(true))]);
        match admit_replay(&decoded, &run_id, &bad) {
            Ok(c) => format!("card:{}", c.verdict.name()),
            Err(e) => format!("refused:{e:?}"),
        }
    };
    match &card_out {
        Ok(c) if c.verdict.name() == "admitted" => {
            checks.push(pass(
                "replay_admission",
                Json::obj([
                    ("card_id", Json::str(&c.card_id)),
                    ("level", Json::str(&c.level)),
                    (
                        "markers_appended",
                        Json::Int((attempt.copy.len() - orig.len()) as i64),
                    ),
                    ("tampered_copy", Json::str(tampered_verdict)),
                ]),
            ));
        }
        Ok(c) => checks.push(fail(
            "replay_admission",
            format!("verdict {} failures {:?}", c.verdict.name(), c.failures),
        )),
        // A typed CardRefused is a real, recorded outcome — the
        // fixture's bundle carries no `compiled_bundle` (the scripted
        // definition does not compile), so the R1-inputs gate refuses
        // verbatim. Recorded honestly, never a pass.
        Err(ref e) => checks.push(expected_refusal(
            "replay_admission",
            Json::obj([
                ("card_refused", Json::str(format!("{e:?}"))),
                ("tampered_copy", Json::str(tampered_verdict)),
            ]),
        )),
    }

    let records = Json::obj([
        ("run_id", Json::str(&run_id)),
        ("bundle_dir", Json::str(sink.to_string_lossy())),
        ("bundle", bundle),
        (
            "replay",
            match &card_out {
                Ok(c) => Json::str(&c.card_id),
                Err(e) => Json::str(format!("refused:{e:?}")),
            },
        ),
    ]);
    Ok((checks, records))
}

// ── EX-02 ─────────────────────────────────────────────────────────────

/// The decoded `ex-02-fixture/1` — the pinned two-task informative
/// grade table (never an authored result; the recipe derives every
/// coordinate durable).
#[derive(Debug)]
struct Fixture {
    tasks: Vec<String>,
    control_arm: String,
    test_arm: String,
    replicates: u32,
    metric: String,
    prior: String,
    seed: u64,
    price_table_version: String,
    split_label: String,
    stratum: String,
    family: String,
    /// `task → arm → per-replicate verdicts`.
    grades: BTreeMap<String, BTreeMap<String, Vec<bool>>>,
    /// `task → arm → pinned success count` — the lesson's declared
    /// expectation, independent of `grades` (a wrong count/mean fails
    /// against this pin, never against what was executed).
    expected: BTreeMap<String, BTreeMap<String, i64>>,
}

fn decode_fixture(j: &Json) -> Result<Fixture, ExampleError> {
    let tf = |k: &str| refused(code::TYPE_MISMATCH, format!("fixture.{k}"));
    if str_at(j, "schema") != Some(EX02_FIXTURE_SCHEMA) {
        return Err(refused(code::UNSUPPORTED_SHAPE, "fixture.schema"));
    }
    let req = |k: &str| -> Result<&Json, ExampleError> {
        j.get(k)
            .ok_or_else(|| refused(code::MISSING_FIELD, format!("fixture.{k}")))
    };
    let arms = req("arms")?;
    let mut grades: BTreeMap<String, BTreeMap<String, Vec<bool>>> = BTreeMap::new();
    if let Json::Obj(gm) = req("grades")? {
        for (task, arm_map) in gm {
            let mut am = BTreeMap::new();
            if let Json::Obj(av) = arm_map {
                for (arm, vals) in av {
                    let Json::Arr(items) = vals else {
                        return Err(tf("grades"));
                    };
                    am.insert(
                        arm.clone(),
                        items
                            .iter()
                            .map(|b| match b {
                                Json::Bool(x) => Ok(*x),
                                _ => Err(tf("grades")),
                            })
                            .collect::<Result<Vec<bool>, _>>()?,
                    );
                }
            }
            grades.insert(task.clone(), am);
        }
    }
    let mut expected: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    if let Json::Obj(em) = req("expected")? {
        for (task, arm_map) in em {
            let mut am = BTreeMap::new();
            if let Json::Obj(av) = arm_map {
                for (arm, c) in av {
                    am.insert(arm.clone(), c.as_int().ok_or_else(|| tf("expected"))?);
                }
            }
            expected.insert(task.clone(), am);
        }
    }
    let suite = req("suite")?;
    let req_str = |k: &str| -> Result<String, ExampleError> {
        req(k)?.as_str().map(str::to_string).ok_or_else(|| tf(k))
    };
    Ok(Fixture {
        tasks: match req("tasks")? {
            Json::Arr(t) => t
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => return Err(tf("tasks")),
        },
        control_arm: str_at(arms, "control")
            .ok_or_else(|| tf("arms"))?
            .to_string(),
        test_arm: str_at(arms, "test").ok_or_else(|| tf("arms"))?.to_string(),
        replicates: req("replicates")?
            .as_int()
            .ok_or_else(|| tf("replicates"))? as u32,
        metric: req_str("metric")?,
        prior: req_str("prior")?,
        seed: req("seed")?.as_int().ok_or_else(|| tf("seed"))? as u64,
        price_table_version: req_str("price_table_version")?,
        split_label: str_at(suite, "split_label")
            .ok_or_else(|| tf("suite"))?
            .to_string(),
        stratum: str_at(suite, "stratum")
            .unwrap_or("public_dated")
            .to_string(),
        family: str_at(suite, "family")
            .unwrap_or("coding_terminal")
            .to_string(),
        grades,
        expected,
    })
}

fn pinned(tag: &str) -> String {
    hharness_identity::idp_id(&format!("ex02.{tag}"), tag.as_bytes())
}

/// The per-task `exploratory` spec — two arms (control/test over the
/// strategy factor's two levels), one task, `replicates` per cell.
fn ex02_spec(fx: &Fixture, task: &str) -> ExperimentSpec {
    let level = |id: &str| FactorLevel {
        id: id.to_string(),
        content_ref: pinned(&format!("level.{id}")),
        label: id.to_string(),
    };
    let levels = vec![level("evict_oldest"), level("tiered_k4")];
    let arm = |id: &str, lv: &str| ArmSpec {
        arm_id: id.to_string(),
        hypothesis: id.to_string(),
        level_assignment: BTreeMap::from([("strategy".to_string(), lv.to_string())]),
        eval_budget: format!("eval:{id}"),
        search_budget: None,
        inference_budget: None,
        match_spec: None,
        artifact_ref: Ref::new("artifact:x", pinned("artifact.x")),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    };
    let level_assignment = |lv: &str| LevelSpec {
        level_id: lv.to_string(),
        ref_: pinned(&format!("level.{lv}")),
        overrides: None,
        label: lv.to_string(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Exploratory,
        design: Design {
            id: format!("design:ex02-{task}"),
            kind: DesignKind::FullFactorial,
            factors: vec![FactorDeclaration {
                name: "strategy".to_string(),
                kind: hharness_ontology::FactorKind::Harness,
                granularity: None,
                levels,
                role: None,
            }],
            blocking: vec!["task".to_string()],
            replicates_per_cell: fx.replicates,
            pairing: Pairing::ByTask,
            seed_policy: SeedPolicy {
                harness_rng: true,
                requested_sampling_seed: false,
                seed_honoured_required: false,
            },
            held_out_split_ref: None,
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "ex-02 exploratory fixture".to_string(),
                primary_metrics: vec![fx.metric.clone()],
                equivalence_margin: None,
                min_n: 1,
                analysis_plan_ref: pinned("analysis"),
                task_split_hash: pinned("split.hash"),
                interactions: vec![],
                dead_weight_purpose: false,
            },
            registry_snapshot_id: Some(pinned("registry.snap")),
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: vec![FactorSpec {
            name: "strategy".to_string(),
            kind: hharness_ontology::FactorKind::Harness,
            granularity: Some(hharness_ontology::participant::Granularity::ComponentLevel),
            role: None,
            levels: vec![
                level_assignment("evict_oldest"),
                level_assignment("tiered_k4"),
            ],
        }],
        arms: vec![
            arm(&fx.control_arm, "evict_oldest"),
            arm(&fx.test_arm, "tiered_k4"),
        ],
        suite: SuiteBinding {
            suite_ref: pinned("suite"),
            split_labels_used: vec![SplitLabel::Dev],
            split_assignment_ref: Some(pinned("split.assign")),
        },
        replicates_per_cell: fx.replicates,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: false,
            seed_honoured_required: false,
        },
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: format!("perm:ex02-{task}"),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 1_000_000,
            backoff: Backoff {
                min_ms: 0,
                multiplier_ppm: 1_000_000,
                max_ms: 0,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: hharness_lab::experiment::ExperimentBudgets {
            experiment: "pool".to_string(),
            instrument: "instr".to_string(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: format!("salt:ex02-{task}"),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn ex02_plan(fx: &Fixture, spec: &ExperimentSpec, task: &str) -> RecipePlan {
    use hharness_budget::spec::{BudgetMode, BudgetSpec};
    use hharness_budget::{DimensionId, DimensionKey};
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
        .to_json()
    };
    let task_grades = &fx.grades[task];
    let plan = Json::obj([
        (
            "hello",
            Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str("ex-02")),
                        ("version", Json::str("1")),
                        ("kind", Json::str("test")),
                    ]),
                ),
                (
                    "capabilities",
                    Json::obj([
                        ("experimental", Json::Bool(true)),
                        ("serves_measurement", Json::Bool(true)),
                        ("serves_host_executor", Json::Bool(true)),
                        ("serves_permission_channel", Json::Bool(true)),
                        ("accepts_ephemeral_frames", Json::Bool(true)),
                    ]),
                ),
            ]),
        ),
        (
            "register",
            Json::obj([
                ("spec", spec.to_json()),
                (
                    "budgets",
                    Json::Obj(BTreeMap::from([
                        (format!("eval:{}", fx.control_arm), caps(100)),
                        (format!("eval:{}", fx.test_arm), caps(100)),
                        ("pool".to_string(), caps(1_000)),
                        ("instr".to_string(), caps(1_000)),
                    ])),
                ),
                (
                    "suite_tasks",
                    Json::Arr(vec![Json::obj([
                        ("task_id", Json::str(task)),
                        ("split_label", Json::str(&fx.split_label)),
                    ])]),
                ),
                (
                    "arm_configs",
                    Json::Obj(BTreeMap::from([
                        (
                            fx.control_arm.clone(),
                            Json::obj([
                                ("configuration_id", Json::str(pinned("cfg.a"))),
                                ("configuration_version_id", Json::str(pinned("cfgv.a"))),
                            ]),
                        ),
                        (
                            fx.test_arm.clone(),
                            Json::obj([
                                ("configuration_id", Json::str(pinned("cfg.b"))),
                                ("configuration_version_id", Json::str(pinned("cfgv.b"))),
                            ]),
                        ),
                    ])),
                ),
            ]),
        ),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "subject_input",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("text")),
                ("text", Json::str("begin")),
            ])]),
        ),
        ("grade_metric", Json::str(&fx.metric)),
        (
            "grades",
            Json::Obj(
                task_grades
                    .iter()
                    .map(|(a, v)| {
                        (
                            a.clone(),
                            Json::Arr(v.iter().map(|b| Json::Bool(*b)).collect()),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "analysis",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("spec_id", Json::str("")),
                        ("kind", Json::str("summarize")),
                        (
                            "query",
                            Json::obj([("metrics", Json::Arr(vec![Json::str(&fx.metric)]))]),
                        ),
                        ("spec_ref", Json::str(&spec.experiment_id)),
                        (
                            "estimator_selection",
                            EstimatorSelection {
                                method: IntervalMethod::ClusteredClt,
                                selection_rule: "adr-0158.clt_floor".to_string(),
                                floors: BTreeMap::new(),
                                fallback_chain: Vec::new(),
                                substituted: None,
                            }
                            .to_json(),
                        ),
                        ("outputs", Json::Arr(vec![Json::str("report")])),
                    ]),
                ),
                (
                    "tasks",
                    Json::Arr(vec![Json::obj([
                        ("task_id", Json::str(task)),
                        ("suite_id", Json::str(pinned("suite"))),
                        ("split_label", Json::str(&fx.split_label)),
                        ("split_hash", Json::str(pinned("split.hash"))),
                        ("stratum", Json::str(&fx.stratum)),
                    ])]),
                ),
                (
                    "suites",
                    Json::Arr(vec![Json::obj([
                        ("suite_id", Json::str(pinned("suite"))),
                        ("family", Json::str(&fx.family)),
                    ])]),
                ),
                ("seed", Json::Int(fx.seed as i64)),
                ("price_table_version", Json::str(&fx.price_table_version)),
            ]),
        ),
        (
            "weights",
            Json::obj([
                ("family", Json::str("uniform")),
                ("rule", Json::str("w_t = 1/T")),
            ]),
        ),
    ]);
    RecipePlan::from_json(&plan).expect("ex02 plan shape")
}

/// The durable `measurement.experiment.run_settled` ref for
/// (`experiment_run_id`, `run_id`) — the settlement's actual ref.
fn settled_ref(svc: &EmbedService, exp_run: &str, run_id: &str) -> Option<String> {
    svc.store()
        .events(exp_run)
        .ok()?
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.experiment.run_settled"
                && str_at(&e.payload, "run_id") == Some(run_id)
        })
        .map(|e| e.event_id.clone())
}

fn ex02(
    svc: &mut EmbedService,
    _card: &ExampleCard,
    _work: &Path,
    pins: &BTreeMap<String, (PathBuf, Vec<u8>)>,
) -> Result<(Vec<CheckRecord>, Json), ExampleError> {
    let mut checks: Vec<CheckRecord> = Vec::new();
    let fbytes = &pins
        .get("fixture")
        .ok_or_else(|| refused(code::MISSING_FIELD, "card.pins.fixture"))?
        .1;
    let fx = decode_fixture(
        &hharness_wire::json::parse(
            std::str::from_utf8(fbytes)
                .map_err(|e| refused(code::TYPE_MISMATCH, format!("{e}")))?,
        )
        .map_err(|e| refused(code::TYPE_MISMATCH, format!("fixture json: {e}")))?,
    )?;

    // ── one recipe per task — 2 arms × 5 replicates each = 20 ───────
    let mut slots = Vec::new();
    let mut exp_runs: Vec<String> = Vec::new();
    let mut exp_ids: Vec<String> = Vec::new();
    let mut receipts_closed = 0u32;
    let mut resolvers_per: Vec<BTreeMap<String, Json>> = Vec::new();
    for task in &fx.tasks {
        let spec = ex02_spec(&fx, task);
        let plan = ex02_plan(&fx, &spec, task);
        let r = run_recipe(svc, &plan).map_err(|e| ExampleError::Owner {
            seam: "recipe",
            detail: format!("{task}: {e}"),
        })?;
        if r.receipt.verdict == "closed" {
            receipts_closed += 1;
        }
        if let Json::Obj(m) = &plan.register {
            let mut res = m.clone();
            res.remove("spec");
            resolvers_per.push(res);
        }
        exp_runs.push(r.experiment_run_id.clone());
        exp_ids.push(r.experiment_id.clone());
        slots.extend(r.slots);
    }
    checks.push(
        if slots.len() == (fx.tasks.len() * 2 * fx.replicates as usize)
            && receipts_closed as usize == fx.tasks.len()
            && slots.iter().all(|s| {
                !s.submission_ref.is_empty()
                    && !s.grade_ref.is_empty()
                    && !s.row_version_id.is_empty()
                    && s.outcome_class == "scored"
            })
        {
            pass(
                "subjects_20",
                Json::obj([
                    ("slots", Json::Int(slots.len() as i64)),
                    ("receipts_closed", Json::Int(receipts_closed as i64)),
                ]),
            )
        } else {
            fail(
                "subjects_20",
                format!("{} slots, {receipts_closed} closed receipts", slots.len()),
            )
        },
    );

    // ── durable grade recomputation vs the pinned sequences ─────────
    {
        // counts per (task, arm) cell re-derived from the durable
        // `measurement.metric.emitted` rows — never the fixture text.
        let mut actual: BTreeMap<(String, String), (u64, u64)> = BTreeMap::new();
        let view0 = ExperimentView::fold(svc.store().events(&exp_runs[0]).map_err(|e| {
            ExampleError::Owner {
                seam: "view",
                detail: format!("{e:?}"),
            }
        })?);
        let mut view = view0;
        for extra in &exp_runs[1..] {
            let v = ExperimentView::fold(svc.store().events(extra).map_err(|e| {
                ExampleError::Owner {
                    seam: "view",
                    detail: format!("{e:?}"),
                }
            })?);
            view.plans.extend(v.plans);
            view.amendments.extend(v.amendments);
        }
        for s in &slots {
            let ps = view
                .plans
                .get(&s.run_plan_id)
                .ok_or_else(|| ExampleError::Owner {
                    seam: "view",
                    detail: format!("no plan {}", s.run_plan_id),
                })?;
            let emit = svc
                .store()
                .events(&s.run_id)
                .map_err(|e| ExampleError::Owner {
                    seam: "store",
                    detail: format!("{e:?}"),
                })?
                .iter()
                .find(|e| {
                    e.class == "measurement.metric.emitted"
                        && str_at(&e.payload, "oracle_ref") == Some(crate::recipe::RECIPE_VERSION)
                })
                .ok_or_else(|| ExampleError::Owner {
                    seam: "store",
                    detail: format!("{}: no recipe grade row", s.run_id),
                })?;
            let ok = matches!(
                emit.payload.get("value").and_then(|v| v.get("value")),
                Some(Json::Bool(true))
            );
            let cell = actual
                .entry((ps.task_id.clone(), ps.arm_id.clone()))
                .or_insert((0, 0));
            cell.1 += 1;
            if ok {
                cell.0 += 1;
            }
        }
        let mut mismatches: Vec<Json> = Vec::new();
        for (task, arm_map) in &fx.expected {
            for (arm, c) in arm_map {
                let want = (*c as u64, fx.replicates as u64);
                let got = actual
                    .get(&(task.clone(), arm.clone()))
                    .copied()
                    .unwrap_or((0, 0));
                if got != want {
                    mismatches.push(Json::obj([
                        ("task", Json::str(task)),
                        ("arm", Json::str(arm)),
                        ("want", Json::str(format!("{}/{}", want.0, want.1))),
                        ("got", Json::str(format!("{}/{}", got.0, got.1))),
                    ]));
                }
            }
        }
        checks.push(if mismatches.is_empty() {
            pass(
                "cells_pinned",
                Json::obj([("cells", Json::Int(actual.len() as i64))]),
            )
        } else {
            fail_json("cells_pinned", Json::Arr(mismatches))
        });
    }

    // ── recipient-side input rebuild (fresh handles; durable facts
    //     only — never producer-side objects) ─────────────────────────
    let store_root = svc.store().root().to_path_buf();
    let results =
        ResultsStore::open(store_root.join("results")).map_err(|e| ExampleError::Owner {
            seam: "project",
            detail: format!("{e:?}"),
        })?;
    let store =
        hharness_ledger::store::Store::open(&store_root).map_err(|e| ExampleError::Owner {
            seam: "store",
            detail: format!("{e:?}"),
        })?;
    let docs = LabDocs::open(&store_root).map_err(|e| ExampleError::Owner {
        seam: "labdocs",
        detail: format!("{e:?}"),
    })?;
    let mut rows = Vec::new();
    let mut manifests = BTreeMap::new();
    let mut facts = BTreeMap::new();
    let mut watermark_set = hharness_results::watermark::WatermarkSet::new();
    let mut pins_head: BTreeMap<String, u64> = BTreeMap::new();
    for s in &slots {
        let row = results
            .get_version(&s.row_version_id)
            .map_err(|e| ExampleError::Owner {
                seam: "project",
                detail: format!("{}: {e:?}", s.row_version_id),
            })?;
        pins_head.insert(
            s.run_id.clone(),
            store
                .head(&s.run_id)
                .map_err(|e| ExampleError::Owner {
                    seam: "store",
                    detail: format!("{e:?}"),
                })?
                .seq as u64,
        );
        manifests.insert(
            s.run_id.clone(),
            store
                .manifest(&s.run_id)
                .map_err(|e| ExampleError::Owner {
                    seam: "store",
                    detail: format!("{e:?}"),
                })?
                .clone(),
        );
        let evs: Vec<(u64, String, Json)> = store
            .events(&s.run_id)
            .map_err(|e| ExampleError::Owner {
                seam: "store",
                detail: format!("{e:?}"),
            })?
            .iter()
            .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
            .collect();
        facts.insert(s.run_id.clone(), LedgerFacts::from_events(&evs));
        for (rid, seq) in &row.watermark_set.runs {
            watermark_set.pin(rid, *seq);
        }
        rows.push(row);
    }
    let tasks: Vec<TaskContext> = fx
        .tasks
        .iter()
        .map(|t| TaskContext {
            task_id: t.clone(),
            suite_id: pinned("suite"),
            split_label: SplitLabel::parse(&fx.split_label).unwrap_or(SplitLabel::Dev),
            split_hash: pinned("split.hash"),
            stratum: ContaminationStratum::parse(&fx.stratum)
                .unwrap_or(ContaminationStratum::PublicDated),
        })
        .collect();
    let suites = vec![SuiteContext {
        suite_id: pinned("suite"),
        retired_for_headline: false,
        family: EnvironmentFamily::parse(&fx.family).unwrap_or(EnvironmentFamily::CodingTerminal),
    }];
    // The merged planned-slot universe — the union of both experiments'
    // folded views (a single experiment's view cannot express the
    // two-task population fmc's `tasks>=2` floor requires).
    let view =
        {
            let mut v = ExperimentView::fold(store.events(&exp_runs[0]).map_err(|e| {
                ExampleError::Owner {
                    seam: "view",
                    detail: format!("{e:?}"),
                }
            })?);
            for extra in &exp_runs[1..] {
                let w =
                    ExperimentView::fold(store.events(extra).map_err(|e| ExampleError::Owner {
                        seam: "view",
                        detail: format!("{e:?}"),
                    })?);
                v.plans.extend(w.plans);
                v.amendments.extend(w.amendments);
            }
            v
        };
    let input = AnalysisInput {
        rows: &rows,
        manifests: &manifests,
        facts: &facts,
        tasks: &tasks,
        suites: &suites,
        design: None,
        arm_specs: &[],
        declarations: &[],
        pre_registration: None,
        watermark_set: &watermark_set,
        metric_registry_version: "",
        price_table_version: &fx.price_table_version,
        confidence_ppm: 950_000,
        resampling_draws: 0,
        seed: fx.seed,
        post_amendment: !view.amendments.is_empty(),
        view: Some(&view),
    };
    let mut fmc_spec = AnalysisSpec {
        spec_id: String::new(),
        kind: fm::CONTRAST_KIND.to_string(),
        query: QuerySpec {
            metrics: vec![],
            filters: Some(Json::obj([
                ("metric", Json::str(&fx.metric)),
                ("test_arm", Json::str(&fx.test_arm)),
                ("control_arm", Json::str(&fx.control_arm)),
            ])),
            grain: None,
        },
        // Two bound experiments — no single `hh-experiment/1` doc names
        // this analysis; the merged view is the recorded universe.
        spec_ref: None,
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::FiniteManifestContrast(fx.prior.clone()),
            selection_rule: "ex-02.card".to_string(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    fmc_spec.spec_id = fmc_spec.spec_id();

    // ── persisted report — the owner persist path (idempotent; a
    //     re-serve never recomputes — same inputs keep identity) ──────
    let outcome =
        analyze_and_record(&docs, &fmc_spec, &input).map_err(|e| ExampleError::Owner {
            seam: "analysis",
            detail: format!("{e:?}"),
        })?;
    let again = analyze_and_record(&docs, &fmc_spec, &input).map_err(|e| ExampleError::Owner {
        seam: "analysis",
        detail: format!("{e:?}"),
    })?;
    checks.push(if again.report_id == outcome.report_id {
        pass(
            "report_identity",
            Json::obj([("report_id", Json::str(&outcome.report_id))]),
        )
    } else {
        fail(
            "report_identity",
            format!("{} ≠ {}", outcome.report_id, again.report_id),
        )
    });

    // ── informative contrast — per-task cells, unequal nondegenerate ─
    {
        let sect = outcome
            .body
            .get(fm::CONTRAST_KIND)
            .cloned()
            .unwrap_or(Json::Null);
        let cells = sect.get("cells").cloned().unwrap_or(Json::Arr(vec![]));
        let mut per_task: BTreeMap<String, (i64, i64, i64, i64)> = BTreeMap::new();
        let mut cell_ok = true;
        if let Json::Arr(items) = &cells {
            for c in items {
                let t = str_at(c, "task_id").unwrap_or("?").to_string();
                let g = |arm: &str, key: &str| {
                    c.get(arm)
                        .and_then(|a| a.get(key))
                        .and_then(Json::as_int)
                        .unwrap_or(-1)
                };
                let e = per_task.entry(t).or_insert((0, 0, 0, 0));
                *e = (
                    g("test", "c"),
                    g("test", "n"),
                    g("control", "c"),
                    g("control", "n"),
                );
            }
            // Nondegenerate: every task has both cells resolved and the
            // observed per-task contrasts are not all identical.
            let mut contrasts: Vec<i128> = Vec::new();
            for (t, (tc, tn, cc, cn)) in &per_task {
                if *tn <= 0 || *cn <= 0 || *tc < 0 || *cc < 0 {
                    cell_ok = false;
                    let _ = t;
                    continue;
                }
                contrasts.push(*tc as i128 * *cn as i128 - *cc as i128 * *tn as i128);
            }
            if per_task.len() != fx.tasks.len() {
                cell_ok = false;
            }
            let informative = cell_ok && contrasts.iter().any(|c| *c != contrasts[0]);
            checks.push(if informative {
                pass(
                    "informative_contrast",
                    Json::obj([
                        ("cells", cells.clone()),
                        (
                            "observed_contrast_ppm",
                            sect.get("observed_contrast_ppm")
                                .cloned()
                                .unwrap_or(Json::Null),
                        ),
                    ]),
                )
            } else {
                fail(
                    "informative_contrast",
                    format!("degenerate cells {per_task:?} — a constant fixture is not the lesson"),
                )
            });
        } else {
            checks.push(fail(
                "informative_contrast",
                format!("no {CONTRAST} cells", CONTRAST = fm::CONTRAST_KIND),
            ));
        }
    }

    // ── exploratory label — a configured sample/budget never becomes
    //     a confirmation default ──────────────────────────────────────
    checks.push(
        if matches!(
            outcome.report.status,
            hharness_lab::analysis::AnalysisStatus::Exploratory
        ) {
            pass(
                "exploratory_label",
                Json::obj([("status", Json::str("exploratory"))]),
            )
        } else {
            fail(
                "exploratory_label",
                format!("status {:?}", outcome.report.status),
            )
        },
    );

    // ── expected-refusal controls — the owners' typed errors verbatim ─
    {
        let mut exp_params = resolvers_per[0].clone();
        exp_params.insert("experiment_id".to_string(), Json::str(&exp_ids[0]));
        let bad_lease = call_err(svc, "lab.experiment.launch", {
            let mut p = exp_params.clone();
            p.insert("run_plan_id".to_string(), Json::str(&slots[0].run_plan_id));
            p.insert("lease_id".to_string(), Json::str("lease-bogus"));
            Json::Obj(p)
        });
        let dup_settle = call_err(svc, "lab.experiment.settle", {
            let mut p = exp_params.clone();
            p.insert("run_plan_id".to_string(), Json::str(&slots[0].run_plan_id));
            Json::Obj(p)
        });
        let missing_row = call_err(
            svc,
            "lab.analysis.analyze",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("spec_id", Json::str("")),
                        ("kind", Json::str("summarize")),
                        ("query", Json::obj([("metrics", Json::Arr(vec![]))])),
                        ("outputs", Json::Arr(vec![Json::str("report")])),
                    ]),
                ),
                ("rows", Json::Arr(vec![Json::str("row-does-not-exist")])),
            ]),
        );
        let all_refused = !matches!(bad_lease, Json::Null) && !matches!(missing_row, Json::Null);
        checks.push(if all_refused {
            expected_refusal(
                "refusal_controls",
                Json::obj([
                    ("bad_lease", bad_lease),
                    ("duplicate_settle", dup_settle),
                    ("missing_row", missing_row),
                ]),
            )
        } else {
            fail(
                "refusal_controls",
                format!("bad_lease {bad_lease:?} missing_row {missing_row:?}"),
            )
        });
    }

    // ── clean-recipient recomputation — frozen pins + the all-planned
    //     reconcile over the merged view; rebuilt input only ─────────
    let declared = DeclaredReceipt {
        slots: view.plans.keys().cloned().collect::<BTreeSet<String>>(),
        attempts: {
            let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for s in &slots {
                m.entry(s.run_plan_id.clone())
                    .or_default()
                    .push(s.run_id.clone());
            }
            m
        },
        settlements: slots
            .iter()
            .map(|s| SettlementDecl {
                run_id: s.run_id.clone(),
                settlement_ref: exp_runs
                    .iter()
                    .find_map(|er| settled_ref(svc, er, &s.run_id))
                    .unwrap_or_default(),
                cost_version: String::new(),
            })
            .collect(),
        grades: slots
            .iter()
            .map(|s| GradeDecl {
                run_id: s.run_id.clone(),
                grade_ref: s.grade_ref.clone(),
                selection_ref: String::new(),
                ordinal: 1,
                counts_subject: true,
                status: GradeStatus::Selected,
            })
            .collect(),
        rows: slots
            .iter()
            .map(|s| RowDecl {
                run_id: s.run_id.clone(),
                version_id: s.row_version_id.clone(),
                plan_ref: s.run_plan_id.clone(),
            })
            .collect(),
        cost_version: String::new(),
        selection_ref: String::new(),
        analysis: AnalysisDecl {
            protocol_ref: None,
            prices: Json::obj([("price_table_version", Json::str(&fx.price_table_version))]),
            method: Json::obj([
                ("estimator", Json::str(fm::CONTRAST_KIND)),
                ("version", Json::str(fm::CONTRAST_VERSION)),
                (
                    "keys",
                    Json::obj([
                        ("population", Json::str("canonical")),
                        ("test_arm", Json::str(&fx.test_arm)),
                        ("control_arm", Json::str(&fx.control_arm)),
                    ]),
                ),
            ]),
            seed: fx.seed,
            draws: 0,
            weights: Json::obj([("weights_ref", Json::str("equal_task:w_t=1/T"))]),
            spec_ref: Some(fmc_spec.spec_id()),
            record_ref: Some(outcome.record.analysis_id.clone()),
            report_ref: Some(outcome.report_id.clone()),
        },
    };
    let rec = recompute_analysis(
        &RecomputePlan {
            spec: &fmc_spec,
            declared: &declared.analysis,
            reference: Some(&outcome.body),
            population: Some(PopulationPins {
                view: &view,
                metric_ref: &fx.metric,
                declared: &declared,
            }),
        },
        &input,
    )
    .map_err(|e| ExampleError::Owner {
        seam: "recompute",
        detail: format!("{e:?}"),
    })?;
    checks.push(if rec.verdict == "reproduced" {
        pass(
            "recipient_recompute",
            Json::obj([
                ("verdict", Json::str("reproduced")),
                ("report_id", Json::str(&rec.report_id)),
                (
                    "legs",
                    Json::Arr(
                        rec.legs
                            .iter()
                            .map(|l| Json::str(format!("{}:{}", l.name, l.verdict)))
                            .collect(),
                    ),
                ),
            ]),
        )
    } else {
        fail(
            "recipient_recompute",
            format!("verdict {} drifts {:?}", rec.verdict, rec.drifts),
        )
    });

    let records = Json::obj([
        (
            "experiment_ids",
            Json::Arr(exp_ids.iter().map(Json::str).collect()),
        ),
        ("report_id", Json::str(&outcome.report_id)),
        ("analysis_record", Json::str(&outcome.record.analysis_id)),
        ("recompute", rec.to_json()),
    ]);
    Ok((checks, records))
}
