//! `BudgetTree` — the pure projection of the `control.budget.*` /
//! `measurement.cost.attributed` event stream (§8.2 §2–3; ADR-0040 D1–D4).
//!
//! Nothing here touches the ledger: the tree is rebuilt by folding envelopes
//! ([`BudgetTree::project`]), and `Account` mutates it *through* appended events —
//! `consumed`/`reserved`/`remaining` are views, never stored counters (ADR-0039 D5).
//!
//! # Containment semantics (ADR-0040 D2)
//!
//! - Charges **propagate to every ancestor**: `consumed[n]` is subtree-aggregated, so
//!   a parent's `remaining` is always the truth.
//! - Outstanding reservations aggregate the same way (`reserved[n]` covers the
//!   subtree) — conservation `Σ reservations + consumed ≤ hard` then holds
//!   per-node along the ancestor chain.
//! - A `slice` child **moves capacity out of the parent**: while the child is
//!   active the parent holds `slice_held += child.hard − child.consumed` per
//!   bounded key (the affine remainder); on completion the held remainder is
//!   released (`control.budget.released{reason: completion}`) — never aliased,
//!   never dropped.
//! - A `pool` child shares the parent's pool and may only tighten — nothing is
//!   held; conservation rides on reserve-before-spend.
//! - `remaining[n][key] = hard − consumed − reserved − slice_held` for bounded
//!   keys (unbounded keys have no remaining limit).
//!
//! # Exhaustion (E1–E5)
//!
//! `check` is root-first and reports the *outermost* exceeded node (a child cannot
//! spin on its own exhaustion while the root is spent). E1's "decision points
//! only" is enforced by `Account::exhaust` refusing while an effect scope is open.

use hh_ledger::event::EventEnvelope;
use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use std::collections::{BTreeMap, BTreeSet};

use crate::errors::BudgetError;
use crate::events::{
    self, AllocatedRow, ConsumedRow, DecisionRow, ReleasedRow, ReminderRow, ReservedRow,
};
use crate::pricing::SpendRow;
use crate::quantity::ResourceVector;
use crate::spec::{BudgetMode, BudgetNode, Reservation};

/// `Exceeded{budget_id, dimension, value, limit}` — the `check` return member
/// (§8.2 §2). `dimension` is the bound key spelling (primary or derived).
#[derive(Debug, Clone, PartialEq)]
pub struct Exceeded {
    /// The exceeded node.
    pub budget_id: String,
    /// The bound dimension key.
    pub dimension: DimensionKey,
    /// The measured value (`consumed` for counters).
    pub value: i64,
    /// The hard limit.
    pub limit: i64,
}

/// Per-node projection state.
#[derive(Debug, Clone)]
pub struct NodeState {
    /// The node record.
    pub node: BudgetNode,
    /// Subtree-aggregated consumed amounts (propagated charges).
    pub consumed: ResourceVector,
    /// Subtree-aggregated outstanding reservations.
    pub reserved: ResourceVector,
    /// Max-aggregated gauge levels (this node's own readings).
    pub gauge_max: ResourceVector,
    /// `Σ (child.hard − child.consumed)` over this node's *active* `slice`
    /// children — the affine capacity held out of this node's pool.
    pub slice_held: ResourceVector,
    /// Child node ids in creation order.
    pub children: Vec<String>,
    /// Grace calls claimed per bound key (E3).
    pub grace_used: BTreeMap<DimensionKey, u32>,
}

/// The run's budget projection — one root, a tree of nodes.
#[derive(Debug, Default)]
pub struct BudgetTree {
    /// The run this tree belongs to.
    pub run_id: String,
    /// Nodes by id.
    pub nodes: BTreeMap<String, NodeState>,
    /// The one root (one per run).
    pub root: Option<String>,
    /// Reservations by id (outstanding and released — `outstanding` flags it).
    pub reservations: BTreeMap<String, Reservation>,
    /// The idempotency key set: `(source_event.event_id, dimension)` already
    /// charged — re-projection yields exactly one charge per pair (AC-7).
    pub charged: BTreeSet<(String, DimensionId)>,
    /// Per-node fold dedup: `(source_event, dimension, budget_id)` — the
    /// propagated rows of one charge batch share `(source, dimension)` and are
    /// folded once each.
    pub charged_nodes: BTreeSet<(String, DimensionId, String)>,
    /// Spend rows (`measurement.cost.attributed`), in event order.
    pub spend_rows: Vec<SpendRow>,
    /// `source_event → charge rows it produced` (per-node propagation list) — the
    /// R-ACC-2 "every accountable event yields ≥ 1 charge" join table.
    pub charges_by_source: BTreeMap<String, Vec<ConsumedRow>>,
    /// Recorded `control.budget.exceeded` rows.
    pub exceeded: Vec<events::ExceededRow>,
    /// `control.decision` rows (stop / grace_call / ask conversions).
    pub decisions: Vec<DecisionRow>,
    /// Delivered budget reminders (E4 dedup: once per
    /// (budget, threshold, window, compaction epoch)).
    pub reminders: Vec<ReminderRow>,
    /// Effect ids that reached `reverted`/`abandoned` — their charges are excluded
    /// from `cost_totals` by projection, never subtracted (ADR-0039 D5).
    pub reverted_effects: BTreeSet<String>,
    /// `event_id → effect_id` for scope-carrying producers (the exclusion join).
    pub event_effect: BTreeMap<String, String>,
    /// Open effect scopes → their effective risk class (E1 — `exhaust` refuses
    /// while any **non-`read_only`** effect is open; `read_only` intents never
    /// block exhaustion — DF-S1.6-1, §5a.2's open-risk set). A missing or
    /// unreadable `effective_risk_class` projects to `RiskClass::UNKNOWN` (the
    /// most dangerous class — legacy rows stay conservative).
    pub open_effects: BTreeMap<String, hh_ontology::risk::RiskClass>,
    /// `effect_id → last probe verdict` — invariant 4 needs it: a `not_applied`
    /// settlement is effect-terminal only when the class forecloses retry
    /// (`irreversible`, or non-`idempotent` without a `not_applied` probe —
    /// ADR-0238 §1).
    pub probe_verdicts: BTreeMap<String, String>,
    /// The recorded stop decision, if any (a stop decision is made once).
    pub stop_decision: Option<DecisionRow>,
}

/// Invariant 4 (ADR-0030 §1; ADR-0238 §1): a `not_applied` settlement stays
/// redispatchable iff the class isn't `irreversible` and either
/// `repeat_safety = idempotent` or the last probe returned `not_applied`.
fn not_applied_retryable(rc: &hh_ontology::risk::RiskClass, last_probe: Option<&String>) -> bool {
    use hh_ontology::risk::{RepeatSafety, RiskReversibility};
    rc.reversibility != RiskReversibility::Irreversible
        && (rc.repeat_safety == RepeatSafety::Idempotent
            || last_probe.map(String::as_str) == Some("not_applied"))
}

impl BudgetTree {
    /// An empty tree for `run_id`.
    pub fn new(run_id: impl Into<String>) -> BudgetTree {
        BudgetTree {
            run_id: run_id.into(),
            ..Default::default()
        }
    }

    /// Rebuild the projection by folding `envelopes` (durable event order).
    ///
    /// Unknown classes are ignored — the tree only reads the budget/accounting
    /// family, the effect terminals (revert exclusion, open-effect drain), the
    /// `control.decision` rows and `context.observation.recorded` reminders.
    pub fn project(
        run_id: impl Into<String>,
        envelopes: &[EventEnvelope],
    ) -> Result<BudgetTree, BudgetError> {
        let mut t = BudgetTree::new(run_id);
        for e in envelopes {
            t.fold(e)?;
        }
        Ok(t)
    }

    /// Fold one envelope into the projection.
    pub fn fold(&mut self, e: &EventEnvelope) -> Result<(), BudgetError> {
        // Track scope bookkeeping needed for E1 (open effects) and revert
        // exclusion. The subject is `scope.effect_id` or — for the
        // `compensated`/`reverted` marker form — `payload.original_effect_id`.
        let eff = e.scope.effect_id.as_deref().or_else(|| {
            e.payload
                .get("original_effect_id")
                .and_then(hh_wire::json::Json::as_str)
        });
        if let Some(eff) = eff {
            self.event_effect
                .insert(e.event_id.clone(), eff.to_string());
            match e.class.as_str() {
                "action.effect.intended" => {
                    let rc = e
                        .payload
                        .get("effective_risk_class")
                        .and_then(hh_ontology::risk::RiskClass::from_json)
                        .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN);
                    self.open_effects.insert(eff.to_string(), rc);
                }
                "action.effect.observed" => {
                    // `observed{partial}` is non-terminal — still open;
                    // `observed{not_applied}` closes the effect only when the
                    // class forecloses retry (invariant 4 — ADR-0238 §1).
                    match e
                        .payload
                        .get("outcome")
                        .and_then(hh_wire::json::Json::as_str)
                    {
                        Some("partial") => {}
                        Some("not_applied") => {
                            let rc = self
                                .open_effects
                                .get(eff)
                                .cloned()
                                .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN);
                            if !not_applied_retryable(&rc, self.probe_verdicts.get(eff)) {
                                self.open_effects.remove(eff);
                            }
                        }
                        _ => {
                            self.open_effects.remove(eff);
                        }
                    }
                }
                "action.effect.probed" => {
                    // A conclusive probe verdict is the observed transition;
                    // `undeterminable` returns to `unknown` — still open, and a
                    // `not_applied` verdict keeps a retryable class open
                    // (invariant 4 — ADR-0238 §1).
                    let verdict = e
                        .payload
                        .get("verdict")
                        .and_then(hh_wire::json::Json::as_str);
                    if let Some(v) = verdict {
                        self.probe_verdicts.insert(eff.to_string(), v.to_string());
                    }
                    match verdict {
                        Some("applied") => {
                            self.open_effects.remove(eff);
                        }
                        Some("not_applied") => {
                            let rc = self
                                .open_effects
                                .get(eff)
                                .cloned()
                                .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN);
                            if !not_applied_retryable(&rc, Some(&"not_applied".to_string())) {
                                self.open_effects.remove(eff);
                            }
                        }
                        _ => {}
                    }
                }
                "action.effect.refused"
                | "action.effect.abandoned"
                | "action.effect.compensated" => {
                    self.open_effects.remove(eff);
                }
                "action.effect.reverted" => {
                    self.open_effects.remove(eff);
                    self.reverted_effects.insert(eff.to_string());
                }
                _ => {}
            }
        }
        match e.class.as_str() {
            events::CLASS_ALLOCATED => {
                let row = events::allocated_from_json(&e.payload)?;
                if row.outcome == "allocated" {
                    self.fold_allocated(
                        row,
                        EventRef {
                            run_id: e.run_id.clone(),
                            event_id: e.event_id.clone(),
                        },
                    )?;
                }
            }
            events::CLASS_CONSUMED => {
                let row = events::consumed_from_json(&e.payload)?;
                self.fold_consumed(row)?;
            }
            events::CLASS_RESERVED => {
                let row = events::reserved_from_json(&e.payload)?;
                if row.outcome == "held" {
                    self.fold_reserved(row);
                }
            }
            events::CLASS_RELEASED => {
                let row = events::released_from_json(&e.payload)?;
                self.fold_released(row)?;
            }
            events::CLASS_EXCEEDED => {
                self.exceeded.push(events::exceeded_from_json(&e.payload)?);
            }
            events::CLASS_AMENDED => {
                let row = events::amended_from_json(&e.payload)?;
                if let Some(n) = self.nodes.get_mut(&row.budget_id) {
                    n.node.amendments.push(EventRef {
                        run_id: e.run_id.clone(),
                        event_id: e.event_id.clone(),
                    });
                    n.node.spec = row.new;
                }
            }
            events::CLASS_DECISION => {
                if let Some(d) = events::decision_from_json(&e.payload, &e.event_id) {
                    match d.kind.as_str() {
                        "stop" => self.stop_decision = Some(d.clone()),
                        "grace_call" => {
                            if let (Some(b), Some(dim)) = (&d.budget_id, d.dimension) {
                                if let Some(n) = self.nodes.get_mut(b) {
                                    *n.grace_used.entry(dim).or_insert(0) += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                    self.decisions.push(d);
                }
            }
            events::CLASS_COST_ATTRIBUTED => {
                if let Some(row) = SpendRow::from_json(&e.payload) {
                    // The spend counter: `spend` is charged through the spend row
                    // (attribution.budget_id), propagated to ancestors like a charge —
                    // this is what makes `spend` ceilings enforceable.
                    let mut v = ResourceVector::zero();
                    v.add(DimensionId::Spend, row.money.micro_units);
                    self.propagate_consumed(&row.attribution.budget_id, &v);
                    self.charged
                        .insert((row.source_event.event_id.clone(), DimensionId::Spend));
                    self.spend_rows.push(row);
                }
            }
            events::CLASS_OBSERVATION => {
                if let Some(r) = events::reminder_from_json(&e.payload) {
                    self.reminders.push(r);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Fold an `allocated` row (outcome = allocated).
    fn fold_allocated(
        &mut self,
        row: AllocatedRow,
        created_by: EventRef,
    ) -> Result<(), BudgetError> {
        if self.nodes.contains_key(&row.budget_id) {
            return Err(BudgetError::CorruptPayload {
                detail: format!("budget {} allocated twice", row.budget_id),
            });
        }
        if row.parent.is_none() {
            if let Some(r) = &self.root {
                return Err(BudgetError::DuplicateRoot {
                    existing: r.clone(),
                });
            }
            self.root = Some(row.budget_id.clone());
        } else if let Some(p) = &row.parent {
            if !self.nodes.contains_key(p) {
                return Err(BudgetError::UnknownBudget {
                    budget_id: p.clone(),
                });
            }
            self.nodes
                .get_mut(p)
                .expect("checked")
                .children
                .push(row.budget_id.clone());
        }
        self.nodes.insert(
            row.budget_id.clone(),
            NodeState {
                node: BudgetNode {
                    budget_id: row.budget_id,
                    scope: row.scope,
                    parent: row.parent,
                    mode: row.mode,
                    spec: row.spec,
                    created_by,
                    amendments: vec![],
                    completed: false,
                },
                consumed: ResourceVector::zero(),
                reserved: ResourceVector::zero(),
                gauge_max: ResourceVector::zero(),
                slice_held: ResourceVector::zero(),
                children: vec![],
                grace_used: BTreeMap::new(),
            },
        );
        Ok(())
    }

    /// Fold a `consumed` row — the row names its node; subtree aggregation is
    /// achieved because `charge` emits one row per node on the path to root.
    fn fold_consumed(&mut self, row: ConsumedRow) -> Result<(), BudgetError> {
        if !self.nodes.contains_key(&row.budget_id) {
            return Err(BudgetError::UnknownBudget {
                budget_id: row.budget_id,
            });
        }
        let dedup_key = (
            row.source_event.event_id.clone(),
            row.dimension,
            row.budget_id.clone(),
        );
        if self.charged_nodes.contains(&dedup_key) {
            return Ok(()); // replay of the same batch — fold is idempotent
        }
        self.charged_nodes.insert(dedup_key);
        // The reservation consumption folds *with* the charge: `used` of the claim
        // moves from `reserved` to `consumed` (excess was already freed by the
        // preceding `released{reservation_excess}` row of the same batch).
        if let Some(rid) = &row.reservation_id {
            let mut propagate: Option<(String, ResourceVector)> = None;
            if let Some(res) = self.reservations.get_mut(rid) {
                if res.budget_id == row.budget_id && res.outstanding {
                    let left = res.claim_left(row.dimension);
                    let used = row.amount.min(left.max(0));
                    res.consumed.add(row.dimension, used);
                    let mut v = ResourceVector::zero();
                    v.add(row.dimension, -used);
                    // `outstanding` re-derives from claim_left over all dims.
                    res.outstanding = res.has_claim_left();
                    propagate = Some((res.budget_id.clone(), v));
                }
            }
            if let Some((bid, v)) = propagate {
                self.propagate_reserved(&bid, &v);
            }
        }
        let n = self.nodes.get_mut(&row.budget_id).expect("checked");
        n.consumed.add(row.dimension, row.amount);
        self.charged
            .insert((row.source_event.event_id.clone(), row.dimension));
        self.charges_by_source
            .entry(row.source_event.event_id.clone())
            .or_default()
            .push(row);
        Ok(())
    }

    /// Fold a `reserved` row (outcome = held).
    fn fold_reserved(&mut self, row: ReservedRow) {
        let res = Reservation {
            reservation_id: row.reservation_id.clone(),
            budget_id: row.budget_id.clone(),
            holder: row.holder,
            quantity: row.quantity.clone(),
            expires_with: "lease".into(),
            ttl_ms: row.ttl_ms,
            consumed: ResourceVector::zero(),
            released: ResourceVector::zero(),
            outstanding: true,
        };
        self.propagate_reserved(&row.budget_id, &row.quantity);
        self.reservations.insert(row.reservation_id, res);
    }

    /// Fold a `released` row — the named amounts leave the reservation's claim
    /// (`quantity − consumed − released` remains claimed) and the ancestor
    /// `reserved` aggregation; `completion` instead returns a slice child's
    /// unspent remainder to its parent's `slice_held`.
    fn fold_released(&mut self, row: ReleasedRow) -> Result<(), BudgetError> {
        match row.reason.as_str() {
            "reservation_excess" | "release" | "lease_lost" => {
                let rid =
                    row.reservation_id
                        .clone()
                        .ok_or_else(|| BudgetError::CorruptPayload {
                            detail: "released{release}: missing reservation_id".into(),
                        })?;
                let budget_id = {
                    let res = self.reservations.get_mut(&rid).ok_or_else(|| {
                        BudgetError::UnknownReservation {
                            reservation_id: rid.clone(),
                        }
                    })?;
                    res.released.add_vec(&row.released);
                    res.outstanding = res.has_claim_left();
                    res.budget_id.clone()
                };
                self.propagate_reserved(&budget_id, &row.released.negated());
            }
            "completion" => {
                // A slice child's unspent remainder returns to the parent: drop the
                // held amount from the parent's `slice_held`.
                if let Some(n) = self.nodes.get(&row.budget_id) {
                    if let Some(p) = n.node.parent.clone() {
                        if let Some(pn) = self.nodes.get_mut(&p) {
                            pn.slice_held.add_vec(&row.released.negated());
                        }
                    }
                }
                if let Some(n) = self.nodes.get_mut(&row.budget_id) {
                    n.node.completed = true;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Add `v` to `consumed` at `budget_id` and every ancestor (charge propagation).
    pub fn propagate_consumed(&mut self, budget_id: &str, v: &ResourceVector) {
        let mut cur = Some(budget_id.to_string());
        while let Some(id) = cur {
            let Some(n) = self.nodes.get_mut(&id) else {
                break;
            };
            n.consumed.add_vec(v);
            // A slice child's consumption shrinks the held remainder at its parent.
            cur = n.node.parent.clone();
            if let (Some(p), Some(cn)) = (&cur, self.nodes.get(&id)) {
                if cn.node.mode == BudgetMode::Slice && !cn.node.completed {
                    if let Some(pn) = self.nodes.get_mut(p) {
                        pn.slice_held.add_vec(&v.negated());
                    }
                }
            }
        }
    }

    /// Add `v` to `reserved` at `budget_id` and every ancestor.
    pub fn propagate_reserved(&mut self, budget_id: &str, v: &ResourceVector) {
        let mut cur = Some(budget_id.to_string());
        while let Some(id) = cur {
            let Some(n) = self.nodes.get_mut(&id) else {
                break;
            };
            n.reserved.add_vec(v);
            cur = n.node.parent.clone();
        }
    }

    /// The ancestor chain `budget_id → … → root` (the node itself first).
    pub fn path(&self, budget_id: &str) -> Result<Vec<String>, BudgetError> {
        if !self.nodes.contains_key(budget_id) {
            return Err(BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            });
        }
        let mut out = vec![];
        let mut cur = Some(budget_id.to_string());
        while let Some(id) = cur {
            let n = self.nodes.get(&id).expect("path member");
            cur = n.node.parent.clone();
            out.push(id);
        }
        Ok(out)
    }

    /// `remaining[budget][key]` for a bounded key:
    /// `hard − consumed − reserved − slice_held` (see the module docs).
    pub fn remaining(&self, budget_id: &str, key: DimensionKey) -> Option<i64> {
        let n = self.nodes.get(budget_id)?;
        let hard = n.node.spec.hard(key)?;
        Some(hard - self.level(n, key))
    }

    /// `consumed + reserved + slice_held` at a bound key.
    pub fn level(&self, n: &NodeState, key: DimensionKey) -> i64 {
        n.consumed.key_total(key) + n.reserved.key_total(key) + n.slice_held.key_total(key)
    }

    /// `check(budget_id) → [Exceeded]` — **root-first**: evaluates the path
    /// root → node, reporting every exceeded bound key ordered outermost-first
    /// (the first element is the outermost exceeded node — the one the envelope
    /// reports; a child cannot spin on its own exhaustion while the root is
    /// spent, ADR-0040 D4).
    ///
    /// A bound counter key is exceeded when `level ≥ hard` (nothing remains —
    /// Inspect's `value >= limit` rule); gauges are cap-checked by
    /// [`BudgetTree::gauge_cap`], never here (E5).
    pub fn check(&self, budget_id: &str) -> Result<Vec<Exceeded>, BudgetError> {
        let mut path = self.path(budget_id)?;
        path.reverse(); // root first
        let mut out = vec![];
        for id in &path {
            let n = &self.nodes[id];
            for (key, rule) in &n.node.spec.dimensions {
                let Some(c) = &rule.hard else { continue };
                if key.primary().map(|d| d.class())
                    == Some(hh_ontology::dimensions::DimensionClass::Gauge)
                {
                    continue; // gauges are E5, not exhaustion
                }
                let v = self.level(n, *key);
                if v >= c.limit {
                    out.push(Exceeded {
                        budget_id: id.clone(),
                        dimension: *key,
                        value: v,
                        limit: c.limit,
                    });
                }
            }
        }
        Ok(out)
    }

    /// E5 — refuse the *next increment* of a gauge beyond its cap. `proposed` is
    /// the level the increment would reach; `GaugeCapExceeded` is the typed
    /// refusal (`SpawnRefused`/`CompactionRequired` territory) — never a stop.
    pub fn gauge_cap(
        &self,
        budget_id: &str,
        dimension: DimensionId,
        proposed: i64,
    ) -> Result<(), BudgetError> {
        let n = self
            .nodes
            .get(budget_id)
            .ok_or_else(|| BudgetError::UnknownBudget {
                budget_id: budget_id.to_string(),
            })?;
        if dimension.class() != hh_ontology::dimensions::DimensionClass::Gauge {
            return Err(BudgetError::DimensionNotBudgetable {
                dimension: dimension.as_str().to_string(),
            });
        }
        if let Some(cap) = n.node.spec.hard(DimensionKey::Primary(dimension)) {
            if proposed > cap {
                return Err(BudgetError::GaugeCapExceeded {
                    budget_id: budget_id.to_string(),
                    dimension,
                    value: proposed,
                    cap,
                });
            }
        }
        Ok(())
    }

    /// Record a gauge observation — max-aggregated (a gauge's stored level is the
    /// max over the run, never a sum).
    pub fn observe_gauge(&mut self, budget_id: &str, dimension: DimensionId, level: i64) {
        if let Some(n) = self.nodes.get_mut(budget_id) {
            let cur = n.gauge_max.get(dimension);
            if level > cur {
                n.gauge_max.add(dimension, level - cur);
            }
        }
    }

    /// The node state.
    pub fn node(&self, budget_id: &str) -> Option<&NodeState> {
        self.nodes.get(budget_id)
    }

    /// Has `(source_event, dimension)` already been charged? The charge
    /// idempotency test (AC-7).
    pub fn is_charged(&self, source_event: &str, dimension: DimensionId) -> bool {
        self.charged
            .contains(&(source_event.to_string(), dimension))
    }

    /// Outstanding reservation held by `holder` on `budget_id` (the holder is the
    /// `effect:<id>`/`model_call:<id>` the reservation was taken for; retries of
    /// `unknown` effects re-use it — ADR-0040 D3).
    pub fn reservation_for(&self, budget_id: &str, holder: &str) -> Option<&Reservation> {
        self.reservations
            .values()
            .find(|r| r.outstanding && r.budget_id == budget_id && r.holder == holder)
    }

    /// The reservation by id.
    pub fn reservation(&self, reservation_id: &str) -> Option<&Reservation> {
        self.reservations.get(reservation_id)
    }
}

impl ResourceVector {
    /// The negated view (release arithmetic — view-only, never a stored counter
    /// mutation).
    pub fn negated(&self) -> ResourceVector {
        let mut v = ResourceVector::zero();
        for (d, a) in self.iter() {
            v.add(d, -a);
        }
        v
    }
}
