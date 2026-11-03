//! `hh-recipe/1` — the one supported versioned in-process `hharness`
//! recipe (R5.28; §2.1's composition row; REQ-E-009, REQ-A-002,
//! REQ-A-202, REQ-A-106 contributing).
//!
//! One bounded client path composes the landed owners —
//! `lab.experiment.*` launch → `adopt_subject` (R5.7's embed arming/drive
//! seam) → scripted-lane `submit` execution → `measurement.emit_metric`
//! grade → `lab.experiment.close` → `ResultsStore::project_and_record`
//! projection → `lab.analysis.analyze` persisted analysis →
//! `assemble_receipt` (R5.23's `population-receipt/1`). The recipe owns
//! no semantics: every durable mutation rides the boundary op or the
//! owning interface; every decode/codec is the owner's own. There is no
//! second runner, no second store, no shadow result/receipt, no forged
//! producer row — a plan supplies *inputs* (spec, budgets, resolvers,
//! environment, subject input, declared grade map, analysis body); the
//! recipe derives every coordinate from durable facts.
//!
//! The `RecipeReceipt` records what the selected client actually did:
//! the negotiated `hello` result verbatim, every boundary request with
//! its outcome class, every owner-seam invocation, the drained
//! `upcall.*`/frame notifications — or `n/a{no_callback_channels}` when
//! the plan arms none — and the full coordinate/receipt tail.
//!
//! Live posture (the ticket's `Live stage:` header): offline only —
//! the scripted `EmbedModel` lane is real execution through the
//! existing runtime; actual task/provider/participant execution stays
//! gated exactly as the owners leave it.

use std::collections::BTreeMap;

use hharness_analysis::kernel::{project_runs, AnalysisInput};
use hharness_analysis::receipt::{
    assemble_receipt, AnalysisDecl, DeclaredReceipt, GradeDecl, GradeStatus, PopulationReceipt,
    RowDecl, SettlementDecl,
};
use hharness_embed::adopt::{AdoptBinding, AdoptPorts};
use hharness_embed::service::{EmbedService, HostCap};
use hharness_embed_schema::types::EnvironmentInput;
use hharness_eval::facts::LedgerFacts;
use hharness_eval::runs::{SuiteContext, TaskContext};
use hharness_experiment::docs::LabDocs;
use hharness_experiment::view::ExperimentView;
use hharness_ledger::manifest::RunManifest;
use hharness_ledger::store::Lease;
use hharness_ontology::eval::{MetricValue, MetricValueKind};
use hharness_ontology::lab::{ContaminationStratum, SplitLabel};
use hharness_results::row::ResultsRow;
use hharness_results::store::ResultsStore;
use hharness_results::version::DerivedReason;
use hharness_results::watermark::WatermarkSet;
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

/// The recipe's version label — recorded on every receipt.
pub const RECIPE_VERSION: &str = "hh-recipe/1";

/// The closed refusal-code set [`RecipeError::Refused`] carries.
pub mod code {
    /// A plan member outside the closed input set — supplied
    /// records/authored rows never ride the plan.
    pub const UNSUPPORTED_SHAPE: &str = "unsupported_shape";
    /// A required plan member is absent.
    pub const MISSING_FIELD: &str = "missing_field";
    /// A plan member decoded to the wrong type.
    pub const TYPE_MISMATCH: &str = "type_mismatch";
    /// The subject run never reached durable finish — a process-only
    /// success never grades.
    pub const EXECUTION_INCOMPLETE: &str = "execution_incomplete";
    /// The run finished but no observed `hh.submit` submission is
    /// durable — there is nothing to grade.
    pub const NO_SUBMISSION: &str = "no_submission";
    /// The declared grade map has no entry for the bound
    /// (arm, replicate) — a grade is declared or refused, never
    /// defaulted.
    pub const GRADE_UNSPECIFIED: &str = "grade_unspecified";
    /// `next` answered a non-`plan` verdict that is not `done`.
    pub const DRAIN_STALLED: &str = "drain_stalled";
}

/// The recipe error sum — typed refusals plus wrapped owner/boundary
/// failures (verbatim detail; never re-classified).
#[derive(Debug)]
pub enum RecipeError {
    /// A recipe-level typed refusal (`code` ∈ [`code`]).
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
    /// An owner-seam call (`adopt`/`project`/`receipt`) failed.
    Owner {
        /// The seam name.
        seam: &'static str,
        /// The owner's own error detail.
        detail: String,
    },
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipeError::Refused { code, detail } => write!(f, "refused{{{code}}}: {detail}"),
            RecipeError::Boundary { method, detail } => write!(f, "boundary {method}: {detail:?}"),
            RecipeError::Owner { seam, detail } => write!(f, "owner {seam}: {detail}"),
        }
    }
}

impl std::error::Error for RecipeError {}

fn refused(code: &'static str, detail: impl Into<String>) -> RecipeError {
    RecipeError::Refused {
        code,
        detail: detail.into(),
    }
}

/// `RecipePlan` — the recipe's records-in input (closed member set).
/// Everything durable derives from these *inputs*; the plan never
/// supplies records (`slots`/`attempts`/`settlements`/`rows`/`metrics`/
/// `analysis.rows` …) — an authored member refuses `unsupported_shape`.
#[derive(Debug, Clone)]
pub struct RecipePlan {
    /// The `hello` params body (`{contract_major, client, capabilities}`).
    pub hello: Json,
    /// The `lab.experiment.register` params body verbatim (`{spec,
    /// budgets, suite_tasks, arm_configs, …}` — the boundary's own
    /// records-in surface).
    pub register: Json,
    /// The adoption environment (`EnvironmentInput`; the smallest
    /// supported composition is `local_host`, per R5.7).
    pub environment: EnvironmentInput,
    /// The host capabilities adoption arms (empty ⇒ the notification
    /// channel is absent — recorded `n/a`, never faked).
    pub host_capabilities: Vec<HostCap>,
    /// The `submit{input}` blocks driving the scripted lane
    /// (a `[{kind:"text",…}]` body reaches `hh.submit` completion;
    /// `{kind:"invoke"}` rides the declared surface table).
    pub subject_input: Json,
    /// The grade metric the instrument records (e.g. `task_success`).
    pub grade_metric: String,
    /// The declared instrument verdicts — `arm_id → per-replicate
    /// bools` (`grades[arm][replicate_index]`); a bound slot with no
    /// entry refuses `grade_unspecified` (never a default success).
    pub grades: BTreeMap<String, Vec<bool>>,
    /// The `lab.analysis.analyze` params body minus `rows` — the recipe
    /// fills `rows[]` with the projected row key ids (a supplied `rows`
    /// member is an authored record — `unsupported_shape`).
    pub analysis: Json,
    /// The task-weight declaration the receipt pins (`weights` —
    /// declared by the plan, verified member-wise by the receipt's
    /// rows→analysis leg).
    pub weights: Json,
    /// Optional pins carried verbatim into the receipt declaration
    /// (`None`/absent reads as the empty pin — the reconcile legs then
    /// skip their member-wise equality checks honestly).
    pub protocol_ref: Option<String>,
    /// The pinned cost-view version (absent ⇒ `""`).
    pub cost_version: Option<String>,
    /// The pinned frozen selection ref (absent ⇒ `""`).
    pub selection_ref: Option<String>,
    /// Drain bound — max `next` calls before `drain_stalled`.
    pub max_drain: u32,
}

/// Members a plan may *not* carry — authored records (the recipe
/// derives every one of them from durable facts).
const AUTHORED_MEMBERS: &[&str] = &[
    "slots",
    "attempts",
    "settlements",
    "grades_done",
    "rows",
    "metrics",
    "run_ids",
    "success",
    "receipt",
    "notifications",
    "requests",
];

const PLAN_MEMBERS: &[&str] = &[
    "hello",
    "register",
    "environment",
    "host_capabilities",
    "subject_input",
    "grade_metric",
    "grades",
    "analysis",
    "weights",
    "protocol_ref",
    "cost_version",
    "selection_ref",
    "max_drain",
];

fn req_member<'a>(j: &'a Json, k: &str) -> Result<&'a Json, RecipeError> {
    j.get(k)
        .ok_or_else(|| refused(code::MISSING_FIELD, format!("plan.{k}")))
}

fn str_member<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

impl RecipePlan {
    /// Strict decode — closed member set, authored members refused.
    pub fn from_json(j: &Json) -> Result<Self, RecipeError> {
        let Json::Obj(m) = j else {
            return Err(refused(code::TYPE_MISMATCH, "plan must be an object"));
        };
        for k in m.keys() {
            if AUTHORED_MEMBERS.contains(&k.as_str()) {
                return Err(refused(
                    code::UNSUPPORTED_SHAPE,
                    format!("plan.{k} is an authored record — the recipe derives it durable"),
                ));
            }
            if !PLAN_MEMBERS.contains(&k.as_str()) {
                return Err(refused(
                    code::UNSUPPORTED_SHAPE,
                    format!("plan.{k} is not a supported recipe member"),
                ));
            }
        }
        if req_member(j, "analysis")?.get("rows").is_some() {
            return Err(refused(
                code::UNSUPPORTED_SHAPE,
                "plan.analysis.rows is an authored record — the recipe fills it from projection",
            ));
        }
        let environment =
            EnvironmentInput::from_json(req_member(j, "environment")?, "/environment")
                .map_err(|e| refused(code::TYPE_MISMATCH, format!("plan.environment: {e:?}")))?;
        let mut host_capabilities = Vec::new();
        if let Some(hc) = j.get("host_capabilities") {
            let Json::Arr(items) = hc else {
                return Err(refused(code::TYPE_MISMATCH, "plan.host_capabilities"));
            };
            for it in items {
                host_capabilities.push(HostCap {
                    capability_id: str_member(it, "capability_id")
                        .unwrap_or_default()
                        .to_string(),
                    surface_id: str_member(it, "surface_id")
                        .ok_or_else(|| {
                            refused(code::MISSING_FIELD, "plan.host_capabilities[].surface_id")
                        })?
                        .to_string(),
                    requires_approval: matches!(
                        it.get("requires_approval"),
                        Some(Json::Bool(true))
                    ),
                    options: match it.get("options") {
                        Some(Json::Arr(a)) => a
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    },
                    timeout_ms: it
                        .get("timeout_ms")
                        .and_then(Json::as_int)
                        .map(|v| v as u64),
                    risk_class: it
                        .get("risk_class")
                        .and_then(hharness_ontology::risk::RiskClass::from_json),
                });
            }
        }
        let mut grades: BTreeMap<String, Vec<bool>> = BTreeMap::new();
        match req_member(j, "grades")? {
            Json::Obj(gm) => {
                for (arm, v) in gm {
                    let Json::Arr(vals) = v else {
                        return Err(refused(code::TYPE_MISMATCH, format!("plan.grades.{arm}")));
                    };
                    grades.insert(
                        arm.clone(),
                        vals.iter()
                            .map(|b| match b {
                                Json::Bool(x) => Ok(*x),
                                _ => Err(refused(
                                    code::TYPE_MISMATCH,
                                    format!("plan.grades.{arm} member"),
                                )),
                            })
                            .collect::<Result<Vec<bool>, _>>()?,
                    );
                }
            }
            _ => return Err(refused(code::TYPE_MISMATCH, "plan.grades")),
        }
        Ok(RecipePlan {
            hello: req_member(j, "hello")?.clone(),
            register: req_member(j, "register")?.clone(),
            environment,
            host_capabilities,
            subject_input: req_member(j, "subject_input")?.clone(),
            grade_metric: req_member(j, "grade_metric")?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| refused(code::TYPE_MISMATCH, "plan.grade_metric"))?,
            grades,
            analysis: req_member(j, "analysis")?.clone(),
            weights: req_member(j, "weights")?.clone(),
            protocol_ref: str_member(j, "protocol_ref").map(str::to_string),
            cost_version: str_member(j, "cost_version").map(str::to_string),
            selection_ref: str_member(j, "selection_ref").map(str::to_string),
            max_drain: j
                .get("max_drain")
                .and_then(Json::as_int)
                .map(|v| v.max(1) as u32)
                .unwrap_or(64),
        })
    }
}

/// One settled slot's coordinates — recorded on the receipt.
#[derive(Debug, Clone)]
pub struct SlotRecord {
    /// The plan coordinate.
    pub run_plan_id: String,
    /// The adopted subject run.
    pub run_id: String,
    /// The bound arm.
    pub arm_id: String,
    /// The bound attempt ordinal.
    pub attempt_no: u32,
    /// The armed adopt session id.
    pub session_id: String,
    /// The durable `hh.submit` submission ref the grade cited.
    pub submission_ref: String,
    /// The emitted grade row's durable event id.
    pub grade_ref: String,
    /// The settle outcome class the engine recorded.
    pub outcome_class: String,
    /// The projected row's key id and head version id.
    pub row_key_id: String,
    /// The projected `RowVersion` id.
    pub row_version_id: String,
}

/// `RecipeReceipt` — what the selected client actually did (and the
/// `population-receipt/1` the chain produced).
#[derive(Debug)]
pub struct RecipeReceipt {
    /// [`RECIPE_VERSION`].
    pub recipe_version: &'static str,
    /// The negotiated `hello` result verbatim (actual versions,
    /// negotiated capabilities).
    pub hello: Json,
    /// One record per boundary request: `{method, outcome, result|error}`.
    pub requests: Vec<Json>,
    /// One record per owner-seam invocation: `{seam, outcome, detail}`.
    pub owner_calls: Vec<Json>,
    /// The drained notifications verbatim, or `n/a{no_callback_channels}`.
    pub notifications: Json,
    /// The registered `experiment_id` / opened `experiment_run_id`.
    pub experiment_id: String,
    /// The experiment run id.
    pub experiment_run_id: String,
    /// The `lab.experiment.close` status.
    pub close_status: String,
    /// Per-slot coordinates.
    pub slots: Vec<SlotRecord>,
    /// The persisted analysis refs `{record_ref, report_ref}`.
    pub analysis_ref: Json,
    /// The `population-receipt/1` (verdict `closed` only on a clean
    /// reconcile — its own failures stay readable).
    pub receipt: PopulationReceipt,
}

impl RecipeReceipt {
    /// Canonical JSON — the recipe's own record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("recipe", Json::str(self.recipe_version)),
            ("hello", self.hello.clone()),
            ("requests", Json::Arr(self.requests.clone())),
            ("owner_calls", Json::Arr(self.owner_calls.clone())),
            ("notifications", self.notifications.clone()),
            ("experiment_id", Json::str(&self.experiment_id)),
            ("experiment_run_id", Json::str(&self.experiment_run_id)),
            ("close_status", Json::str(&self.close_status)),
            (
                "slots",
                Json::Arr(
                    self.slots
                        .iter()
                        .map(|s| {
                            Json::obj([
                                ("run_plan_id", Json::str(&s.run_plan_id)),
                                ("run_id", Json::str(&s.run_id)),
                                ("arm_id", Json::str(&s.arm_id)),
                                ("attempt_no", Json::Int(s.attempt_no as i64)),
                                ("session_id", Json::str(&s.session_id)),
                                ("submission_ref", Json::str(&s.submission_ref)),
                                ("grade_ref", Json::str(&s.grade_ref)),
                                ("outcome_class", Json::str(&s.outcome_class)),
                                ("row_key_id", Json::str(&s.row_key_id)),
                                ("row_version_id", Json::str(&s.row_version_id)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("analysis_ref", self.analysis_ref.clone()),
            ("receipt", self.receipt.to_json()),
        ])
    }
}

/// One boundary call — records `{method, outcome, result|error}` and
/// unwraps `result` or wraps the error verbatim.
fn call(
    svc: &mut EmbedService,
    log: &mut Vec<Json>,
    method: &str,
    params: Json,
) -> Result<Json, RecipeError> {
    let resp = svc.handle(&Request {
        id: Json::str(format!("recipe-{method}")),
        method: method.to_string(),
        params,
    });
    if let Some(r) = resp.get("result") {
        log.push(Json::obj([
            ("method", Json::str(method)),
            ("outcome", Json::str("ok")),
            ("result", r.clone()),
        ]));
        Ok(r.clone())
    } else {
        let err = resp.get("error").cloned().unwrap_or(Json::Null);
        let kind = err
            .get("data")
            .and_then(|d| d.get("kind"))
            .or_else(|| err.get("data"))
            .cloned()
            .unwrap_or(Json::Null);
        log.push(Json::obj([
            ("method", Json::str(method)),
            ("outcome", Json::str("error")),
            ("error", err.clone()),
        ]));
        Err(RecipeError::Boundary {
            method: method.to_string(),
            detail: Json::obj([("error", err), ("kind", kind)]),
        })
    }
}

fn owner_log(log: &mut Vec<Json>, seam: &'static str, outcome: &str, detail: Json) {
    log.push(Json::obj([
        ("seam", Json::str(seam)),
        ("outcome", Json::str(outcome)),
        ("detail", detail),
    ]));
}

/// The durable execution evidence gate — `lifecycle.run.finished`
/// present plus the observed `hh.submit` submission ref (any row's
/// `submission_ref` member — the `control.decision`/observed chain the
/// completion mints). A process-only run never satisfies it.
fn submission_evidence(svc: &EmbedService, run_id: &str) -> (bool, Option<String>) {
    let events = match svc.store().events(run_id) {
        Ok(e) => e,
        Err(_) => return (false, None),
    };
    let finished = events.iter().any(|e| e.class == "lifecycle.run.finished");
    let evidence = events
        .iter()
        .rev()
        .find_map(|e| {
            e.payload
                .get("submission_ref")
                .and_then(Json::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            events
                .iter()
                .rev()
                .find(|e| e.class == "lifecycle.run.finished")
                .map(|e| e.event_id.clone())
        });
    (finished, evidence)
}

/// The `measurement.experiment.run_settled` event id the engine
/// recorded for (`run_plan_id`, `run_id`) on the experiment run — the
/// settlement's actual durable ref.
fn settled_ref(svc: &EmbedService, experiment_run_id: &str, run_id: &str) -> Option<String> {
    svc.store()
        .events(experiment_run_id)
        .ok()?
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.experiment.run_settled"
                && e.payload.get("run_id").and_then(Json::as_str) == Some(run_id)
        })
        .map(|e| e.event_id.clone())
}

fn str_at<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

/// `run_recipe(service, plan)` — drive the one supported composition on
/// a caller-opened in-process boundary (binding (a); the same `handle`
/// the stdio/loopback bindings share). Scratch stores only — the recipe
/// mutates whatever store the service owns.
pub fn run_recipe(svc: &mut EmbedService, plan: &RecipePlan) -> Result<RecipeReceipt, RecipeError> {
    let mut requests: Vec<Json> = Vec::new();
    let mut owner_calls: Vec<Json> = Vec::new();

    // ── negotiate ────────────────────────────────────────────────────
    let hello = call(svc, &mut requests, "hello", plan.hello.clone())?;

    // ── launch ───────────────────────────────────────────────────────
    let reg = call(
        svc,
        &mut requests,
        "lab.experiment.register",
        plan.register.clone(),
    )?;
    let eid = str_at(&reg, "experiment_id")
        .ok_or_else(|| refused(code::TYPE_MISMATCH, "register: no experiment_id"))?
        .to_string();
    // The engine's resolver bag (`Bag::from_params`) is decoded per op —
    // every `lab.experiment.*` call carries the register body's resolver
    // members (budgets/suite_tasks/arm_configs/…) verbatim, minus `spec`.
    let mut resolvers = match &plan.register {
        Json::Obj(m) => m.clone(),
        _ => BTreeMap::new(),
    };
    resolvers.remove("spec");
    let exp_params = |extra: &[(&str, Json)]| {
        let mut p = resolvers.clone();
        p.insert("experiment_id".to_string(), Json::str(&eid));
        for (k, v) in extra {
            p.insert(k.to_string(), v.clone());
        }
        Json::Obj(p)
    };
    call(svc, &mut requests, "lab.experiment.expand", exp_params(&[]))?;
    let opened = call(
        svc,
        &mut requests,
        "lab.experiment.open_experiment",
        exp_params(&[]),
    )?;
    let exp_run = str_at(&opened, "experiment_run_id")
        .ok_or_else(|| refused(code::TYPE_MISMATCH, "open: no experiment_run_id"))?
        .to_string();
    let docs = LabDocs::open(svc.store().root()).map_err(|e| RecipeError::Owner {
        seam: "docs",
        detail: format!("{e:?}"),
    })?;

    // ── drain: claim → launch → adopt → execute → grade → settle ─────
    struct Leg {
        rpid: String,
        run_id: String,
        arm_id: String,
        attempt_no: u32,
        session_id: String,
        submission_ref: String,
        grade_ref: String,
        outcome_class: String,
    }
    let mut legs: Vec<Leg> = Vec::new();
    let ports = AdoptPorts {
        host_caps: &plan.host_capabilities,
    };
    for _ in 0..plan.max_drain {
        let n = call(svc, &mut requests, "lab.experiment.next", exp_params(&[]))?;
        match str_at(&n, "verdict") {
            Some("done") => break,
            Some("plan") => {}
            other => {
                return Err(refused(
                    code::DRAIN_STALLED,
                    format!("next verdict {other:?} — bounded recipe does not wait"),
                ));
            }
        }
        let rpid = str_at(&n, "run_plan_id")
            .ok_or_else(|| refused(code::TYPE_MISMATCH, "next: no run_plan_id"))?
            .to_string();
        let ticket = call(
            svc,
            &mut requests,
            "lab.experiment.claim",
            exp_params(&[("run_plan_id", Json::str(&rpid))]),
        )?;
        let lease_id = str_at(&ticket, "lease_id")
            .ok_or_else(|| refused(code::TYPE_MISMATCH, "claim: no lease_id"))?
            .to_string();
        let launch = call(
            svc,
            &mut requests,
            "lab.experiment.launch",
            exp_params(&[
                ("run_plan_id", Json::str(&rpid)),
                ("lease_id", Json::str(&lease_id)),
            ]),
        )?;
        let run_id = str_at(&launch, "run_id")
            .ok_or_else(|| refused(code::TYPE_MISMATCH, "launch: no run_id"))?
            .to_string();
        let attempt_no = launch.get("attempt_no").and_then(Json::as_int).unwrap_or(0) as u32;
        // ── adopt (owner seam — not an RPC) ──────────────────────────
        // The durable binding re-read: `launch_binding` through the same
        // `ExperimentView::fold` `adopt_subject` validates against.
        let (binding, arm_id, replicate_index) = {
            let view = ExperimentView::fold(svc.store().events(&exp_run).map_err(|e| {
                RecipeError::Owner {
                    seam: "adopt",
                    detail: format!("{e:?}"),
                }
            })?);
            let ps = view.plans.get(&rpid).ok_or_else(|| RecipeError::Owner {
                seam: "adopt",
                detail: format!("no plan state for {rpid}"),
            })?;
            let b = ps
                .launch_binding
                .clone()
                .ok_or_else(|| RecipeError::Owner {
                    seam: "adopt",
                    detail: format!("no launch_binding for {rpid}"),
                })?;
            (b, ps.arm_id.clone(), ps.replicate_index)
        };
        let w = launch.get("subject_writer").cloned().unwrap_or(Json::Null);
        let writer = Lease {
            run_id: run_id.clone(),
            lease_id: str_at(&w, "lease_id").unwrap_or_default().to_string(),
            holder: str_at(&w, "holder").unwrap_or_default().to_string(),
            generation: w.get("generation").and_then(Json::as_int).unwrap_or(0) as u64,
            expires_at_ms: w.get("expires_at_ms").and_then(Json::as_int).unwrap_or(0) as u64,
        };
        let adopted = svc
            .adopt_subject(
                AdoptBinding {
                    experiment_run_id: &exp_run,
                    run_plan_id: &rpid,
                    binding: &binding,
                    subject_writer: Some(&writer),
                },
                &plan.environment,
                &ports,
            )
            .map_err(|e| RecipeError::Owner {
                seam: "adopt",
                detail: format!("{e:?}"),
            })?;
        owner_log(
            &mut owner_calls,
            "adopt",
            "ok",
            Json::obj([
                ("session_id", Json::str(&adopted.session_id)),
                ("subject_run_id", Json::str(&adopted.subject_run_id)),
                ("intent_ref", Json::str(&adopted.intent_ref)),
                ("armed", Json::str(&adopted.armed)),
                (
                    "writer_generation",
                    Json::Int(adopted.writer_generation as i64),
                ),
            ]),
        );
        // ── execute — the scripted lane reaches `hh.submit` ──────────
        call(
            svc,
            &mut requests,
            "submit",
            Json::obj([
                ("session_id", Json::str(&adopted.session_id)),
                ("input", plan.subject_input.clone()),
                (
                    "idempotency_key",
                    Json::str(format!("recipe-submit-{rpid}")),
                ),
            ]),
        )?;
        let (finished, evidence_ref) = submission_evidence(svc, &run_id);
        if !finished {
            return Err(refused(
                code::EXECUTION_INCOMPLETE,
                format!("{run_id} has no durable lifecycle.run.finished"),
            ));
        }
        let submission_ref = evidence_ref.ok_or_else(|| {
            refused(
                code::NO_SUBMISSION,
                format!("{run_id} finished with no durable evidence ref"),
            )
        })?;
        // ── grade — the declared instrument verdict, gated on the
        // evidence the execution leg actually produced ────────────────
        let verdict_bool = plan
            .grades
            .get(&arm_id)
            .and_then(|v| v.get(replicate_index as usize))
            .copied()
            .ok_or_else(|| {
                refused(
                    code::GRADE_UNSPECIFIED,
                    format!("no grade declared for {arm_id} replicate {replicate_index}"),
                )
            })?;
        let metric = MetricValue {
            metric_ref: plan.grade_metric.clone(),
            value: MetricValueKind::Bool(verdict_bool),
            applies_to: run_id.clone(),
            oracle_ref: RECIPE_VERSION.to_string(),
            detector: hharness_ontology::compliance::Detector::Deterministic,
            confidence: None,
            evidence_ref: Some(submission_ref.clone()),
            calibration_ref: None,
            exploratory: None,
        };
        // The instrument verdict lands durable through the kernel-direct
        // seam — `commit_kernel_row_for` rides the run's persisted writer
        // generation (the same path `measurement.export.delivered` uses);
        // `measurement.emit_metric` only serves a live session and the
        // subject's run already finished inside `submit`'s drive.
        let env = svc
            .surface_commit_row(
                RECIPE_VERSION,
                &run_id,
                "measurement.metric.emitted",
                metric.to_json(),
                vec![],
                vec![],
            )
            .map_err(|e| RecipeError::Owner {
                seam: "grade",
                detail: format!("{e:?}"),
            })?;
        let grade_ref = env.event_id.clone();
        owner_log(
            &mut owner_calls,
            "grade",
            "ok",
            Json::obj([
                ("run_id", Json::str(&run_id)),
                ("metric_ref", Json::str(&plan.grade_metric)),
                ("event_id", Json::str(&grade_ref)),
            ]),
        );
        let settled = call(
            svc,
            &mut requests,
            "lab.experiment.settle",
            exp_params(&[("run_plan_id", Json::str(&rpid))]),
        )?;
        legs.push(Leg {
            rpid,
            run_id,
            arm_id,
            attempt_no,
            session_id: adopted.session_id,
            submission_ref,
            grade_ref,
            outcome_class: str_at(&settled, "outcome_class")
                .unwrap_or_default()
                .to_string(),
        });
    }

    // ── close ────────────────────────────────────────────────────────
    let closed = call(svc, &mut requests, "lab.experiment.close", exp_params(&[]))?;
    let close_status = str_at(&closed, "status").unwrap_or_default().to_string();

    // ── projection — the results owner's own surface ─────────────────
    let results =
        ResultsStore::open(svc.store().root().join("results")).map_err(|e| RecipeError::Owner {
            seam: "project",
            detail: format!("{e:?}"),
        })?;
    let mut slots: Vec<SlotRecord> = Vec::new();
    let mut rows: Vec<ResultsRow> = Vec::new();
    for leg in &legs {
        let (row, _ver) = results
            .project_and_record(
                svc.store(),
                Some(&docs),
                &leg.run_id,
                None,
                None,
                DerivedReason::Initial,
            )
            .map_err(|e| RecipeError::Owner {
                seam: "project",
                detail: format!("{}: {e:?}", leg.run_id),
            })?;
        owner_log(
            &mut owner_calls,
            "project",
            "ok",
            Json::obj([
                ("run_id", Json::str(&leg.run_id)),
                ("row_key_id", Json::str(row.key.key_id())),
                ("version_id", Json::str(&row.version_id)),
            ]),
        );
        slots.push(SlotRecord {
            run_plan_id: leg.rpid.clone(),
            run_id: leg.run_id.clone(),
            arm_id: leg.arm_id.clone(),
            attempt_no: leg.attempt_no,
            session_id: leg.session_id.clone(),
            submission_ref: leg.submission_ref.clone(),
            grade_ref: leg.grade_ref.clone(),
            outcome_class: leg.outcome_class.clone(),
            row_key_id: row.key.key_id(),
            row_version_id: row.version_id.clone(),
        });
        rows.push(row);
    }

    // ── persisted analysis — rows[] derived from the projections ─────
    let mut analysis_params = plan.analysis.clone();
    if let Json::Obj(m) = &mut analysis_params {
        m.insert(
            "rows".to_string(),
            Json::Arr(rows.iter().map(|r| Json::str(r.key.key_id())).collect()),
        );
    }
    let analyzed = call(
        svc,
        &mut requests,
        "lab.analysis.analyze",
        analysis_params.clone(),
    )?;
    let record = analyzed.get("record").cloned().unwrap_or(Json::Null);
    let body = analyzed.get("body").cloned().unwrap_or(Json::Null);
    let analysis_ref = Json::obj([
        (
            "record_ref",
            record.get("analysis_id").cloned().unwrap_or(Json::Null),
        ),
        (
            "report_ref",
            body.get("report_id").cloned().unwrap_or(Json::Null),
        ),
    ]);

    // ── notifications — drained verbatim, or typed inapplicability ───
    let drained = svc.drain_notifications();
    let notifications = if plan.host_capabilities.is_empty() {
        Json::obj([
            ("inapplicable", Json::Bool(true)),
            ("reason", Json::str("n/a{no_callback_channels}")),
            ("drained", Json::Arr(drained)),
        ])
    } else {
        Json::obj([
            ("inapplicable", Json::Bool(false)),
            ("drained", Json::Arr(drained)),
        ])
    };

    // ── receipt — declared side derived from actual durable facts ────
    let view =
        ExperimentView::fold(
            svc.store()
                .events(&exp_run)
                .map_err(|e| RecipeError::Owner {
                    seam: "receipt",
                    detail: format!("{e:?}"),
                })?,
        );
    let mut pins: BTreeMap<String, u64> = BTreeMap::new();
    let mut manifests: BTreeMap<String, RunManifest> = BTreeMap::new();
    let mut facts: BTreeMap<String, LedgerFacts> = BTreeMap::new();
    let mut watermark_set = WatermarkSet::new();
    for leg in &legs {
        let head = svc
            .store()
            .head(&leg.run_id)
            .map_err(|e| RecipeError::Owner {
                seam: "receipt",
                detail: format!("{}: {e:?}", leg.run_id),
            })?;
        pins.insert(leg.run_id.clone(), head.seq as u64);
        manifests.insert(
            leg.run_id.clone(),
            svc.store()
                .manifest(&leg.run_id)
                .map_err(|e| RecipeError::Owner {
                    seam: "receipt",
                    detail: format!("{}: {e:?}", leg.run_id),
                })?
                .clone(),
        );
        let evs: Vec<(u64, String, Json)> = svc
            .store()
            .events(&leg.run_id)
            .map_err(|e| RecipeError::Owner {
                seam: "receipt",
                detail: format!("{}: {e:?}", leg.run_id),
            })?
            .iter()
            .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
            .collect();
        facts.insert(leg.run_id.clone(), LedgerFacts::from_events(&evs));
        for (rid, seq) in rows
            .iter()
            .find(|r| r.key.run_id == leg.run_id)
            .map(|r| &r.watermark_set.runs)
            .into_iter()
            .flatten()
        {
            watermark_set.pin(rid, *seq);
        }
    }
    let tasks: Vec<TaskContext> = match analysis_params.get("tasks") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|t| {
                Ok(TaskContext {
                    task_id: str_at(t, "task_id")
                        .ok_or_else(|| refused(code::MISSING_FIELD, "analysis.tasks.task_id"))?
                        .to_string(),
                    suite_id: str_at(t, "suite_id")
                        .ok_or_else(|| refused(code::MISSING_FIELD, "analysis.tasks.suite_id"))?
                        .to_string(),
                    split_label: str_at(t, "split_label")
                        .and_then(SplitLabel::parse)
                        .ok_or_else(|| {
                            refused(code::TYPE_MISMATCH, "analysis.tasks.split_label")
                        })?,
                    split_hash: str_at(t, "split_hash").unwrap_or_default().to_string(),
                    stratum: str_at(t, "stratum")
                        .and_then(ContaminationStratum::parse)
                        .ok_or_else(|| refused(code::TYPE_MISMATCH, "analysis.tasks.stratum"))?,
                })
            })
            .collect::<Result<Vec<_>, RecipeError>>()?,
        _ => Vec::new(),
    };
    let suites: Vec<SuiteContext> = match analysis_params.get("suites") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|s| {
                Ok(SuiteContext {
                    suite_id: str_at(s, "suite_id")
                        .ok_or_else(|| refused(code::MISSING_FIELD, "analysis.suites.suite_id"))?
                        .to_string(),
                    retired_for_headline: matches!(
                        s.get("retired_for_headline"),
                        Some(Json::Bool(true))
                    ),
                    family: str_at(s, "family")
                        .and_then(hharness_ontology::lab::EnvironmentFamily::parse)
                        .ok_or_else(|| refused(code::TYPE_MISMATCH, "analysis.suites.family"))?,
                })
            })
            .collect::<Result<Vec<_>, RecipeError>>()?,
        _ => Vec::new(),
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
        metric_registry_version: analysis_params
            .get("metric_registry_version")
            .and_then(Json::as_str)
            .unwrap_or(""),
        price_table_version: analysis_params
            .get("price_table_version")
            .and_then(Json::as_str)
            .unwrap_or(""),
        confidence_ppm: analysis_params
            .get("confidence_ppm")
            .and_then(Json::as_int)
            .unwrap_or(950_000),
        resampling_draws: analysis_params
            .get("resampling_draws")
            .and_then(Json::as_int)
            .unwrap_or(0) as u64,
        seed: analysis_params
            .get("seed")
            .and_then(Json::as_int)
            .unwrap_or(0) as u64,
        post_amendment: !view.amendments.is_empty(),
        view: Some(&view),
    };
    let (eval_runs, _not_run) = project_runs(&input).map_err(|e| RecipeError::Owner {
        seam: "receipt",
        detail: format!("{e:?}"),
    })?;
    let mut attempts_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for leg in &legs {
        attempts_map
            .entry(leg.rpid.clone())
            .or_default()
            .push(leg.run_id.clone());
    }
    let cost_version = plan.cost_version.clone().unwrap_or_default();
    let selection_ref = plan.selection_ref.clone().unwrap_or_default();
    let declared = DeclaredReceipt {
        slots: view.plans.keys().cloned().collect(),
        attempts: attempts_map,
        settlements: legs
            .iter()
            .map(|leg| SettlementDecl {
                run_id: leg.run_id.clone(),
                settlement_ref: settled_ref(svc, &exp_run, &leg.run_id).unwrap_or_default(),
                cost_version: cost_version.clone(),
            })
            .collect(),
        grades: legs
            .iter()
            .map(|leg| GradeDecl {
                run_id: leg.run_id.clone(),
                grade_ref: leg.grade_ref.clone(),
                selection_ref: selection_ref.clone(),
                ordinal: 1,
                counts_subject: true,
                status: GradeStatus::Selected,
            })
            .collect(),
        rows: legs
            .iter()
            .map(|leg| RowDecl {
                run_id: leg.run_id.clone(),
                version_id: slots
                    .iter()
                    .find(|s| s.run_id == leg.run_id)
                    .map(|s| s.row_version_id.clone())
                    .unwrap_or_default(),
                plan_ref: leg.rpid.clone(),
            })
            .collect(),
        cost_version,
        selection_ref,
        analysis: AnalysisDecl {
            protocol_ref: plan.protocol_ref.clone(),
            prices: Json::obj([(
                "price_table_version",
                record
                    .get("price_table_version")
                    .cloned()
                    .unwrap_or_else(|| {
                        analysis_params
                            .get("price_table_version")
                            .cloned()
                            .unwrap_or(Json::Null)
                    }),
            )]),
            method: Json::obj([
                (
                    "estimator",
                    analysis_params
                        .get("spec")
                        .and_then(|s| s.get("kind"))
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
                (
                    "version",
                    record
                        .get("procedure")
                        .cloned()
                        .unwrap_or_else(|| Json::str("unversioned")),
                ),
            ]),
            seed: analysis_params
                .get("seed")
                .and_then(Json::as_int)
                .unwrap_or(0) as u64,
            draws: analysis_params
                .get("resampling_draws")
                .and_then(Json::as_int)
                .unwrap_or(0) as u64,
            weights: plan.weights.clone(),
            spec_ref: str_at(
                &analysis_params.get("spec").cloned().unwrap_or(Json::Null),
                "spec_ref",
            )
            .map(str::to_string)
            .or(Some(eid.clone())),
            record_ref: record
                .get("analysis_id")
                .and_then(Json::as_str)
                .map(str::to_string),
            report_ref: body
                .get("report_id")
                .and_then(Json::as_str)
                .map(str::to_string),
        },
    };
    let receipt = assemble_receipt(
        &view,
        &eval_runs,
        &tasks,
        &plan.grade_metric,
        &pins,
        &declared,
    )
    .map_err(|e| RecipeError::Owner {
        seam: "receipt",
        detail: format!("{e:?}"),
    })?;
    owner_log(
        &mut owner_calls,
        "receipt",
        "ok",
        Json::obj([("verdict", Json::str(receipt.verdict))]),
    );

    Ok(RecipeReceipt {
        recipe_version: RECIPE_VERSION,
        hello,
        requests,
        owner_calls,
        notifications,
        experiment_id: eid,
        experiment_run_id: exp_run,
        close_status,
        slots,
        analysis_ref,
        receipt,
    })
}
