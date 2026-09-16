//! The `control.budget.*` and `measurement.cost.attributed` payload shapes plus the
//! budget-owned `StopReason` members (§8.2 §3; ADR-0039 D3/D4; ADR-0040 D5/D6;
//! ADR-0106 D7/D8). One builder + one decoder per class — the single schema source
//! (CC7); `Account` emits through these and `BudgetTree::project` decodes with them.
//!
//! Class registration lives in `hh_ledger::classes` (the one persistence-policy
//! table); this module owns the *payload contract*.

use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::attribution::Attribution;
use crate::errors::BudgetError;
use crate::pricing::SpendRow;
use crate::quantity::ResourceVector;
use crate::spec::{BudgetMode, BudgetScope, BudgetSpec};

// ── class spellings (registered in hh_ledger::classes::CLASS_TABLE) ────────────

/// `control.budget.allocated` — `allocate` (allocated or refused; AC-4).
pub const CLASS_ALLOCATED: &str = "control.budget.allocated";
/// `control.budget.reserved` — `reserve` (held or refused).
pub const CLASS_RESERVED: &str = "control.budget.reserved";
/// `control.budget.consumed` — `charge` (one per `(source_event, dimension)`).
pub const CLASS_CONSUMED: &str = "control.budget.consumed";
/// `control.budget.released` — reservation excess / slice remainder release.
pub const CLASS_RELEASED: &str = "control.budget.released";
/// `control.budget.exceeded` — E2's first row.
pub const CLASS_EXCEEDED: &str = "control.budget.exceeded";
/// `control.budget.amended` — `amend`.
pub const CLASS_AMENDED: &str = "control.budget.amended";
/// `control.decision` — the envelope's decision row (E2's second row:
/// `kind: stop, reason: budget_exhausted{dimension}`; also `grace_call` claims).
pub const CLASS_DECISION: &str = "control.decision";
/// `measurement.cost.attributed` — `attribute_spend`'s `SpendRow`.
pub const CLASS_COST_ATTRIBUTED: &str = "measurement.cost.attributed";
/// `context.observation.recorded` — E4's reminder-delivery row (the §05c
/// `context.artefact.*`/`verification.artefact.followed` chain is spec-staged later;
/// at Stage 1 the reminder's delivery is recorded here so advise dedup is
/// ledger-derived — ADR-0236 D-8).
pub const CLASS_OBSERVATION: &str = "context.observation.recorded";

// ── StopReason members owned by this section ─────────────────────────────────

/// `budget_exhausted{dimension}` — E2's stop reason. `approvals_exhausted` is the
/// rendering alias for `dimension = approvals.requested` (§8.2 §3; CF-224).
pub fn stop_reason_budget_exhausted(dimension: DimensionKey) -> String {
    format!("budget_exhausted{{{}}}", dimension.as_str())
}

/// `context_exhausted{required_tokens, cap}` — the *distinct* compaction stop reason
/// (CF-168; window exhaustion is never `budget_exhausted`).
pub fn stop_reason_context_exhausted(required_tokens: i64, cap: i64) -> String {
    format!("context_exhausted{{required_tokens:{required_tokens},cap:{cap}}}")
}

/// The `approvals_exhausted` alias — the rendered form of
/// `budget_exhausted{approvals.requested}` (§8.2 §3 Stop reasons row).
pub const STOP_REASON_APPROVALS_EXHAUSTED: &str = "approvals_exhausted";

/// The `grace_calls_exhausted` member — the grace allowance spent (E3's bound).
pub const STOP_REASON_GRACE_EXHAUSTED: &str = "grace_calls_exhausted";

// ── payload builders ─────────────────────────────────────────────────────────

fn event_ref_json(r: &EventRef) -> Json {
    Json::obj([
        ("run_id", Json::str(&r.run_id)),
        ("event_id", Json::str(&r.event_id)),
    ])
}

fn event_ref_from_json(j: &Json) -> Option<EventRef> {
    Some(EventRef {
        run_id: j.get("run_id")?.as_str()?.to_string(),
        event_id: j.get("event_id")?.as_str()?.to_string(),
    })
}

/// `control.budget.allocated` — the `BudgetNode` record
/// `{budget_id, scope, parent?, mode, spec, created_by, outcome, refusal?}`.
/// `outcome ∈ {allocated, refused}`; a refused allocation is still ledgered (AC-4).
pub fn allocated_payload(
    node: &crate::spec::BudgetNode,
    outcome: &str,
    refusal: Option<&BudgetError>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("budget_id".to_string(), Json::str(&node.budget_id));
    m.insert("scope".to_string(), node.scope.to_json());
    if let Some(p) = &node.parent {
        m.insert("parent".to_string(), Json::str(p));
    }
    m.insert("mode".to_string(), Json::str(node.mode.as_str()));
    m.insert("spec".to_string(), node.spec.to_json());
    m.insert("created_by".to_string(), event_ref_json(&node.created_by));
    m.insert("outcome".to_string(), Json::str(outcome));
    if let Some(r) = refusal {
        m.insert(
            "refusal".to_string(),
            Json::obj([("error", Json::str(r.to_string()))]),
        );
    }
    Json::Obj(m)
}

/// The decoded `allocated` row the projection folds.
#[derive(Debug, Clone)]
pub struct AllocatedRow {
    /// The new node's id.
    pub budget_id: String,
    /// Its scope.
    pub scope: BudgetScope,
    /// Its parent (`None` = the root).
    pub parent: Option<String>,
    /// `slice` | `pool`.
    pub mode: BudgetMode,
    /// The declared spec.
    pub spec: BudgetSpec,
    /// `allocated` | `refused`.
    pub outcome: String,
}

pub fn allocated_from_json(j: &Json) -> Result<AllocatedRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.allocated: {d}"),
    };
    Ok(AllocatedRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        scope: BudgetScope::from_json(j.get("scope").ok_or_else(|| bad("scope"))?)
            .ok_or_else(|| bad("scope decode"))?,
        parent: j.get("parent").and_then(Json::as_str).map(str::to_string),
        mode: BudgetMode::parse(
            j.get("mode")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("mode"))?,
        )
        .ok_or_else(|| bad("mode value"))?,
        spec: BudgetSpec::from_json(j.get("spec").ok_or_else(|| bad("spec"))?)
            .ok_or_else(|| bad("spec decode"))?,
        outcome: j
            .get("outcome")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("outcome"))?
            .to_string(),
    })
}

/// `control.budget.consumed` —
/// `{budget_id, dimension, amount, running_total, limit?, source_event,
///   reservation_id?, attribution, over_reservation?}` (§8.2 §3 verbatim).
#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the payload's.
pub fn consumed_payload(
    budget_id: &str,
    dimension: DimensionId,
    amount: i64,
    running_total: i64,
    limit: Option<i64>,
    source_event: &EventRef,
    reservation_id: Option<&str>,
    attribution: &Attribution,
    cache_ttl: Option<&str>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("budget_id".to_string(), Json::str(budget_id));
    m.insert("dimension".to_string(), Json::str(dimension.as_str()));
    m.insert("amount".to_string(), Json::Int(amount));
    m.insert("running_total".to_string(), Json::Int(running_total));
    if let Some(l) = limit {
        m.insert("limit".to_string(), Json::Int(l));
    }
    m.insert("source_event".to_string(), event_ref_json(source_event));
    if let Some(r) = reservation_id {
        m.insert("reservation_id".to_string(), Json::str(r));
    }
    if let Some(t) = cache_ttl {
        m.insert("cache_ttl".to_string(), Json::str(t));
    }
    m.insert("attribution".to_string(), attribution.to_json());
    Json::Obj(m)
}

/// The decoded `consumed` row.
#[derive(Debug, Clone)]
pub struct ConsumedRow {
    /// The node charged.
    pub budget_id: String,
    /// The primary dimension.
    pub dimension: DimensionId,
    /// The amount (0 for cache hits).
    pub amount: i64,
    /// The producing event.
    pub source_event: EventRef,
    /// The reservation consumed, if any.
    pub reservation_id: Option<String>,
    /// `attribution` (R-ACC-3 fields; `over_reservation`/`cache` inside).
    pub attribution: Attribution,
    /// The `cache_write` `[ttl_class]` qualifier.
    pub cache_ttl: Option<String>,
}

pub fn consumed_from_json(j: &Json) -> Result<ConsumedRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.consumed: {d}"),
    };
    Ok(ConsumedRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        dimension: DimensionId::parse(
            j.get("dimension")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("dimension"))?,
        )
        .ok_or_else(|| bad("dimension value"))?,
        amount: j
            .get("amount")
            .and_then(Json::as_int)
            .ok_or_else(|| bad("amount"))?,
        source_event: event_ref_from_json(
            j.get("source_event").ok_or_else(|| bad("source_event"))?,
        )
        .ok_or_else(|| bad("source_event decode"))?,
        reservation_id: j
            .get("reservation_id")
            .and_then(Json::as_str)
            .map(str::to_string),
        attribution: Attribution::from_json(
            j.get("attribution").ok_or_else(|| bad("attribution"))?,
        )
        .ok_or_else(|| bad("attribution decode"))?,
        cache_ttl: j
            .get("cache_ttl")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

/// `control.budget.reserved` — `{budget_id, reservation_id, holder, quantity, ttl_ms,
/// expires_with, outcome ∈ {held, refused}}`.
pub fn reserved_payload(
    budget_id: &str,
    reservation_id: &str,
    holder: &str,
    quantity: &ResourceVector,
    ttl_ms: u64,
    outcome: &str,
    refusal: Option<&BudgetError>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("budget_id".to_string(), Json::str(budget_id));
    m.insert("reservation_id".to_string(), Json::str(reservation_id));
    m.insert("holder".to_string(), Json::str(holder));
    m.insert("quantity".to_string(), quantity.to_json());
    m.insert("ttl_ms".to_string(), Json::Int(ttl_ms as i64));
    m.insert("expires_with".to_string(), Json::str("lease"));
    m.insert("outcome".to_string(), Json::str(outcome));
    if let Some(r) = refusal {
        m.insert(
            "refusal".to_string(),
            Json::obj([("error", Json::str(r.to_string()))]),
        );
    }
    Json::Obj(m)
}

/// The decoded `reserved` row.
#[derive(Debug, Clone)]
pub struct ReservedRow {
    /// The node.
    pub budget_id: String,
    /// The reservation id.
    pub reservation_id: String,
    /// The holder (`effect:<id>` | `model_call:<id>`).
    pub holder: String,
    /// The claimed quantity.
    pub quantity: ResourceVector,
    /// The TTL hint.
    pub ttl_ms: u64,
    /// `held` | `refused`.
    pub outcome: String,
}

pub fn reserved_from_json(j: &Json) -> Result<ReservedRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.reserved: {d}"),
    };
    Ok(ReservedRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        reservation_id: j
            .get("reservation_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("reservation_id"))?
            .to_string(),
        holder: j
            .get("holder")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("holder"))?
            .to_string(),
        quantity: ResourceVector::from_json(j.get("quantity").ok_or_else(|| bad("quantity"))?)
            .ok_or_else(|| bad("quantity decode"))?,
        ttl_ms: j
            .get("ttl_ms")
            .and_then(Json::as_int)
            .ok_or_else(|| bad("ttl_ms"))? as u64,
        outcome: j
            .get("outcome")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("outcome"))?
            .to_string(),
    })
}

/// `control.budget.released` — `{budget_id, reservation_id?, released, reason}`.
/// `reason ∈ {reservation_excess, release, completion, lease_lost}`.
pub fn released_payload(
    budget_id: &str,
    reservation_id: Option<&str>,
    released: &ResourceVector,
    reason: &str,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("budget_id".to_string(), Json::str(budget_id));
    if let Some(r) = reservation_id {
        m.insert("reservation_id".to_string(), Json::str(r));
    }
    m.insert("released".to_string(), released.to_json());
    m.insert("reason".to_string(), Json::str(reason));
    Json::Obj(m)
}

/// The decoded `released` row.
#[derive(Debug, Clone)]
pub struct ReleasedRow {
    /// The node.
    pub budget_id: String,
    /// The reservation released, if any.
    pub reservation_id: Option<String>,
    /// The amounts released.
    pub released: ResourceVector,
    /// `reservation_excess` | `release` | `completion` | `lease_lost`.
    pub reason: String,
}

pub fn released_from_json(j: &Json) -> Result<ReleasedRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.released: {d}"),
    };
    Ok(ReleasedRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        reservation_id: j
            .get("reservation_id")
            .and_then(Json::as_str)
            .map(str::to_string),
        released: ResourceVector::from_json(j.get("released").ok_or_else(|| bad("released"))?)
            .ok_or_else(|| bad("released decode"))?,
        reason: j
            .get("reason")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("reason"))?
            .to_string(),
    })
}

/// `control.budget.exceeded` — `Exceeded{budget_id, dimension, value, limit}`
/// (§8.2 `check` return shape).
pub fn exceeded_payload(exceeded: &crate::tree::Exceeded) -> Json {
    Json::obj([
        ("budget_id", Json::str(&exceeded.budget_id)),
        ("dimension", Json::str(exceeded.dimension.as_str())),
        ("value", Json::Int(exceeded.value)),
        ("limit", Json::Int(exceeded.limit)),
    ])
}

/// The decoded `exceeded` row.
#[derive(Debug, Clone)]
pub struct ExceededRow {
    /// The node.
    pub budget_id: String,
    /// The dimension (primary or derived bound name).
    pub dimension: DimensionKey,
    /// The measured value.
    pub value: i64,
    /// The limit.
    pub limit: i64,
}

pub fn exceeded_from_json(j: &Json) -> Result<ExceededRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.exceeded: {d}"),
    };
    Ok(ExceededRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        dimension: DimensionKey::parse(
            j.get("dimension")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("dimension"))?,
        )
        .ok_or_else(|| bad("dimension value"))?,
        value: j
            .get("value")
            .and_then(Json::as_int)
            .ok_or_else(|| bad("value"))?,
        limit: j
            .get("limit")
            .and_then(Json::as_int)
            .ok_or_else(|| bad("limit"))?,
    })
}

/// `control.budget.amended` — `{budget_id, old, new, authority}` (§8.2 `amend`).
pub fn amended_payload(
    budget_id: &str,
    old: &BudgetSpec,
    new: &BudgetSpec,
    authority: &str,
) -> Json {
    Json::obj([
        ("budget_id", Json::str(budget_id)),
        ("old", old.to_json()),
        ("new", new.to_json()),
        ("authority", Json::str(authority)),
    ])
}

/// The decoded `amended` row.
#[derive(Debug, Clone)]
pub struct AmendedRow {
    /// The node.
    pub budget_id: String,
    /// The new spec.
    pub new: BudgetSpec,
}

pub fn amended_from_json(j: &Json) -> Result<AmendedRow, BudgetError> {
    let bad = |d: &str| BudgetError::CorruptPayload {
        detail: format!("control.budget.amended: {d}"),
    };
    Ok(AmendedRow {
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_id"))?
            .to_string(),
        new: BudgetSpec::from_json(j.get("new").ok_or_else(|| bad("new"))?)
            .ok_or_else(|| bad("spec decode"))?,
    })
}

/// `control.decision` — `{kind, reason?, budget_id?, dimension?, triggered_by?,
/// decider}`. Kinds used here: `stop` (E2), `grace_call` (E3).
pub fn decision_payload(
    kind: &str,
    decider: &str,
    reason: Option<&str>,
    budget_id: Option<&str>,
    dimension: Option<DimensionKey>,
    triggered_by: Option<&EventRef>,
    call_no: Option<u32>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("kind".to_string(), Json::str(kind));
    m.insert("decider".to_string(), Json::str(decider));
    if let Some(r) = reason {
        m.insert("reason".to_string(), Json::str(r));
    }
    if let Some(b) = budget_id {
        m.insert("budget_id".to_string(), Json::str(b));
    }
    if let Some(d) = dimension {
        m.insert("dimension".to_string(), Json::str(d.as_str()));
    }
    if let Some(t) = triggered_by {
        m.insert("triggered_by".to_string(), event_ref_json(t));
    }
    if let Some(n) = call_no {
        m.insert("call_no".to_string(), Json::Int(n as i64));
    }
    Json::Obj(m)
}

/// The decoded `decision` row.
#[derive(Debug, Clone)]
pub struct DecisionRow {
    /// The event that carried the decision.
    pub event_id: String,
    /// `stop` | `grace_call` | other.
    pub kind: String,
    /// The reason member (e.g. `budget_exhausted{model_calls}`).
    pub reason: Option<String>,
    /// The budget the decision names.
    pub budget_id: Option<String>,
    /// The dimension named.
    pub dimension: Option<DimensionKey>,
}

pub fn decision_from_json(j: &Json, event_id: &str) -> Option<DecisionRow> {
    Some(DecisionRow {
        event_id: event_id.to_string(),
        kind: j.get("kind")?.as_str()?.to_string(),
        reason: j.get("reason").and_then(Json::as_str).map(str::to_string),
        budget_id: j
            .get("budget_id")
            .and_then(Json::as_str)
            .map(str::to_string),
        dimension: j
            .get("dimension")
            .and_then(Json::as_str)
            .and_then(DimensionKey::parse),
    })
}

/// `measurement.cost.attributed` — the `SpendRow` payload (§8.2 §3).
pub fn cost_attributed_payload(row: &SpendRow) -> Json {
    row.to_json()
}

/// `context.observation.recorded{kind: "budget_reminder", …}` — E4's once-per
/// (budget, threshold, window, compaction-epoch) delivery record.
pub fn reminder_payload(
    budget_id: &str,
    dimension: DimensionKey,
    threshold_key: &str,
    window: &str,
    epoch: u64,
    item: &Json,
) -> Json {
    Json::obj([
        ("kind", Json::str("budget_reminder")),
        ("budget_id", Json::str(budget_id)),
        ("dimension", Json::str(dimension.as_str())),
        ("threshold", Json::str(threshold_key)),
        ("window", Json::str(window)),
        ("compaction_epoch", Json::Int(epoch as i64)),
        ("item", item.clone()),
    ])
}

/// A fired soft threshold — the `advise` return member (E4).
#[derive(Debug, Clone)]
pub struct ThresholdFire {
    /// The node whose threshold fired.
    pub budget_id: String,
    /// The bound key.
    pub dimension: DimensionKey,
    /// The threshold's stable key (`{at}@{rule}`).
    pub threshold: String,
    /// The context window the reminder is for.
    pub window: String,
    /// The compaction epoch (re-arm boundary).
    pub compaction_epoch: u64,
    /// The measured consumption at fire time.
    pub consumed: i64,
    /// The hard limit.
    pub limit: i64,
    /// The `HarnessRule` ref whose action renders the reminder.
    pub action: String,
}

/// The decoded reminder-delivery row (advise dedup).
#[derive(Debug, Clone)]
pub struct ReminderRow {
    /// The node.
    pub budget_id: String,
    /// The dimension.
    pub dimension: DimensionKey,
    /// The threshold key.
    pub threshold: String,
    /// The context window.
    pub window: String,
    /// The compaction epoch the delivery was recorded under.
    pub compaction_epoch: u64,
}

/// Decode a `context.observation.recorded{kind:"budget_reminder"}` row; `None` for
/// other observation kinds.
pub fn reminder_from_json(j: &Json) -> Option<ReminderRow> {
    if j.get("kind").and_then(Json::as_str) != Some("budget_reminder") {
        return None;
    }
    Some(ReminderRow {
        budget_id: j.get("budget_id")?.as_str()?.to_string(),
        dimension: DimensionKey::parse(j.get("dimension")?.as_str()?)?,
        threshold: j.get("threshold")?.as_str()?.to_string(),
        window: j.get("window")?.as_str()?.to_string(),
        compaction_epoch: j.get("compaction_epoch")?.as_int()? as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::ChargedTo;

    #[test]
    fn consumed_payload_round_trip() {
        let mut a = Attribution::subject("run-1", "budget-1", "participant-0");
        a.charged_to = ChargedTo::Instrument;
        let src = EventRef {
            run_id: "run-1".into(),
            event_id: "evt-9".into(),
        };
        let p = consumed_payload(
            "budget-1",
            DimensionId::TokensInputCacheWrite,
            10,
            110,
            Some(1000),
            &src,
            Some("res-1"),
            &a,
            Some("5m"),
        );
        let r = consumed_from_json(&p).unwrap();
        assert_eq!(r.dimension, DimensionId::TokensInputCacheWrite);
        assert_eq!(r.amount, 10);
        assert_eq!(r.reservation_id.as_deref(), Some("res-1"));
        assert_eq!(r.cache_ttl.as_deref(), Some("5m"));
        assert_eq!(r.attribution.charged_to, ChargedTo::Instrument);
    }

    #[test]
    fn stop_reason_members() {
        assert_eq!(
            stop_reason_budget_exhausted(DimensionKey::Primary(DimensionId::ModelCalls)),
            "budget_exhausted{model_calls}"
        );
        assert_eq!(
            stop_reason_budget_exhausted(DimensionKey::Primary(DimensionId::ApprovalsRequested)),
            "budget_exhausted{approvals.requested}"
        );
        assert_eq!(
            stop_reason_context_exhausted(12_000, 8_192),
            "context_exhausted{required_tokens:12000,cap:8192}"
        );
    }
}
