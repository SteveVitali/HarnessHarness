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

use hh_embed_schema::errors::EmbedError;
#[cfg(feature = "tier-c4")]
use hh_embed_schema::types::NarrowingLeaf;
#[cfg(feature = "tier-c4")]
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::service::EmbedService;

#[cfg(feature = "tier-c4")]
use hh_fleet::{
    engine::FleetEngine,
    errors::FleetError,
    source::{FixtureAdapter, WorkSourceAdapter},
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
    m.insert("state".into(), Json::str(derive_state(it)));
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

/// `BoundaryAdapter` — the S5.6 two-lane source selection (§5i.1 #3;
/// ADR-0205 D5). A `source` doc carrying the `webhook` member builds the
/// [`WebhookAdapter`] (the signed-webhook lane — `webhook` is the
/// [`IngressPolicy`] verbatim); anything else is the `hh.fleet.fixture/1`
/// pull lane. One enum, one delegation — the engine's generic
/// `A: WorkSourceAdapter` seam never sees the split.
#[cfg(feature = "tier-c4")]
pub enum BoundaryAdapter {
    /// The pull lane — the pinned fixture document.
    Fixture(FixtureAdapter),
    /// The push lane — signed-webhook ingress plus the shared
    /// suspended/activate_run/records members.
    Webhook(hh_fleet::ingress::WebhookAdapter),
}

#[cfg(feature = "tier-c4")]
impl hh_fleet::source::WorkSourceAdapter for BoundaryAdapter {
    fn occurrences(&mut self, since: Option<u64>) -> Vec<hh_fleet::source::SourceOccurrence> {
        match self {
            BoundaryAdapter::Fixture(a) => a.occurrences(since),
            BoundaryAdapter::Webhook(a) => a.occurrences(since),
        }
    }
    fn suspended(&mut self, source_id: &str) -> bool {
        match self {
            BoundaryAdapter::Fixture(a) => a.suspended(source_id),
            BoundaryAdapter::Webhook(a) => a.suspended(source_id),
        }
    }
    fn activate_run(&mut self, candidates: &[String]) -> Vec<String> {
        match self {
            BoundaryAdapter::Fixture(a) => a.activate_run(candidates),
            BoundaryAdapter::Webhook(a) => a.activate_run(candidates),
        }
    }
    fn list(&mut self, states: &[String]) -> Vec<Json> {
        match self {
            BoundaryAdapter::Fixture(a) => a.list(states),
            BoundaryAdapter::Webhook(a) => a.list(states),
        }
    }
    fn get(&mut self, native_ids: &[String]) -> Vec<Json> {
        match self {
            BoundaryAdapter::Fixture(a) => a.get(native_ids),
            BoundaryAdapter::Webhook(a) => a.get(native_ids),
        }
    }
    fn capabilities(&self) -> hh_fleet::capabilities::AdapterCapabilities {
        match self {
            BoundaryAdapter::Fixture(a) => a.capabilities(),
            BoundaryAdapter::Webhook(a) => a.capabilities(),
        }
    }
    fn fault(&self) -> Option<String> {
        match self {
            BoundaryAdapter::Fixture(a) => a.fault(),
            BoundaryAdapter::Webhook(a) => a.fault(),
        }
    }
}

/// The boundary adapter — the pinned source document each call carries
/// (`{run, source?}`; absent = the empty fixture, so driver-only ops do
/// not resupply it). `source.webhook` selects the signed-webhook lane.
#[cfg(feature = "tier-c4")]
fn adapter(params: &Json, fleet_run: &str) -> Result<BoundaryAdapter, EmbedError> {
    let doc = params
        .get("source")
        .cloned()
        .unwrap_or_else(|| Json::obj([("schema_version", Json::str("hh.fleet.fixture/1"))]));
    if doc.get("webhook").is_some() {
        hh_fleet::ingress::WebhookAdapter::from_doc(&doc, fleet_run)
            .map(BoundaryAdapter::Webhook)
            .map_err(fleet_err)
    } else {
        FixtureAdapter::from_doc(doc)
            .map(BoundaryAdapter::Fixture)
            .map_err(fleet_err)
    }
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
    params: &Json,
) -> Result<&'a mut FleetEngine, EmbedError> {
    if !engines.contains_key(run_id) {
        let eng =
            FleetEngine::ensure(store, run_id, FLEET_HOLDER, WRITER_TTL_MS).map_err(fleet_err)?;
        engines.insert(run_id.to_string(), eng);
    }
    let eng = engines.get_mut(run_id).unwrap();
    // P12 — a boundary-initiated op stamps the declared `requester`
    // member on the rows it writes (`params.requester`, verbatim;
    // absent = no stamp). Process state, never authority.
    eng.set_requester(params.get("requester").cloned());
    Ok(eng)
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, run, params)?;
                Ok(Json::obj([("run_id", Json::str(run))]))
            }
            "fleet.restore" => {
                let run = req_str(params, "run")?.to_string();
                let mut ad = adapter(params, &run)?;
                let now = now_ms(params, self.store.now_ms());
                let (eng, report) = FleetEngine::restore(
                    &mut self.store,
                    &run,
                    FLEET_HOLDER,
                    WRITER_TTL_MS,
                    &mut ad,
                    now,
                )
                .map_err(fleet_err)?;
                self.fleet_engines.insert(run.clone(), eng);
                Ok(report_json(&report))
            }
            "fleet.observe" => {
                let run = req_str(params, "run")?.to_string();
                let mut ad = adapter(params, &run)?;
                let now = now_ms(params, self.store.now_ms());
                let observed =
                    fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                        .observe(&mut self.store, &mut ad, now)
                        .map_err(fleet_err)?;
                Ok(Json::obj([(
                    "observed",
                    Json::Arr(observed.iter().map(Json::str).collect()),
                )]))
            }
            "fleet.reconcile" => {
                let run = req_str(params, "run")?.to_string();
                let mut ad = adapter(params, &run)?;
                let now = now_ms(params, self.store.now_ms());
                let report = fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .reconcile(&mut self.store, &mut ad, now)
                    .map_err(fleet_err)?;
                Ok(report_json(&report))
            }
            "fleet.create_work_item" => {
                let run = req_str(params, "run")?.to_string();
                let init = WorkItemInit::from_json(req(params, "item")?).map_err(fleet_err)?;
                match fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .bind_source(&mut self.store, &item, binding)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.claim" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .claim(&mut self.store, &item)
                    .map_err(fleet_err)
            }
            "fleet.dispatch" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let mut ad = adapter(params, &run)?;
                let now = now_ms(params, self.store.now_ms());
                let mut report = hh_fleet::engine::ReconcileReport::default();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .dispatch(&mut self.store, &mut ad, &item, now, &mut report)
                    .map_err(fleet_err)?;
                Ok(report_json(&report))
            }
            "fleet.dispatch_note" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let spec_ref = req_str(params, "spec_ref")?.to_string();
                let run_ref = opt_str(params, "run_ref");
                let error = opt_str(params, "error");
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .block(&mut self.store, &item, "human_gate")
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.resume_from_handoff" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let by = req_str(params, "by")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .resume_from_handoff(&mut self.store, &item, &by)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.transfer_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let to = req_str(params, "to")?.to_string();
                let basis = req_str(params, "basis")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .transfer_owner(&mut self.store, &item, &to, &basis)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.acknowledge_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let agent = req_str(params, "agent")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .ack_owner(&mut self.store, &item, &agent)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.cancel" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let by = req_str(params, "by")?.to_string();
                let cascade = opt_str(params, "cascade").unwrap_or_else(|| "self".into());
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .block(&mut self.store, &item, &code)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.unblock" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let code = req_str(params, "code")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .unblock(&mut self.store, &item, &code)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.stop" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .stop(&mut self.store, &item)
                    .map_err(fleet_err)?;
                Ok(Json::obj([("ok", Json::Bool(true))]))
            }
            "fleet.set_owner" => {
                let run = req_str(params, "run")?.to_string();
                let item = req_str(params, "item")?.to_string();
                let agent = req_str(params, "agent")?.to_string();
                let owner = req_str(params, "owner")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                let issue_ref =
                    fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
                let view = fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .work_item(&item)
                    .map_err(fleet_err)?;
                Ok(item_json(&view))
            }
            "fleet.list" => {
                let run = req_str(params, "run")?.to_string();
                let items: Vec<Json> =
                    fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                        .list()
                        .iter()
                        .map(item_json)
                        .collect();
                Ok(Json::obj([("items", Json::Arr(items))]))
            }
            "fleet.fleet_view" => {
                let run = req_str(params, "run")?.to_string();
                let eng = fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?;
                let mut v = eng.fleet_view(&mut self.store);
                // S5.6 — the `metrics` member is the catalogue's fleet
                // rows in §5h.1 `MetricDeclaration` form (§5i.1 §6's
                // "the FleetView metrics member — every row …"). Cells
                // fold with the telemetry slice; each row's fold status
                // is declared, never estimated.
                if let Json::Obj(ref mut m) = v {
                    m.insert(
                        "metrics".into(),
                        Json::Arr(
                            hh_telemetry::catalogue::FLEET_METRICS
                                .iter()
                                .map(|fm| fm.declaration().to_json())
                                .collect(),
                        ),
                    );
                }
                Ok(v)
            }
            "fleet.state_map" => {
                let run = req_str(params, "run")?.to_string();
                let item = opt_str(params, "item");
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .state_map(item.as_deref())
                    .map_err(fleet_err)
            }
            "fleet.accountability_record" => {
                let run = req_str(params, "run")?.to_string();
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                    .accountability_record(&mut self.store)
                    .map_err(fleet_err)
            }
            "fleet.audit_link" => {
                let run = req_str(params, "run")?.to_string();
                let obligations =
                    fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
                        .audit_link(&self.store)
                        .map_err(fleet_err)?;
                Ok(Json::obj([("obligations", Json::Arr(obligations))]))
            }
            // ── S5.6: the adapter surface — signed-webhook ingress, the
            // capability probe, and the source read minimum (§5i.1 #3;
            // ADR-0205 D5).
            "fleet.webhook_ingress" => {
                // `{run, source{webhook{…policy…}}, delivery, signature,
                // key, now_ms?}` → `{status, occurrence_id}`. The
                // signature is `hmac-sha256:<hex>` over the delivery's
                // canonical form; `key` is the broker-resolved secret for
                // the policy's `key_ref` — verification only, never
                // copied into a ledger row or a reply record.
                let run = req_str(params, "run")?.to_string();
                let mut ad = adapter(params, &run)?;
                let BoundaryAdapter::Webhook(wh) = &mut ad else {
                    return Err(bad("/source", "webhook_lane_required"));
                };
                let delivery = req(params, "delivery")?;
                let signature = req_str(params, "signature")?;
                let key = req_str(params, "key")?;
                let now = now_ms(params, self.store.now_ms());
                match wh.receive(delivery, signature, key.as_bytes(), now) {
                    Ok(hh_fleet::ingress::IngressOutcome::Received { occurrence_id }) => {
                        Ok(Json::obj([
                            ("status", Json::str("received")),
                            ("occurrence_id", Json::str(&occurrence_id)),
                        ]))
                    }
                    Ok(hh_fleet::ingress::IngressOutcome::Duplicate { occurrence_id }) => {
                        Ok(Json::obj([
                            ("status", Json::str("duplicate")),
                            ("occurrence_id", Json::str(&occurrence_id)),
                        ]))
                    }
                    Err(e) => Err(EmbedError::Refused {
                        reason: e.code().to_string(),
                    }),
                }
            }
            "fleet.source_capabilities" => {
                // The capability probe record — the adapter's declared/
                // probed capability set verbatim (tri-state — `unknown`
                // is never coerced).
                let run = req_str(params, "run")?.to_string();
                let ad = adapter(params, &run)?;
                Ok(Json::obj([
                    ("run", Json::str(&run)),
                    ("capabilities", ad.capabilities().to_json()),
                ]))
            }
            "fleet.source_records" => {
                // The adapter read minimum — `list{states[]}` or
                // `get{native_ids[]}` (`list` wins when both are
                // present); a latched fault rides `source_unavailable`.
                let run = req_str(params, "run")?.to_string();
                let mut ad = adapter(params, &run)?;
                let records = if let Some(Json::Arr(states)) = params.get("states") {
                    let states: Vec<String> = states
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect();
                    ad.list(&states)
                } else if let Some(Json::Arr(ids)) = params.get("native_ids") {
                    let ids: Vec<String> = ids
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect();
                    ad.get(&ids)
                } else {
                    ad.list(&[])
                };
                let mut m = BTreeMap::new();
                m.insert("run".into(), Json::str(&run));
                m.insert("records".into(), Json::Arr(records));
                if let Some(f) = ad.fault() {
                    m.insert("source_unavailable".into(), Json::str(f));
                }
                Ok(Json::Obj(m))
            }
            "fleet.check_activation_delta" => {
                let run = req_str(params, "run")?.to_string();
                let candidate: Vec<NarrowingLeaf> = match req(params, "candidate")? {
                    Json::Arr(a) => a
                        .iter()
                        .map(|v| {
                            NarrowingLeaf::from_json(v, "/candidate")
                                .map_err(|e| bad("/candidate", &format!("{e:?}")))
                        })
                        .collect::<Result<_, _>>()?,
                    _ => return Err(bad("/candidate", "type_mismatch")),
                };
                fleet_engine(&mut self.fleet_engines, &mut self.store, &run, params)?
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
        // S5.6 — a faulted source reports itself (`source_unavailable`
        // naming the adapter fault); absent = healthy (null when none).
        (
            "source_unavailable",
            r.source_unavailable
                .as_ref()
                .map(Json::str)
                .unwrap_or(Json::Null),
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
