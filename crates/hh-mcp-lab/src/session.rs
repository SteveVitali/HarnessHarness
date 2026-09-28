//! The surface session — the `run_kind = surface` run that owns the
//! `lifecycle.turn.*` + `action.effect.*` chain for every `tools/call`
//! (spec §7.3 §2.5: "each `tools/call` is a surface-session turn:
//! `lifecycle.turn.opened/closed` wrap an `action.effect.*` sequence …
//! `prepared → dispatched → observed`").
//!
//! Everything durable lands through the `surface_*` seam on
//! `EmbedService` — the session holds the writer `Lease`, the caller's
//! `CallerBinding`, the handle table (a pure projection of
//! `lifecycle.surface.call.minted` rows), and the turn counter (rebuilt as
//! `max(turn seq)` after a resume — never a guessed continuation).
//!
//! Charges (AC-R-2.11.3-5): every `tools/call` posts
//! `control.budget.consumed{tool_calls: 1}` against
//! `binding.budget_node` — the pool root the session allocated at open —
//! with `charged_to = instrument` (the *server*'s work). A launched
//! run's own spend charges `subject` on its own allocation — the two
//! strata never blur.

use std::collections::BTreeMap;

use hh_budget::account::ChargeRequest;
use hh_budget::attribution::{Attribution, ChargedTo};
use hh_budget::quantity::ResourceQuantity;
use hh_budget::spec::{BudgetMode, BudgetNode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_embed::service::EmbedService;
use hh_embed_schema::errors::EmbedError;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::Lease;
use hh_ontology::dimensions::DimensionId;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::binding::CallerBinding;
use crate::handles::HandleTable;

/// The producer tag on every surface-minted row (`component_class =
/// "kernel"`, `component_variant_ref` = this — the audit partition's
/// Rule-P check admits it).
pub const SURFACE_COMPONENT: &str = "kernel:mcp-lab";

/// The `caller_process.mechanism` the manifest records — a hosted
/// participant sees a `OpaqueProcess` it cannot look inside (CF-356's
/// opaque-adapter row lives on the hosted run's side; the surface's
/// own mechanism is `local_process`-equivalent).
pub const SURFACE_MECHANISM: &str = "local_process";

/// The live surface session.
pub struct SurfaceSession {
    /// The `run_kind = surface` run id.
    pub run_id: String,
    /// The writer lease (generation-fenced appends).
    pub lease: Lease,
    /// The resolved caller binding this session serves.
    pub binding: CallerBinding,
    /// Turns minted so far (rebuilt = `max(turn seq)` — the counter is
    /// durable-derived, never a guess).
    pub turn_no: u64,
    /// The minted-handle table — folded from `lifecycle.surface.call.minted` rows.
    pub handles: HandleTable,
    /// The pool root's budget id (`= binding.budget_node`).
    pub pool_node: String,
}

impl SurfaceSession {
    /// Open a fresh surface session: the `surface` run + the pool-root
    /// `control.budget.allocated` row, both durable before the first
    /// `tools/call` can name them.
    pub fn open(
        svc: &mut EmbedService,
        binding: CallerBinding,
        exposure_version_id: &str,
        catalogue_hash: &str,
        protocols: &[&str],
    ) -> Result<SurfaceSession, EmbedError> {
        let mut manifest = RunManifest::minimal(RunKind::Surface);
        // Non-agent runs carry no configuration/environment cells
        // (ADR-0183 §C — `minimal` seeds the agent-set; a surface run
        // clears it or `open_run`'s invariant refuses
        // `ManifestInvalid`).
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest.environment_ref = None;
        manifest.environment_version_id = None;
        manifest
            .extra
            .insert("caller_binding".to_string(), binding.to_json());
        manifest.extra.insert(
            "exposure_version_id".to_string(),
            Json::str(exposure_version_id),
        );
        manifest
            .extra
            .insert("catalogue_hash".to_string(), Json::str(catalogue_hash));
        manifest.extra.insert(
            "protocol_bindings".to_string(),
            Json::Arr(protocols.iter().map(|p| Json::str(*p)).collect()),
        );
        manifest.extra.insert(
            "caller_process".to_string(),
            Json::obj([
                ("participant_ref", Json::str(binding.binding_id.clone())),
                ("mechanism", Json::str(SURFACE_MECHANISM)),
                ("transport", Json::str("mcp")),
            ]),
        );
        let (run_id, lease) = svc.surface_open_run(manifest)?;
        let mut session = SurfaceSession {
            run_id: run_id.clone(),
            lease,
            binding: binding.clone(),
            turn_no: 0,
            handles: HandleTable::new(),
            pool_node: binding.budget_node.clone(),
        };
        session.allocate_pool_root(svc)?;
        Ok(session)
    }

    /// The pool root: `control.budget.allocated{budget_id:
    /// binding.budget_node, parent: null, mode: pool, spec:
    /// binding.pool}` — minted verbatim (the binding names the node's
    /// id; the row is the durable record the `Account` fold projects).
    /// `DuplicateRoot` on a second call — idempotent by construction.
    fn allocate_pool_root(&mut self, svc: &mut EmbedService) -> Result<(), EmbedError> {
        // The pool spec parses once — a malformed `binding.pool` fails
        // the session open, never mid-turn.
        let spec = parse_pool_spec(&self.binding.pool).map_err(|e| EmbedError::Refused {
            reason: format!("binding.pool malformed: {e}"),
        })?;
        let ev = mint_event(
            svc,
            &self.run_id,
            "control.budget.allocated",
            hh_budget::events::allocated_payload(
                &BudgetNode {
                    budget_id: self.binding.budget_node.clone(),
                    scope: BudgetScope {
                        kind: BudgetScopeKind::AgentProcess,
                        target: self.run_id.clone(),
                    },
                    parent: None,
                    mode: BudgetMode::Pool,
                    spec,
                    created_by: EventRef {
                        run_id: self.run_id.clone(),
                        event_id: svc.surface_head_event_id(&self.run_id)?,
                    },
                    amendments: vec![],
                    completed: false,
                },
                "allocated",
                None,
            ),
            Scope::default(),
            vec![],
        )?;
        svc.surface_append(&self.run_id, &self.lease, vec![ev])?;
        Ok(())
    }

    /// Rebuild a session over an existing surface run — the post-crash
    /// path: the binding owns the run (manifest `caller_binding`
    /// matches), the lease re-acquires through `takeover` when the
    /// persisted record outlived the holder.
    pub fn resume(
        svc: &mut EmbedService,
        run_id: &str,
        binding: CallerBinding,
        lease: Lease,
    ) -> SurfaceSession {
        let mut turn_no = 0u64;
        if let Ok(events) = svc.surface_events(run_id) {
            for e in events {
                if e.class == "lifecycle.turn.started" {
                    turn_no += 1;
                }
            }
        }
        SurfaceSession {
            run_id: run_id.to_string(),
            lease,
            pool_node: binding.budget_node.clone(),
            binding,
            turn_no,
            handles: HandleTable::rebuild(svc.store(), run_id),
        }
    }

    /// `lifecycle.turn.started{turn_id, tool, call_id, binding_id}` —
    /// the turn scope opens; every later row of the call carries
    /// `scope.turn_id`.
    pub fn begin_turn(
        &mut self,
        svc: &mut EmbedService,
        tool: &str,
        call_id: &Json,
    ) -> Result<(String, EventRef), EmbedError> {
        self.turn_no += 1;
        let turn_id = svc.surface_alloc_id("turn");
        let ev = mint_event(
            svc,
            &self.run_id,
            "lifecycle.turn.started",
            Json::obj([
                ("turn_id", Json::str(turn_id.clone())),
                ("turn_no", Json::Int(self.turn_no as i64)),
                ("tool", Json::str(tool)),
                ("call_id", call_id.clone()),
                ("binding_id", Json::str(self.binding.binding_id.clone())),
            ]),
            Scope {
                turn_id: Some(turn_id.clone()),
                ..Scope::default()
            },
            vec![],
        )?;
        let range = svc.surface_append(&self.run_id, &self.lease, vec![ev])?;
        let event_id = svc
            .surface_events(&self.run_id)?
            .get(range.first as usize)
            .map(|e| e.event_id.clone())
            .unwrap_or_default();
        Ok((
            turn_id,
            EventRef {
                run_id: self.run_id.clone(),
                event_id,
            },
        ))
    }

    /// `lifecycle.turn.finished{turn_id, outcome}` — the scope closes.
    /// `outcome ∈ {applied, refused, partial}` — the tool-effect's
    /// terminal.
    pub fn finish_turn(
        &mut self,
        svc: &mut EmbedService,
        turn_id: &str,
        outcome: &str,
        detail: Json,
    ) -> Result<(), EmbedError> {
        let mut m = BTreeMap::new();
        m.insert("turn_id".into(), Json::str(turn_id));
        m.insert("outcome".into(), Json::str(outcome));
        if let Json::Obj(d) = detail {
            for (k, v) in d {
                m.insert(k, v);
            }
        }
        let ev = mint_event(
            svc,
            &self.run_id,
            "lifecycle.turn.finished",
            Json::Obj(m),
            Scope {
                turn_id: Some(turn_id.to_string()),
                ..Scope::default()
            },
            vec![],
        )?;
        svc.surface_append(&self.run_id, &self.lease, vec![ev])?;
        Ok(())
    }

    /// The per-call charge: `control.budget.consumed{tool_calls: +1}`
    /// against `binding.budget_node`, `charged_to = instrument` — the
    /// server's work on the surface run (AC-R-2.11.3-5). `source` is
    /// the call's terminal effect event (the charge's idempotency key
    /// — `(source, dimension)` charges exactly once, so a rebuilt
    /// session never double-posts).
    pub fn charge_turn(
        &mut self,
        svc: &mut EmbedService,
        source: &EventRef,
        dimension: DimensionId,
        amount: i64,
    ) -> Result<(), EmbedError> {
        let mut account = svc
            .surface_account(&self.run_id)
            .map_err(|e| EmbedError::Refused {
                reason: format!("account: {e:?}"),
            })?;
        account
            .charge(
                &self.lease,
                &ChargeRequest {
                    budget_id: self.pool_node.clone(),
                    quantity: ResourceQuantity {
                        dimension,
                        amount,
                        unit: dimension.unit().to_string(),
                        model_ref: None,
                        measured_at: source.clone(),
                    },
                    source: source.clone(),
                    attribution: Attribution {
                        run_id: self.run_id.clone(),
                        participant_ref: self.binding.binding_id.clone(),
                        charged_to: ChargedTo::Instrument,
                        budget_id: self.pool_node.clone(),
                        component_class: Some("mcp".to_string()),
                        component_variant_ref: Some(SURFACE_COMPONENT.to_string()),
                        binding_locality: Some("in_process".to_string()),
                        model_ref: None,
                        ir_refs: vec![],
                        cache: None,
                        over_reservation: false,
                    },
                    reservation_id: None,
                    cache_ttl: None,
                },
            )
            .map_err(|e| EmbedError::Refused {
                reason: format!("surface charge: {e}"),
            })?;
        Ok(())
    }

    /// The live account projection — `run_status.account` is the
    /// `Account`'s own totals, never a recomputation (AC-5's
    /// "returned account equals projection").
    pub fn account_json(&mut self, svc: &mut EmbedService) -> Result<Json, EmbedError> {
        let account = svc
            .surface_account(&self.run_id)
            .map_err(|e| EmbedError::Refused {
                reason: format!("account: {e:?}"),
            })?;
        let totals = account.totals(
            &hh_budget::account::TotalsScope::Run,
            &hh_budget::account::TotalsGroupBy {
                charged_to: true,
                ..Default::default()
            },
        );
        let mut dims = BTreeMap::new();
        for (d, amt) in &totals.by_dimension.amounts {
            dims.insert(d.as_str().to_string(), Json::Int(*amt));
        }
        Ok(Json::obj([
            ("budget_id", Json::str(self.pool_node.clone())),
            ("consumed", Json::Obj(dims)),
            (
                "by_charged_to",
                Json::Obj(
                    totals
                        .by_charged_to
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(v.amounts.values().sum::<i64>())))
                        .collect(),
                ),
            ),
        ]))
    }

    /// The launch budget gate (AC-R-2.11.3-9): the child's requested
    /// `{dimension → hard}` heads are checked against the pool root's
    /// *remaining* projection — `MissingBudget` when the pool declares
    /// no head for the dimension, `InsufficientBudget` when it does and
    /// `requested > remaining`.
    pub fn check_pool(
        &mut self,
        svc: &mut EmbedService,
        requested: &BTreeMap<DimensionId, i64>,
    ) -> Result<(), crate::dispatch::SurfaceError> {
        let account = svc.surface_account(&self.run_id).map_err(|e| {
            crate::dispatch::SurfaceError::new(
                "kernel_error",
                format!("account: {e:?}"),
                Json::Null,
            )
        })?;
        for (dim, req) in requested {
            let key = hh_ontology::dimensions::DimensionKey::Primary(*dim);
            match account.remaining(&self.pool_node, key) {
                Some(rem) => {
                    if *req > rem {
                        return Err(crate::dispatch::SurfaceError::new(
                            "InsufficientBudget",
                            format!(
                                "requested {req} of {dim} exceeds pool remaining {rem}",
                                dim = dim.as_str()
                            ),
                            Json::obj([
                                ("dimension", Json::str(dim.as_str())),
                                ("requested", Json::Int(*req)),
                                ("available", Json::Int(rem)),
                                ("budget_id", Json::str(self.pool_node.clone())),
                            ]),
                        ));
                    }
                }
                None => {
                    return Err(crate::dispatch::SurfaceError::new(
                        "MissingBudget",
                        format!(
                            "budget pool `{}` declares no `{dim}` head",
                            self.pool_node,
                            dim = dim.as_str()
                        ),
                        Json::obj([
                            ("dimension", Json::str(dim.as_str())),
                            ("budget_id", Json::str(self.pool_node.clone())),
                        ]),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The `{budget_id, consumed{…}, remaining{…}}` view — the answer
    /// payload's `budget` member.
    pub fn budget_view(&mut self, svc: &mut EmbedService) -> Json {
        // The declared dimension names first (the events borrow ends
        // before `Account` takes `svc`).
        let mut dims = Vec::new();
        if let Ok(events) = svc.surface_events(&self.run_id) {
            for e in events {
                if e.class != "control.budget.allocated" {
                    continue;
                }
                if e.payload.get("budget_id").and_then(Json::as_str)
                    != Some(self.pool_node.as_str())
                {
                    continue;
                }
                if let Some(Json::Obj(dd)) = e.payload.get("spec").and_then(|s| s.get("dimensions"))
                {
                    dims.extend(dd.keys().cloned());
                }
            }
        }
        let account = match svc.surface_account(&self.run_id) {
            Ok(a) => a,
            Err(_) => return Json::obj([("budget_id", Json::str(self.pool_node.clone()))]),
        };
        let mut remaining = BTreeMap::new();
        for d in dims {
            if let Some(key) = hh_ontology::dimensions::DimensionKey::parse(&d) {
                if let Some(rem) = account.remaining(&self.pool_node, key) {
                    remaining.insert(d.clone(), Json::Int(rem));
                }
            }
        }
        Json::obj([
            ("budget_id", Json::str(self.pool_node.clone())),
            ("remaining", Json::Obj(remaining)),
        ])
    }

    /// The surface run's durable envelopes.
    pub fn events<'a>(
        &self,
        svc: &'a EmbedService,
    ) -> Result<&'a [hh_ledger::event::EventEnvelope], EmbedError> {
        svc.surface_events(&self.run_id)
    }
}

/// Parse `binding.pool` → `BudgetSpec` — accepts the `BudgetSpec` JSON
/// (`{mode, dimensions:{<key>:{hard:{limit,unit}|…}}}`) or the
/// `BudgetInput::Node` lowering (`{dimensions:{<key>:{hard:<int>}}}` or
/// `{<key>:<int>}`). `mode` defaults `pool` (the root's only mode).
pub fn parse_pool_spec(pool: &Json) -> Result<BudgetSpec, String> {
    if let Some(s) = BudgetSpec::from_json(pool) {
        return Ok(s);
    }
    let dims = pool
        .get("dimensions")
        .and_then(|d| match d {
            Json::Obj(m) => Some(m.clone()),
            _ => None,
        })
        .ok_or_else(|| "pool.dimensions missing".to_string())?;
    let mut spec = BudgetSpec {
        mode: BudgetMode::Pool,
        ..Default::default()
    };
    for (k, rule) in &dims {
        let key = hh_ontology::dimensions::DimensionKey::parse(k)
            .ok_or_else(|| format!("pool dimension `{k}` unknown"))?;
        let limit = match rule {
            // `{hard: <int>}` — the BudgetInput lowering.
            Json::Obj(rm) => rm
                .get("hard")
                .and_then(|h| match h {
                    Json::Int(n) => Some(*n),
                    Json::Obj(hm) => hm.get("limit").and_then(Json::as_int),
                    _ => None,
                })
                .ok_or_else(|| format!("pool.{k}.hard missing"))?,
            Json::Int(n) => *n,
            _ => return Err(format!("pool.{k} not a rule")),
        };
        spec.dimensions
            .insert(key, hh_budget::spec::DimensionRule::hard(limit, key));
    }
    spec.validate().map_err(|e| format!("{e}"))?;
    Ok(spec)
}

/// Mint a kernel-produced `Event` for the surface run — the
/// `EventMinter` equivalent with the surface's producer tag and a
/// caller-set `scope`/`causes`. The append gate (fencing, scopes,
/// effect fold) runs at `surface_append`.
pub fn mint_event(
    svc: &mut EmbedService,
    run_id: &str,
    class: &str,
    payload: Json,
    scope: Scope,
    causes: Vec<EventRef>,
) -> Result<Event, EmbedError> {
    Ok(Event {
        event_id: svc.surface_alloc_id("evt"),
        class: class.to_string(),
        ts: svc.surface_ts_now(),
        hlc: None,
        producer: Producer::kernel(SURFACE_COMPONENT),
        scope,
        parent_event_id: svc.surface_head_event_id(run_id)?,
        causes,
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            SURFACE_COMPONENT,
            svc.surface_now_ms().max(0) as u64,
        )),
        content_kind: None,
        payload,
    })
}
