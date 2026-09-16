//! `Account` — the ledger-bound half of §8.2 (the `account(run)` op's
//! `AccountingHandle{budget_id, charge(), reserve(), remaining()}` and its peers).
//!
//! Every mutation is a `Store::append` of `control.budget.*` / `control.decision` /
//! `measurement.cost.attributed` events under the run's writer lease — the single
//! accounting authority (ADR-0040 D1). The in-memory [`BudgetTree`] is the same
//! projection a rebuild computes, folded incrementally so a caller sees its own
//! writes without re-reading the WAL.
//!
//! # E1–E5 wiring (ADR-0040 D5; the envelope's rule — §05e consumes this)
//!
//! - **E1** — [`Account::exhaust`] refuses `EffectsInFlight` while an effect scope
//!   is open: a committed effect reaches a terminal or `unknown` *first*, and the
//!   envelope calls `exhaust` only at decision points (G-PRE-CALL / G-POST-EFFECT).
//! - **E2** — `exhaust` appends `control.budget.exceeded` *and*
//!   `control.decision{kind: stop, reason: budget_exhausted{dimension}}` in one
//!   atomic batch; a second `exhaust` is `AlreadyStopped`.
//! - **E3** — [`Account::claim_grace`] claims the declared per-dimension grace
//!   allowance (`control.decision{kind: grace_call}`); it is never a ceiling
//!   widening — `check` is unchanged.
//! - **E4** — [`Account::advise`] fires soft thresholds: once per
//!   (budget, threshold, context window, compaction epoch), recorded on
//!   `context.observation.recorded{kind: "budget_reminder"}` (the
//!   `context.artefact.*`/`verification.artefact.followed` chain is the context
//!   builder's, §05c); never affects `check`.
//! - **E5** — [`Account::gauge_reserve`] refuses the next increment with
//!   `GaugeCapExceeded` (`SpawnRefused`/`CompactionRequired`), never a stop.

use hh_ledger::event::{Event, Producer, Scope, SeqRange};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ontology::dimensions::{DimensionClass, DimensionId, DimensionKey};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::attribution::Attribution;
use crate::errors::BudgetError;
use crate::events::{self, ThresholdFire};
use crate::pricing::{CostTotals, PricingTable, SpendSource};
use crate::quantity::{ResourceQuantity, ResourceVector};
use crate::spec::{BudgetMode, BudgetNode, BudgetScope, BudgetSpec};
use crate::tree::{BudgetTree, Exceeded};
use crate::usage::TokenDecomposition;

/// The kernel component spelling on this crate's rows (`producer.component_variant_ref`
/// and the provenance `component_ref`).
pub const COMPONENT: &str = "hh-budget/1";

/// `charge`'s parameter pack (§8.2 `charge(run, lease, budget_id, quantity, source,
/// attribution)` — the optional `reservation`/`cache_ttl` are the declared payload
/// members `reservation_id?` / `cache_ttl`).
#[derive(Debug, Clone)]
pub struct ChargeRequest {
    /// The node charged.
    pub budget_id: String,
    /// The quantity charged (primary dimension only).
    pub quantity: ResourceQuantity,
    /// The producing event — the idempotency key with `dimension`.
    pub source: EventRef,
    /// The attribution (R-ACC-3/4 fields; `cache.hit`, `over_reservation` inside).
    pub attribution: Attribution,
    /// The reservation consumed, if the call reserved first.
    pub reservation_id: Option<String>,
    /// The `cache_write` `[ttl_class]` qualifier on the charge.
    pub cache_ttl: Option<String>,
}

/// The `resolve_ask` outcome (approvals-budget exhaustion — ADR-0040 D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskOutcome {
    /// The ask proceeds to the human (budget remains).
    Allow,
    /// The approvals budget is exhausted: the ask converts to `deny` — never
    /// `allow` — and a `security.permission.decided{deny}` row is ledgered.
    Deny,
}

/// `amend`'s authority (§8.2 `amend` invariant column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmendAuthority {
    /// An operator/human principal — may amend the **root** (live retune, C1).
    Operator,
    /// The parent process amending a **child** — bounded by the parent's remaining.
    Parent,
    /// A host override — may only **tighten** (ADR-0040 D6; ADR-0177 D8).
    HostOverride,
}

impl AmendAuthority {
    pub fn as_str(self) -> &'static str {
        match self {
            AmendAuthority::Operator => "operator",
            AmendAuthority::Parent => "parent",
            AmendAuthority::HostOverride => "host_override",
        }
    }
}

/// The `totals` scope (§8.2 `totals(run | budget_id | experiment_id, group_by)`).
/// `experiment_id` is a Stage-6 scope — the `Experiment` arm records it so the
/// signature is stable; folding experiment scope lands with §06's run sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TotalsScope {
    /// The whole run.
    Run,
    /// One budget subtree (`budget_id`).
    Budget(String),
    /// An experiment's arm runs (Stage-6; the name is the `experiment_id`).
    Experiment(String),
}

/// Which `cost_totals` groupings to compute (§8.2 `totals … group_by`). The view
/// always groups by dimension; the flags add the named strata.
#[derive(Debug, Clone, Default)]
pub struct TotalsGroupBy {
    /// `model_ref` stratum.
    pub model: bool,
    /// `charged_to` stratum.
    pub charged_to: bool,
    /// component variant stratum.
    pub component_variant: bool,
    /// participant stratum.
    pub participant: bool,
}

/// The run's accounting handle — lease-bound ops over one run's `BudgetTree`.
pub struct Account<'a> {
    /// The store (single accounting authority — the writer lease).
    pub store: &'a mut Store,
    /// The run.
    pub run_id: String,
    /// The budget projection — rebuilt by [`BudgetTree::project`], folded
    /// incrementally by the ops below (writes apply through appended events).
    pub tree: BudgetTree,
}

impl<'a> Account<'a> {
    /// Open the account over a run: `tree = project(store.events(run))`.
    pub fn open(store: &'a mut Store, run_id: &str) -> Result<Account<'a>, BudgetError> {
        let events = store.events(run_id).map_err(BudgetError::Ledger)?.to_vec();
        let tree = BudgetTree::project(run_id, &events)?;
        Ok(Account {
            store,
            run_id: run_id.to_string(),
            tree,
        })
    }

    fn mint(
        &self,
        class: &str,
        payload: Json,
        scope: Scope,
        causes: Vec<EventRef>,
    ) -> Result<Event, BudgetError> {
        Ok(Event {
            event_id: self.store.alloc_id("evt"),
            class: class.to_string(),
            ts: self.store.ts_now(),
            hlc: None,
            producer: Producer::kernel(COMPONENT),
            scope,
            parent_event_id: self.store.head_event_id(&self.run_id)?,
            causes,
            refs: vec![],
            ir_refs: vec![],
            surface_ids: BTreeMap::new(),
            provenance: Some(ProvenanceRecord::kernel(COMPONENT, self.store.now_ms())),
            content_kind: None,
            payload,
        })
    }

    fn append(&mut self, lease: &Lease, events: Vec<Event>) -> Result<SeqRange, BudgetError> {
        let r = self.store.append(&self.run_id, lease, events)?;
        // Fold our own writes so views see them without a WAL re-read — the same
        // fold a rebuild performs.
        let tail = self
            .store
            .events(&self.run_id)
            .map_err(BudgetError::Ledger)?;
        for e in &tail[(r.first as usize)..=(r.last as usize)] {
            self.tree.fold(e)?;
        }
        Ok(r)
    }

    // ── allocate ─────────────────────────────────────────────────────────

    /// `allocate(parent: budget_id?, scope, spec) → budget_id` (§8.2 §2).
    ///
    /// `parent = None` allocates the one run root — a second root is
    /// `DuplicateRoot`. For a child: `child.hard ≤ parent.remaining`
    /// dimension-wise at allocation (`BudgetExceedsParent`); `slice` children then
    /// move their ceiling out of the parent's pool (`slice_held`), `pool`
    /// children share and may only tighten. **Refusals are ledgered** — an
    /// `allocated{outcome: refused, refusal}` row is appended before the error
    /// returns (AC-4).
    pub fn allocate(
        &mut self,
        lease: &Lease,
        parent: Option<&str>,
        scope: BudgetScope,
        spec: BudgetSpec,
    ) -> Result<String, BudgetError> {
        spec.validate()?;
        let budget_id = self.store.alloc_id("budget");
        let event_id = self.store.alloc_id("evt");
        let created_by = EventRef {
            run_id: self.run_id.clone(),
            event_id: event_id.clone(),
        };
        let node = BudgetNode {
            budget_id: budget_id.clone(),
            scope: scope.clone(),
            parent: parent.map(str::to_string),
            mode: if parent.is_none() {
                BudgetMode::Pool
            } else {
                spec.mode
            },
            spec: spec.clone(),
            created_by,
            amendments: vec![],
            completed: false,
        };
        // Containment + root rules, computed against the projection.
        let refusal: Option<BudgetError> = match parent {
            None => self.tree.root.as_ref().map(|r| BudgetError::DuplicateRoot {
                existing: r.clone(),
            }),
            Some(p) => {
                if !self.tree.nodes.contains_key(p) {
                    Some(BudgetError::UnknownBudget {
                        budget_id: p.to_string(),
                    })
                } else {
                    let pn = self.tree.node(p).expect("checked");
                    spec.within_parent(&pn.node.spec, &|k| {
                        self.tree.remaining(p, k).unwrap_or(i64::MAX)
                    })
                    .err()
                }
            }
        };
        let ev = Event {
            event_id,
            ..self.mint(
                events::CLASS_ALLOCATED,
                events::allocated_payload(
                    &node,
                    if refusal.is_none() {
                        "allocated"
                    } else {
                        "refused"
                    },
                    refusal.as_ref(),
                ),
                Scope::default(),
                vec![],
            )?
        };
        self.append(lease, vec![ev])?;
        if let Some(e) = refusal {
            return Err(e);
        }
        // A slice child moves its ceiling out of the parent's pool now.
        if spec.mode == BudgetMode::Slice {
            if let (Some(p), true) = (parent, self.tree.nodes.contains_key(&budget_id)) {
                let mut held = ResourceVector::zero();
                for (k, c) in spec.hard_caps_map() {
                    let key = DimensionKey::parse(&k).expect("validated key");
                    held.add_key_amount(key, c);
                }
                if let Some(pn) = self.tree.nodes.get_mut(p) {
                    pn.slice_held.add_vec(&held);
                }
            }
        }
        Ok(budget_id)
    }

    // ── reserve ──────────────────────────────────────────────────────────

    /// `reserve(budget_id, quantity, holder, ttl) → reservation_id` (§8.2 §2;
    /// ADR-0040 D3).
    ///
    /// Conservation: for the node and every ancestor,
    /// `consumed + reserved + slice_held + quantity ≤ hard` on every bounded key —
    /// a violation is `InsufficientBudget{dimension, requested, available}`, a
    /// typed pre-dispatch refusal (ledgered as `reserved{outcome: refused}`).
    /// Gauges are `DimensionNotBudgetable` — they cap via E5, never reserve.
    pub fn reserve(
        &mut self,
        lease: &Lease,
        budget_id: &str,
        quantity: &ResourceVector,
        holder: &str,
        ttl_ms: u64,
    ) -> Result<String, BudgetError> {
        if !self.tree.nodes.contains_key(budget_id) {
            return Err(BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            });
        }
        for (d, _) in quantity.iter() {
            if d.class() == DimensionClass::Gauge {
                return Err(BudgetError::DimensionNotBudgetable {
                    dimension: d.as_str().to_string(),
                });
            }
        }
        // Conservation along the ancestor chain, applied to reservations.
        let mut refusal: Option<BudgetError> = None;
        for (d, amount) in quantity.iter() {
            let key = DimensionKey::Primary(d);
            for anc in self.tree.path(budget_id)? {
                let n = &self.tree.nodes[&anc];
                if let Some(hard) = n.node.spec.hard(key) {
                    let level = self.tree.level(n, key);
                    if level + amount > hard {
                        refusal = Some(BudgetError::InsufficientBudget {
                            dimension: key,
                            requested: amount,
                            available: (hard - level).max(0),
                        });
                        break;
                    }
                }
            }
            if refusal.is_some() {
                break;
            }
        }
        let reservation_id = self.store.alloc_id("res");
        let ev = self.mint(
            events::CLASS_RESERVED,
            events::reserved_payload(
                budget_id,
                &reservation_id,
                holder,
                quantity,
                ttl_ms,
                if refusal.is_none() { "held" } else { "refused" },
                refusal.as_ref(),
            ),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])?;
        if let Some(e) = refusal {
            return Err(e);
        }
        Ok(reservation_id)
    }

    // ── charge ───────────────────────────────────────────────────────────

    /// `charge(run, lease, budget_id, quantity, source, attribution) → SeqRange`
    /// (§8.2 §2; ADR-0039 D3 as amended).
    ///
    /// - **Idempotent** on `(source_event, dimension)`: a second charge with the
    ///   same pair appends nothing and returns an empty range (`AC-7` — a crash
    ///   between usage and charge replays cleanly).
    /// - **Reservation:** `req.reservation_id` (or the outstanding reservation of
    ///   `holder` = the quantity's `measured_at` scope holder) is consumed up to
    ///   its remaining claim; the excess claim is released
    ///   (`released{reservation_excess}`); consumption beyond the claim is
    ///   charged and flagged `over_reservation = true`, never refused.
    /// - **Propagation:** one `consumed` row per node on the path to root, each
    ///   with its own `running_total`/`limit`.
    /// - **Cache hits:** `amount = 0` rows with `attribution.cache.hit = true`
    ///   post as ordinary charges.
    /// - Errors: `UnknownBudget`, `DimensionNotBudgetable` (gauge/derived),
    ///   `UnknownSourceEvent`, `Fenced` (via `Ledger`).
    pub fn charge(&mut self, lease: &Lease, req: &ChargeRequest) -> Result<SeqRange, BudgetError> {
        let dim = req.quantity.dimension;
        if dim.class() == DimensionClass::Gauge {
            return Err(BudgetError::DimensionNotBudgetable {
                dimension: dim.as_str().to_string(),
            });
        }
        if !self.tree.nodes.contains_key(&req.budget_id) {
            return Err(BudgetError::UnknownBudget {
                budget_id: req.budget_id.clone(),
            });
        }
        // Idempotency — exactly one charge per (source_event, dimension).
        if self.tree.is_charged(&req.source.event_id, dim) {
            let h = self.store.head(&self.run_id).map_err(BudgetError::Ledger)?;
            return Ok(SeqRange {
                first: h.seq,
                last: h.seq,
                count: 0,
            });
        }
        // The source must resolve to a committed event in some run (the append's
        // cause resolution enforces it too — pre-checked for the typed error).
        if !self.source_resolves(&req.source) {
            return Err(BudgetError::UnknownSourceEvent {
                event_id: req.source.event_id.clone(),
            });
        }
        // ── reservation consumption (purely event-derived) ───────────────
        // Batch order matters for the fold: `released{reservation_excess}` first
        // (frees the unused claim), then the `consumed` rows (each names the
        // reservation and moves `used` from `reserved` to `consumed`).
        let mut over_reservation = req.attribution.over_reservation;
        let mut events_out: Vec<Event> = vec![];
        if let Some(rid) = &req.reservation_id {
            let res = self
                .tree
                .reservations
                .get(rid)
                .filter(|r| r.outstanding)
                .ok_or_else(|| BudgetError::UnknownReservation {
                    reservation_id: rid.clone(),
                })?;
            let claim_left = res.claim_left(dim);
            let used = req.quantity.amount.min(claim_left.max(0));
            let over = req.quantity.amount - used;
            if over > 0 {
                over_reservation = true;
            }
            let excess = claim_left - used;
            if excess > 0 {
                let mut released = ResourceVector::zero();
                released.add(dim, excess);
                let ev = self.mint(
                    events::CLASS_RELEASED,
                    events::released_payload(
                        &req.budget_id,
                        Some(rid),
                        &released,
                        "reservation_excess",
                    ),
                    Scope::default(),
                    vec![],
                )?;
                events_out.push(ev);
            }
        }
        // ── the propagated charge rows ───────────────────────────────────
        let mut path = self.tree.path(&req.budget_id)?;
        // Emit leaf-first (charged node first, then ancestors to the root).
        let mut running: BTreeMap<String, i64> = BTreeMap::new();
        for id in &path {
            let n = &self.tree.nodes[id];
            // running_total includes this charge — reservations already claimed
            // don't double-count: running is `consumed + amount` (the view basis).
            let total = n.consumed.get(dim) + req.quantity.amount;
            running.insert(id.clone(), total);
        }
        for id in path.drain(..) {
            let n = &self.tree.nodes[&id];
            let mut a = req.attribution.clone();
            a.budget_id = id.clone();
            a.over_reservation = over_reservation;
            let limit = n.node.spec.hard(DimensionKey::Primary(dim));
            let ev = self.mint(
                events::CLASS_CONSUMED,
                events::consumed_payload(
                    &id,
                    dim,
                    req.quantity.amount,
                    *running.get(&id).unwrap_or(&0),
                    limit,
                    &req.source,
                    req.reservation_id.as_deref(),
                    &a,
                    req.cache_ttl.as_deref(),
                ),
                Scope::default(),
                vec![req.source.clone()],
            )?;
            events_out.push(ev);
        }
        self.append(lease, events_out)
    }

    fn source_resolves(&self, source: &EventRef) -> bool {
        if source.run_id == self.run_id {
            self.store
                .events(&self.run_id)
                .map(|es| es.iter().any(|e| e.event_id == source.event_id))
                .unwrap_or(false)
        } else {
            // Cross-run sources resolve through the store's lineage index — the
            // append's `causes` check is authoritative; treat as resolvable and
            // let `append` produce `UnknownEventRef` if not.
            true
        }
    }

    // ── release / complete ───────────────────────────────────────────────

    /// `release(reservation_id)` — free the reservation's remaining claim
    /// (`released{reason: release}`; ADR-0040 D3 — affine, never dropped).
    pub fn release(
        &mut self,
        lease: &Lease,
        reservation_id: &str,
    ) -> Result<SeqRange, BudgetError> {
        let res = self
            .tree
            .reservations
            .get(reservation_id)
            .filter(|r| r.outstanding)
            .ok_or_else(|| BudgetError::UnknownReservation {
                reservation_id: reservation_id.to_string(),
            })?;
        let left = res.quantity.minus(&res.consumed);
        let budget_id = res.budget_id.clone();
        let ev = self.mint(
            events::CLASS_RELEASED,
            events::released_payload(&budget_id, Some(reservation_id), &left, "release"),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])
    }

    /// `complete(budget_id)` — a `slice` child's unspent remainder moves back to
    /// the parent (`released{reason: completion}`; ADR-0040 D2's affine rule —
    /// never aliased, never dropped). For `pool` children marks the node complete.
    pub fn complete(&mut self, lease: &Lease, budget_id: &str) -> Result<SeqRange, BudgetError> {
        let n = self
            .tree
            .node(budget_id)
            .ok_or_else(|| BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            })?;
        if n.node.mode != BudgetMode::Slice {
            self.tree
                .nodes
                .get_mut(budget_id)
                .expect("checked")
                .node
                .completed = true;
            let h = self.store.head(&self.run_id).map_err(BudgetError::Ledger)?;
            return Ok(SeqRange {
                first: h.seq,
                last: h.seq,
                count: 0,
            });
        }
        // The held remainder = hard − consumed per bounded key.
        let mut rem = ResourceVector::zero();
        for (key, c) in n.node.spec.hard_caps_map() {
            let key = DimensionKey::parse(&key).expect("validated key");
            let used = n.consumed.key_total(key);
            let left = (c - used).max(0);
            if left > 0 {
                rem.add_key_amount(key, left);
            }
        }
        let ev = self.mint(
            events::CLASS_RELEASED,
            events::released_payload(budget_id, None, &rem, "completion"),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])
    }

    // ── check / exhaust (E1–E5) ──────────────────────────────────────────

    /// `check(budget_id) → [Exceeded]` — root-first (ADR-0040 D4).
    pub fn check(&self, budget_id: &str) -> Result<Vec<Exceeded>, BudgetError> {
        self.tree.check(budget_id)
    }

    /// `exhaust(budget_id, dimension)` — the envelope's rule (§8.2 §2; ADR-0040
    /// D5; E1/E2). Appends `control.budget.exceeded` then
    /// `control.decision{kind: stop, reason: budget_exhausted{dimension}}` in one
    /// atomic batch. `EffectsInFlight` while a non-`read_only` effect scope is open
    /// (E1 + DF-S1.6-1 — the committed effect reaches a terminal or `unknown`
    /// first; `read_only` intents never block); `AlreadyStopped`
    /// when a stop decision exists (a stop decision is made once). Callers use
    /// [`Account::check`] first — `exhaust` also records the *outermost*
    /// exceeded node when `budget_id` names a descendant of it.
    pub fn exhaust(
        &mut self,
        lease: &Lease,
        budget_id: &str,
        dimension: DimensionKey,
        triggered_by: Option<&EventRef>,
    ) -> Result<SeqRange, BudgetError> {
        // E1 as refined by DF-S1.6-1 (§5a.2): exhaustion is refused while a
        // non-`read_only` effect is in flight — a `read_only` intent holds no
        // recovery obligation, so it never blocks the drain.
        let in_flight: Vec<String> = self
            .tree
            .open_effects
            .iter()
            .filter(|(_, rc)| !rc.is_read_only())
            .map(|(id, _)| id.clone())
            .collect();
        if !in_flight.is_empty() {
            return Err(BudgetError::EffectsInFlight { open: in_flight });
        }
        if let Some(d) = &self.tree.stop_decision {
            return Err(BudgetError::AlreadyStopped {
                decision_event_id: d.event_id.clone(),
            });
        }
        // Root-first: the first `check` hit is the outermost exceeded node — every
        // exceeded bound key *at that node* is ledgered (the node is the report).
        let exceeded = self.check(budget_id)?;
        let outermost = exceeded
            .first()
            .ok_or_else(|| BudgetError::CorruptPayload {
                detail: format!(
                    "exhaust({budget_id}, {}) with no exceeded bound — call check first",
                    dimension.as_str()
                ),
            })?;
        let node_id = outermost.budget_id.clone();
        let mut evs = vec![];
        for x in exceeded.iter().filter(|x| x.budget_id == node_id) {
            evs.push(self.mint(
                events::CLASS_EXCEEDED,
                events::exceeded_payload(x),
                Scope::default(),
                triggered_by.cloned().into_iter().collect(),
            )?);
        }
        let first = outermost.clone();
        evs.push(self.mint(
            events::CLASS_DECISION,
            events::decision_payload(
                "stop",
                "envelope",
                Some(&events::stop_reason_budget_exhausted(first.dimension)),
                Some(&first.budget_id),
                Some(first.dimension),
                triggered_by,
                None,
            ),
            Scope::default(),
            vec![],
        )?);
        self.append(lease, evs)
    }

    /// E3 — claim the declared `grace` allowance on `dimension` (one terminal
    /// summarising call, per `Grace::max_calls`). `GraceExhausted` past the
    /// allowance; never a ceiling widening — `check` is unchanged.
    pub fn claim_grace(
        &mut self,
        lease: &Lease,
        budget_id: &str,
        dimension: DimensionKey,
    ) -> Result<SeqRange, BudgetError> {
        let n = self
            .tree
            .node(budget_id)
            .ok_or_else(|| BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            })?;
        let rule = n.node.spec.dimensions.get(&dimension);
        let max = rule
            .and_then(|r| r.grace.as_ref().map(|g| g.max_calls))
            .unwrap_or(0);
        let used = *n.grace_used.get(&dimension).unwrap_or(&0);
        if used >= max {
            return Err(BudgetError::GraceExhausted {
                budget_id: budget_id.to_string(),
                dimension,
                max_calls: max,
            });
        }
        let ev = self.mint(
            events::CLASS_DECISION,
            events::decision_payload(
                "grace_call",
                "envelope",
                Some("grace"),
                Some(budget_id),
                Some(dimension),
                None,
                Some(used + 1),
            ),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])
    }

    /// E4 — evaluate soft thresholds on `budget_id` for context window `window`
    /// at compaction epoch `epoch`; append one
    /// `context.observation.recorded{kind:"budget_reminder"}` per newly-crossed
    /// (budget, threshold, window, epoch). Returns the fired thresholds. Never
    /// affects `check` (soft ≠ hard — ADR-0040 D5).
    pub fn advise(
        &mut self,
        lease: &Lease,
        budget_id: &str,
        window: &str,
        compaction_epoch: u64,
    ) -> Result<Vec<ThresholdFire>, BudgetError> {
        let n = self
            .tree
            .node(budget_id)
            .ok_or_else(|| BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            })?;
        let mut fired = vec![];
        for (key, rule) in &n.node.spec.dimensions {
            let Some(h) = &rule.hard else { continue };
            let consumed = n.consumed.key_total(*key);
            for t in &rule.soft {
                if !t.fired(consumed, h.limit) {
                    continue;
                }
                let tk = t.key();
                let already = self.tree.reminders.iter().any(|r| {
                    r.budget_id == budget_id
                        && r.dimension == *key
                        && r.threshold == tk
                        && r.window == window
                        && r.compaction_epoch == compaction_epoch
                });
                if !already {
                    fired.push(ThresholdFire {
                        budget_id: budget_id.to_string(),
                        dimension: *key,
                        threshold: tk,
                        window: window.to_string(),
                        compaction_epoch,
                        consumed,
                        limit: h.limit,
                        action: t.action.clone(),
                    });
                }
            }
        }
        let mut evs = vec![];
        for f in &fired {
            let item = Json::obj([
                ("kind", Json::str("budget_reminder")),
                ("rule", Json::str(&f.action)),
                ("dimension", Json::str(f.dimension.as_str())),
                ("consumed", Json::Int(f.consumed)),
                ("limit", Json::Int(f.limit)),
            ]);
            evs.push(self.mint(
                events::CLASS_OBSERVATION,
                events::reminder_payload(
                    &f.budget_id,
                    f.dimension,
                    &f.threshold,
                    &f.window,
                    f.compaction_epoch,
                    &item,
                ),
                Scope::default(),
                vec![],
            )?);
        }
        if !evs.is_empty() {
            self.append(lease, evs)?;
        }
        Ok(fired)
    }

    /// E5 — refuse the next increment of gauge `dimension` on `budget_id` beyond
    /// its cap (`GaugeCapExceeded` — `SpawnRefused`/`CompactionRequired`
    /// territory; never a stop, never a charge).
    pub fn gauge_reserve(
        &self,
        budget_id: &str,
        dimension: DimensionId,
        proposed_level: i64,
    ) -> Result<(), BudgetError> {
        self.tree.gauge_cap(budget_id, dimension, proposed_level)
    }

    /// Record a gauge level (max-aggregated).
    pub fn observe_gauge(&mut self, budget_id: &str, dimension: DimensionId, level: i64) {
        self.tree.observe_gauge(budget_id, dimension, level);
    }

    /// The approvals-budget rule (ADR-0040 D6; AC-13): `approvals.requested`
    /// exhaustion converts the next `ask` into `deny`, never `allow`. On `Deny`
    /// a `security.permission.decided{decision: deny, decider: policy}` row is
    /// ledgered — the run's authority ceiling is never touched.
    pub fn resolve_ask(
        &mut self,
        lease: &Lease,
        budget_id: &str,
    ) -> Result<AskOutcome, BudgetError> {
        let key = DimensionKey::Primary(DimensionId::ApprovalsRequested);
        let exhausted = self.check(budget_id)?.iter().any(|x| x.dimension == key);
        if !exhausted {
            return Ok(AskOutcome::Allow);
        }
        let n = self.tree.node(budget_id).expect("checked");
        let limit = n.node.spec.hard(key).unwrap_or(0);
        let value = n.consumed.get(DimensionId::ApprovalsRequested);
        let ev = self.mint(
            "security.permission.decided",
            Json::obj([
                ("decision", Json::str("deny")),
                ("decider", Json::str("policy")),
                (
                    "reason",
                    Json::str(events::stop_reason_budget_exhausted(key)),
                ),
                ("budget_id", Json::str(budget_id)),
                ("value", Json::Int(value)),
                ("limit", Json::Int(limit)),
            ]),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])?;
        Ok(AskOutcome::Deny)
    }

    // ── amend (governance) ───────────────────────────────────────────────

    /// `amend(budget_id, delta, authority) → SeqRange` (§8.2 §2; ADR-0040 D6).
    ///
    /// - `Operator` may amend the **root** (live retune, C1).
    /// - `Parent` may amend a **child**, but the new spec must stay within the
    ///   parent's remaining (`AuthorityInsufficient` otherwise).
    /// - `HostOverride` may only **tighten** — every new hard ≤ the old hard.
    pub fn amend(
        &mut self,
        lease: &Lease,
        budget_id: &str,
        new_spec: &BudgetSpec,
        authority: AmendAuthority,
    ) -> Result<SeqRange, BudgetError> {
        new_spec.validate()?;
        let n = self
            .tree
            .node(budget_id)
            .ok_or_else(|| BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            })?;
        let is_root = n.node.parent.is_none();
        match authority {
            AmendAuthority::Operator => {
                if !is_root {
                    return Err(BudgetError::AuthorityInsufficient {
                        detail: "operator amend applies to the run root".into(),
                    });
                }
            }
            AmendAuthority::Parent => {
                if is_root {
                    return Err(BudgetError::AuthorityInsufficient {
                        detail: "a root has no parent to amend it".into(),
                    });
                }
                let p = n.node.parent.clone().expect("child");
                let pn = self.tree.node(&p).expect("parent exists");
                new_spec
                    .within_parent(&pn.node.spec, &|k| {
                        self.tree.remaining(&p, k).unwrap_or(i64::MAX)
                    })
                    .map_err(|_| BudgetError::AuthorityInsufficient {
                        detail: format!(
                            "child amend exceeds parent {p} remaining (budget_delta would be loosening)"
                        ),
                    })?;
            }
            AmendAuthority::HostOverride => {
                // Tighten-only: every new hard ≤ the old hard (per bound key).
                for (k, c) in new_spec.hard_caps_map() {
                    let key = DimensionKey::parse(&k).expect("validated key");
                    let old = n.node.spec.hard(key).unwrap_or(0);
                    if c > old {
                        return Err(BudgetError::AuthorityInsufficient {
                            detail: format!("host override widened {k} {old} → {c} (tighten only)"),
                        });
                    }
                }
            }
        }
        let ev = self.mint(
            events::CLASS_AMENDED,
            events::amended_payload(budget_id, &n.node.spec, new_spec, authority.as_str()),
            Scope::default(),
            vec![],
        )?;
        self.append(lease, vec![ev])
    }

    // ── attribute_spend ──────────────────────────────────────────────────

    /// `attribute_spend(run, lease, source, pricing, cache_ttl?) → SeqRange`
    /// (§8.2 §2): derives the spend row from the canonical decomposition against
    /// the pinned table and appends `measurement.cost.attributed`. Spend is
    /// derived, never raw; `NoPrice` on a missing row; `exact` requires measured
    /// and `coverage = 1`. Idempotent on `(source_event, spend)` — a replayed
    /// derivation is a no-op.
    #[allow(clippy::too_many_arguments)]
    pub fn attribute_spend(
        &mut self,
        lease: &Lease,
        source: EventRef,
        decomp: &TokenDecomposition,
        model_ref: &crate::attribution::ModelRef,
        table: &PricingTable,
        pin: Option<String>,
        source_kind: &SpendSource,
        coverage_ppm: i64,
        attribution: Attribution,
    ) -> Result<SeqRange, BudgetError> {
        if self.tree.is_charged(&source.event_id, DimensionId::Spend) {
            let h = self.store.head(&self.run_id).map_err(BudgetError::Ledger)?;
            return Ok(SeqRange {
                first: h.seq,
                last: h.seq,
                count: 0,
            });
        }
        let row = crate::pricing::attribute_spend(
            source.clone(),
            decomp,
            model_ref,
            table,
            pin,
            source_kind,
            coverage_ppm,
            attribution,
        )?;
        let ev = self.mint(
            events::CLASS_COST_ATTRIBUTED,
            events::cost_attributed_payload(&row),
            Scope::default(),
            vec![source],
        )?;
        let r = self.append(lease, vec![ev])?;
        // `is_charged` dedup keys off `charged` — spend rows credit through the
        // tree fold; register the key so a replay is a no-op.
        self.tree
            .charged
            .insert((row.source_event.event_id.clone(), DimensionId::Spend));
        Ok(r)
    }

    // ── totals / accountability ──────────────────────────────────────────

    /// `totals(run | budget_id | experiment_id, group_by) → cost_totals`
    /// (§8.2 §2): the materialized view **rebuilt** from charge and spend rows —
    /// never a stored counter; reverted effects' charges are excluded by
    /// projection, never subtracted.
    pub fn totals(&self, scope: &TotalsScope, _group_by: &TotalsGroupBy) -> CostTotals {
        let mut t = CostTotals::default();
        // A row is in a `Budget(b)` scope iff its budget is `b` or a descendant of
        // `b` (`path(row.budget) ∋ b`); `Run`/`Experiment` scope takes the whole run.
        let in_scope = |budget_id: &str| -> bool {
            match scope {
                TotalsScope::Run | TotalsScope::Experiment(_) => true,
                TotalsScope::Budget(b) => self
                    .tree
                    .path(budget_id)
                    .map(|p| p.iter().any(|id| id == b))
                    .unwrap_or(false),
            }
        };
        let reverted = |event_id: &str| -> bool {
            self.tree
                .event_effect
                .get(event_id)
                .map(|e| self.tree.reverted_effects.contains(e))
                .unwrap_or(false)
        };
        // Count each (source, dim) charge once — at the leaf-most node (the row
        // whose budget is an ancestor of no other row's budget of this charge);
        // ancestor propagation rows are the same charge seen at the parent view.
        for rows in self.tree.charges_by_source.values() {
            for r in rows {
                if !in_scope(&r.budget_id) || reverted(&r.source_event.event_id) {
                    continue;
                }
                // Leaf-most row = the row whose budget is an ancestor of no other
                // row's budget in this charge batch.
                let is_leaf = rows.iter().all(|o| {
                    o.budget_id == r.budget_id
                        || !self
                            .tree
                            .path(&o.budget_id)
                            .map(|p| p.contains(&r.budget_id))
                            .unwrap_or(false)
                });
                if !is_leaf {
                    continue;
                }
                t.by_dimension.add(r.dimension, r.amount);
                if let Some(m) = &r.attribution.model_ref {
                    t.by_model
                        .entry(m.pricing_key())
                        .or_default()
                        .add(r.dimension, r.amount);
                }
                t.by_charged_to
                    .entry(r.attribution.charged_to.as_str().to_string())
                    .or_default()
                    .add(r.dimension, r.amount);
                t.by_component_variant
                    .entry(
                        r.attribution
                            .component_variant_ref
                            .clone()
                            .unwrap_or_default(),
                    )
                    .or_default()
                    .add(r.dimension, r.amount);
                t.by_participant
                    .entry(r.attribution.participant_ref.clone())
                    .or_default()
                    .add(r.dimension, r.amount);
            }
        }
        // Spend rows: `spend` counter + provenance/confidence/coverage strata.
        for s in &self.tree.spend_rows {
            if !in_scope(&s.attribution.budget_id) || reverted(&s.source_event.event_id) {
                continue;
            }
            t.by_dimension.add(DimensionId::Spend, s.money.micro_units);
            *t.spend_by_currency
                .entry(s.money.currency.clone())
                .or_insert(0) += s.money.micro_units;
            *t.provenance_mix
                .entry(s.provenance.as_str().to_string())
                .or_insert(0) += s.money.micro_units;
            t.min_confidence = Some(match t.min_confidence {
                None => s.confidence,
                Some(c) => {
                    if c.rank() <= s.confidence.rank() {
                        c
                    } else {
                        s.confidence
                    }
                }
            });
            t.by_charged_to
                .entry(s.attribution.charged_to.as_str().to_string())
                .or_default()
                .add(DimensionId::Spend, s.money.micro_units);
            t.by_model
                .entry(s.model_ref.pricing_key())
                .or_default()
                .add(DimensionId::Spend, s.money.micro_units);
            if let Some(c) = &s.attribution.cache {
                if c.hit {
                    *t.avoided_by_cache_kind
                        .entry(c.kind.clone().unwrap_or_else(|| "unknown".into()))
                        .or_insert(0) += s.money.micro_units;
                }
            }
        }
        t
    }

    /// R-ACC-2 — the accountability projection: every accountable event class
    /// must yield ≥ 1 charge. Returns the **orphans** — accountable events with
    /// no `control.budget.consumed{source_event}` row (the projection
    /// validator's failure list; an empty report is the invariant).
    pub fn accountability_report(&self) -> AccountabilityReport {
        const ACCOUNTABLE: &[&str] = &[
            "model.call.completed",
            "action.tool.completed",
            "action.effect.observed",
            "security.permission.decided",
            "verification.validator.invoked",
            "lifecycle.component.invoked",
        ];
        let mut orphans = vec![];
        let events = match self.store.events(&self.run_id) {
            Ok(e) => e,
            Err(_) => return AccountabilityReport { orphans },
        };
        for e in events {
            let accountable = ACCOUNTABLE.contains(&e.class.as_str())
                || e.class.starts_with("action.environment.");
            if accountable && !self.tree.charges_by_source.contains_key(&e.event_id) {
                orphans.push(e.event_id.clone());
            }
        }
        AccountabilityReport { orphans }
    }

    /// `remaining(budget_id, key)` — the `AccountingHandle.remaining()` read.
    pub fn remaining(&self, budget_id: &str, key: DimensionKey) -> Option<i64> {
        self.tree.remaining(budget_id, key)
    }
}

/// The R-ACC-2 report — `orphans` is empty iff every accountable event produced
/// ≥ 1 charge.
#[derive(Debug, Clone, Default)]
pub struct AccountabilityReport {
    /// Accountable events with no charge.
    pub orphans: Vec<String>,
}

impl AccountabilityReport {
    /// R-ACC-2 satisfied.
    pub fn is_clean(&self) -> bool {
        self.orphans.is_empty()
    }
}

impl ResourceVector {
    /// Add `amount` under a bound key — derived keys add nothing (a derived name
    /// is a view; only primary dims hold amounts). For `slice_held` bookkeeping the
    /// derived key's *components* are what the parent's pool loses.
    fn add_key_amount(&mut self, key: DimensionKey, amount: i64) {
        match key {
            DimensionKey::Primary(d) => self.add(d, amount),
            DimensionKey::Derived(d) => {
                for c in d.components() {
                    self.add(*c, amount);
                }
            }
        }
    }
}
