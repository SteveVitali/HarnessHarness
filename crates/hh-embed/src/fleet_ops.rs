//! `fleet.*` dispatch — §5i.1's human-agent organizational layer (S4.9;
//! R-2.12.6; ADR-0205 D8 record family + ADR-0206 D1 reconciliation
//! family). The boundary is records-in/records-out like `lab.experiment.*`:
//! the caller supplies the spec and the pinned source document each call
//! (`source` is the `hh.fleet.fixture/1` doc; absent = the empty fixture),
//! the engine consumes the existing `Store` directly — one ledger, one
//! authority path, no private store.
//!
//! Writer leases persist across calls in `self.fleet_engines`
//! (`fleet_run → FleetEngine`); a restart re-acquires through the normal
//! fence (ADR-0130). Built without `tier-c4`, the whole surface answers
//! `Unsupported{by: "tier-c4"}` — a typed refusal, never silent degrade
//! (CC6/removability).

#[cfg(feature = "tier-c4")]
use std::collections::BTreeMap;

#[cfg(feature = "tier-c4")]
use hh_ledger::store::Store;
use hh_embed_schema::errors::EmbedError;
#[cfg(feature = "tier-c4")]
use hh_embed_schema::types::NarrowingLeaf;
use hh_wire::json::Json;

use crate::service::EmbedService;

#[cfg(feature = "tier-c4")]
use hh_fleet::{
    engine::FleetEngine,
    errors::FleetError,
    source::FixtureAdapter,
    spec::FleetSpec,
    work_item::{derive_state, WorkItemInit, WorkItemView},
};

#[cfg(feature = "tier-c4")]
const FLEET_HOLDER: &str = "hh-embed:fleet";
#[cfg(feature = "tier-c4")]
const WRITER_TTL_MS: u64 = 60_000;

#[cfg(feature = "tier-c4")]
fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

#[cfg(feature = "tier-c4")]
fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

#[cfg(feature = "tier-c4")]
fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, EmbedError> {
    req(j, k)?
        .as_str()
        .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
}

#[cfg(feature = "tier-c4")]
fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

#[cfg(feature = "tier-c4")]
fn str_list(j: &Json, k: &str) -> Result<Vec<String>, EmbedError> {
    match j.get(k) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
            })
            .collect(),
        _ => Err(bad(&format!("/{k}"), "type_mismatch")),
    }
}

/// The folded item view rendered as a record (one scheme — the same
/// members `fleet.fleet_view`'s `items[]` carries).
#[cfg(feature = "tier-c4")]
fn item_json(it: &WorkItemView) -> Json {
    let mut m = BTreeMap::new();
    m.insert("item_id".into(), Json::str(&it.item_id));
    m.insert("run_item_id".into(), Json::str(&it.run_item_id));
    m.insert("title".into(), Json::str(&it.title));
    m.insert("state".into(), Json::str(&derive_state(it)));
    m.insert("spec_ref".into(), Json::str(&it.spec_ref));
    m.insert("source".into(), it.source.clone());
    m.insert("idempotency_key".into(), Json::str(&it.idempotency_key));
    m.insert(
        "owner".into(),
        it.owner.as_ref().map(Json::str).unwrap_or(Json::Null),
    );
    m.insert("owner_ack".into(), Json::Bool(it.owner_ack));
    m.insert(
        "lease_agent_ref".into(),
        it.lease_agent_ref
            .as_ref()
            .map(Json::str)
            .unwrap_or(Json::Null),
    );
    m.insert(
        "blocking".into(),
        Json::Arr(it.blocking.iter().map(Json::str).collect()),
    );
    m.insert(
        "blocked".into(),
        Json::Arr(it.blocked.iter().map(Json::str).collect()),
    );
    m.insert("suspended".into(), Json::Bool(it.suspended));
    m.insert("watch_state".into(), Json::str(&it.watch_state));
    if let Some(s) = &it.settlement {
        m.insert("outcome".into(), Json::str(&s.outcome));
        m.insert(
            "evidence_refs".into(),
            Json::Arr(s.evidence_refs.iter().map(Json::str).collect()),
        );
    }
    if let Some(r) = &it.dispatch.run_ref {
        m.insert("run_ref".into(), Json::str(r));
    }
    Json::Obj(m)
}

/// The boundary adapter — the pinned fixture document each call carries
/// (`{run, source?}`; absent = the empty fixture, so driver-only ops do
/// not resupply it).
#[cfg(feature = "tier-c4")]
fn adapter(params: &Json) -> Result<FixtureAdapter, EmbedError> {
    let doc = params.get("source").cloned().unwrap_or_else(|| {
        Json::obj([("schema_version", Json::str("hh.fleet.fixture/1"))])
    });
    FixtureAdapter::from_doc(doc).map_err(fleet_err)
}

/// `FleetError` → the closed `EmbedError` sum — `Refused{reason}` is the
/// engine's refused-code carrier (one scheme; the refusal family names
/// its rule verbatim).
#[cfg(feature = "tier-c4")]
fn fleet_err(e: FleetError) -> EmbedError {
    match e {
        FleetError::Store(hh_ledger::errors::LedgerError::UnknownRun { run_id }) => {
            EmbedError::UnknownRun { run_id }
        }
        FleetError::Store(hh_ledger::errors::LedgerError::WouldBlock { active_holder }) => {
            EmbedError::WouldBlock { active_holder }
        }
        FleetError::SchemaViolation { detail } => EmbedError::SchemaViolation {
            path: "/params".to_string(),
            code: detail,
        },
        FleetError::MissingBudgetRef => EmbedError::UnbudgetedArm,
        FleetError::Account(hh_budget::errors::BudgetError::InsufficientBudget {
            dimension,
            ..
        }) => EmbedError::InsufficientBudget {
            dimension: Some(format!("{dimension:?}")),
        },
        other => EmbedError::Refused {
            reason: other.code(),
        },
    }
}

/// `fleet_engine(engines, store, run) → &mut FleetEngine` — get-or-ensure
/// the activation's engine (rebuilds the fold over the durable prefix on
/// first touch; the writer lease persists in the map). A free function so
/// call sites borrow `self.fleet_engines`/`self.store` as disjoint fields.
#[cfg(feature = "tier-c4")]
fn fleet_engine<'a>(
    engines: &'a mut BTreeMap<String, FleetEngine>,
    store: &mut Store,
    run_id: &str,
) -> Result<&'a mut FleetEngine, EmbedError> {
    if !engines.contains_key(run_id) {
        let eng = FleetEngine::ensure(store, run_id, FLEET_HOLDER, WRITER_TTL_MS)
            .map_err(fleet_err)?;
        engines.insert(run_id.to_string(), eng);
    }
    Ok(engines.get_mut(run_id).unwrap())
}

#[cfg(feature = "tier-c4")]
impl EmbedService {

    /// The `fleet.*` dispatch — one arm for the whole surface; the op
    /// names are the registry's (single source, CC7).
    pub(crate) fn fleet_dispatch(
        &mut self,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        match method {
            "fleet.open" => {
                let spec = FleetSpec::from_json(req(params, "spec")?).map_err(fleet_err)?;
                let (run_id, eng) =
                    FleetEngine::open(&mut self.store, FLEET_HOLDER, WRITER_TTL_MS, spec)
                        .map_err(fleet_err)?;
                self.fleet_engines.insert(run_id.clone(), eng);
                Ok(Json::obj([("run_id", Json::str(&run_id))]))
            }
            "fleet.ensure" => {
                let run = req_str(params, "run")?;
                fleet_engine(&mut self.fleet_engines, &mut self.store, run)?;
                Ok(Json::obj([("run_id", Json::str(run))]))
            }
            "fleet.restore" => {
                let run = req_str(params, "run")?.to_string();
                let ad = adapter(params)?;
                let now = now_ms(params, self.store.now_ms());
                let (eng, report) =
                    FleetEngine::restore(&mut self.store, &run, FLEET_HOLDER, WRITER_TTL_MS, &ad, now)
                        .map_err(fleet_err)?;
                self.fleet_engines.insert(run.clone(), eng);
                Ok(report_json(&report))
            }
            "fleet.observe" => {
                let run = req_str(params, "run")?.to_string();
                let ad = adapter(params)?;
                let now = now_ms(params, self.store.now_ms());
                let observed = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .observe(&mut self.store, &ad, now)
                    .map_err(fleet_err)?;
                Ok(Json::obj([(
                    "observed",
                    Json::Arr(observed.iter().map(Json::str).collect()),
                )]))
            }
            "fleet.reconcile" => {
                let run = req_str(params, "run")?.to_string();
                let ad = adapter(params)?;
                let now = now_ms(params, self.store.now_ms());
                let report = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .reconcile(&mut self.store, &ad, now)
                    .map_err(fleet_err)?;
                Ok(report_json(&report))
            }
            "fleet.create_work_item" => {
                let run = req_str(params, "run")?.to_string();
                let init = WorkItemInit::from_json(req(params, "item")?).map_err(fleet_err)?;
                match fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .admit(&mut self.store, init)
                    .map_err(fleet_err)?
                {
                    hh_fleet::engine::AdmitOutcome::Admitted { item_id, event_id } => {
                        Ok(Json::obj([
                            ("status", Json::str("admitted")),
                            ("item_id", Json::str(&item_id)),
                            ("event_id", Json::str(&event_id)),
                        ]))
                    }
                    hh_fleet::engine::AdmitOutcome::Known { item_id } => Ok(Json::obj([
                        ("status", Json::str("known")),
                        ("item_id", Json::str(&item_id)),
                    ])),
                }
            }
            "fleet.bind_source" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let binding = req(params, "binding")?.clone();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .bind_source(&mut self.store, &item, binding)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.claim" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .claim(&mut self.store, &item)
                    .map_err(fleet_err)
            }
            "fleet.dispatch" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let ad = adapter(params)?;
                let now = now_ms(params, self.store.now_ms());
                let mut report = hh_fleet::engine::ReconcileReport::default();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .dispatch(&mut self.store, &ad, &item, now, &mut report)
                    .map_err(fleet_err)?;
                Ok(report_json(&report))
            }
            "fleet.dispatch_note" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let spec_ref = req_str(params, "spec_ref")?.to_string();
                let run_ref = opt_str(params, "run_ref");
                let error = opt_str(params, "error");
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .dispatch_note(
                        &mut self.store,
                        &item,
                        &spec_ref,
                        run_ref.as_deref(),
                        error.as_deref(),
                    )
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.settle" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let outcome = req_str(params, "outcome")?.to_string();
                let evidence = str_list(params, "evidence_refs")?;
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .settle(&mut self.store, &item, &outcome, evidence)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.handoff" => {
                // The canonical state handoff — the item enters the
                // human-gate state (`blocked{human_gate}`; only a human
                // grant resumes it — `resume_from_handoff`).
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .block(&mut self.store, &item, "human_gate")
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.resume_from_handoff" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let by = req_str(params, "by")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .resume_from_handoff(&mut self.store, &item, &by)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.transfer_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let to = req_str(params, "to")?.to_string();
                let basis = req_str(params, "basis")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .transfer_owner(&mut self.store, &item, &to, &basis)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.acknowledge_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let agent = req_str(params, "agent")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .ack_owner(&mut self.store, &item, &agent)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.cancel" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let by = req_str(params, "by")?.to_string();
                let cascade = opt_str(params, "cascade").unwrap_or_else(|| "self".into());
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .cancel(&mut self.store, &item, &by, &cascade)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.annotate" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let subject = req_str(params, "subject")?.to_string();
                let subject_ref = params.get("subject_ref").cloned();
                let text_ref = req_str(params, "text_ref")?.to_string();
                let readers = str_list(params, "readers")?;
                let by = req_str(params, "by")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .annotate(
                        &mut self.store,
                        &item,
                        &subject,
                        subject_ref.as_ref(),
                        &text_ref,
                        &readers,
                        &by,
                    )
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.block" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let code = req_str(params, "code")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .block(&mut self.store, &item, &code)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.unblock" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let code = req_str(params, "code")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .unblock(&mut self.store, &item, &code)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.stop" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .stop(&mut self.store, &item)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.set_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let agent = req_str(params, "agent")?.to_string();
                let owner = req_str(params, "owner")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .set_owner(&mut self.store, &item, &agent, &owner)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.escalate" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let issue = req_str(params, "issue")?.to_string();
                let by = req_str(params, "by")?.to_string();
                let to = opt_str(params, "to");
                let deadline = params.get("deadline_ms").and_then(Json::as_int);
                let issue_ref = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .escalate(
                        &mut self.store,
                        &item,
                        &issue,
                        &by,
                        to.as_deref(),
                        deadline.map(|d| d as u64),
                    )
                    .map_err(fleet_err)?;
                Ok(Json::obj([("issue_ref", Json::str(&issue_ref))]))
            }
            "fleet.resolve_escalation" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let issue_ref = req_str(params, "issue_ref")?.to_string();
                let by = req_str(params, "by")?.to_string();
                let resolution = req_str(params, "resolution")?.to_string();
                let note = opt_str(params, "note");
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .resolve_escalation(
                        &mut self.store,
                        &item,
                        &issue_ref,
                        &by,
                        &resolution,
                        note.as_deref(),
                    )
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.work_item" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let view = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .work_item(&item)
                    .map_err(fleet_err)?;
                Ok(item_json(&view))
            }
            "fleet.list" => {
                let run = req_str(params, "run")?.to_string();
                let items: Vec<Json> = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .list()
                    .iter()
                    .map(item_json)
                    .collect();
                Ok(Json::obj([("items", Json::Arr(items))]))
            }
            "fleet.fleet_view" => Ok(fleet_engine(&mut self.fleet_engines, &mut self.store, req_str(params, "run")?)?.fleet_view()),
            "fleet.state_map" => {
                let run = req_str(params, "run")?.to_string();
                let item = opt_str(params, "item");
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .state_map(item.as_deref())
                    .map_err(fleet_err)
            }
            "fleet.accountability_record" => {
                let run = req_str(params, "run")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .accountability_record(&mut self.store)
                    .map_err(fleet_err)
            }
            "fleet.audit_link" => {
                let run = req_str(params, "run")?.to_string();
                let obligations = fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .audit_link(&self.store)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("obligations", Json::Arr(obligations))]))
            }
            "fleet.check_activation_delta" => {
                let run = req_str(params, "run")?.to_string();
                let candidate: Vec<NarrowingLeaf> = match req(params, "candidate")? {
                    Json::Arr(a) => a
                        .iter()
                        .map(|v| {
                            NarrowingLeaf::from_json(v, "/candidate").map_err(|e| bad("/candidate", &format!("{e:?}")))
                        })
                        .collect::<Result<_, _>>()?,
                    _ => return Err(bad("/candidate", "type_mismatch")),
                };
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run)?
                    .check_activation_delta(&candidate)
                    .map_err(fleet_err)
            }
            other => Err(EmbedError::Refused {
                reason: format!("unknown_op:{other}"),
            }),
        }
    }
}

/// `ReconcileReport` as a record — the pass's durable-evidence summary.
#[cfg(feature = "tier-c4")]
fn report_json(r: &hh_fleet::engine::ReconcileReport) -> Json {
    Json::obj([
        ("schema", Json::str("hh.fleet.report/1")),
        (
            "observed",
            Json::Arr(r.observed.iter().map(Json::str).collect()),
        ),
        ("fired", Json::Arr(r.fired.iter().map(Json::str).collect())),
        (
            "dispatched",
            Json::Arr(r.dispatched.iter().map(Json::str).collect()),
        ),
        (
            "blocked",
            Json::Arr(r.blocked.iter().map(Json::str).collect()),
        ),
        (
            "escalated",
            Json::Arr(r.escalated.iter().map(Json::str).collect()),
        ),
        (
            "settled",
            Json::Arr(r.settled.iter().map(Json::str).collect()),
        ),
        (
            "cursor",
            Json::obj([
                ("event_count", Json::Int(r.cursor.event_count as i64)),
                ("observed_at_ms", Json::Int(r.cursor.observed_at_ms as i64)),
            ]),
        ),
    ])
}

/// `{now_ms}` — the caller's logical clock wins; absent, the service
/// environment's injected clock (never `std::time` — ADR-0167 D4).
#[cfg(feature = "tier-c4")]
fn now_ms(params: &Json, default: u64) -> u64 {
    params
        .get("now_ms")
        .and_then(Json::as_int)
        .map(|v| v as u64)
        .unwrap_or(default)
}

#[cfg(not(feature = "tier-c4"))]
impl EmbedService {
    /// Tier absent — the ops remain in the schema (CC7) and answer the
    /// typed `tier_unavailable` refusal (CC6 removability).
    pub(crate) fn fleet_dispatch(
        &mut self,
        _method: &str,
        _params: &Json,
    ) -> Result<Json, EmbedError> {
        Err(EmbedError::Unsupported {
            by: "tier-c4".to_string(),
        })
    }
}
