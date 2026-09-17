//! The `DecisionView` — the broker's read-only fold over a run's
//! `security.permission.*` rows (§5g.3 §2's PDP/CDP gate; ADR-0058 D1). The
//! broker **never decides** — it verifies that the monitor *recorded* a
//! `decided{decision = allow}` covering a `secret_access` grant for the
//! channel and holder, then delivers. `DecisionMissing` when the record isn't
//! there (AC-R-2.8.3-13).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::kinds::EffectDomain;
use hh_ledger::event::EventEnvelope;
use hh_monitor::args::scope_covers;
use hh_monitor::events::handle_from_granted;
use hh_monitor::AuthorityHandle;
use hh_wire::json::Json;

/// One `security.permission.decided` row, projected to the members the gate
/// reads (`{event_id, effect_id, decision, handle_ids}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecidedRow {
    /// The event id — `bind`'s `monitor_decision_ref` names it.
    pub event_id: String,
    /// The effect the decision was for (`used` rows must reference it).
    pub effect_id: String,
    /// `allow` | `ask` | `deny` (closed tag).
    pub decision: String,
    /// The covering handle ids the decision relied on.
    pub handle_ids: Vec<String>,
}

/// The folded view — decided rows, live grant handles, revoked handle ids.
#[derive(Debug, Clone, Default)]
pub struct DecisionView {
    /// `event_id → decided row`.
    pub decided: BTreeMap<String, DecidedRow>,
    /// `handle_id → granted handle` (`security.permission.granted`).
    pub handles: BTreeMap<String, AuthorityHandle>,
    /// Revoked handle ids (`security.permission.revoked` — `handle_id` plus the
    /// cascade list).
    pub revoked_handles: BTreeSet<String>,
}

/// Fold a run's committed events into the view (pure — the broker reads the
/// *recorded* rows, never model text; the fold is over the same events the
/// monitor's own table projection reads).
pub fn fold(events: &[EventEnvelope]) -> DecisionView {
    let mut v = DecisionView::default();
    for env in events {
        match env.class.as_str() {
            "security.permission.decided" => {
                let p = &env.payload;
                let decision = p.get("decision").and_then(Json::as_str).unwrap_or("");
                let effect_id = p.get("effect_id").and_then(Json::as_str).unwrap_or("");
                let handle_ids = match p.get("handle_ids") {
                    Some(Json::Arr(ids)) => ids
                        .iter()
                        .filter_map(Json::as_str)
                        .map(String::from)
                        .collect(),
                    _ => Vec::new(),
                };
                v.decided.insert(
                    env.event_id.clone(),
                    DecidedRow {
                        event_id: env.event_id.clone(),
                        effect_id: effect_id.to_string(),
                        decision: decision.to_string(),
                        handle_ids,
                    },
                );
            }
            "security.permission.granted" => {
                if let Some(h) = handle_from_granted(env) {
                    v.handles.insert(h.handle_id.as_str().to_string(), h);
                }
            }
            "security.permission.revoked" => {
                if let Some(id) = env.payload.get("handle_id").and_then(Json::as_str) {
                    v.revoked_handles.insert(id.to_string());
                }
                if let Some(Json::Arr(cascade)) = env.payload.get("cascade") {
                    for c in cascade {
                        if let Some(id) = c.as_str() {
                            v.revoked_handles.insert(id.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    v
}

/// A covering decision — the decided row + the handle that covers.
#[derive(Debug, Clone)]
pub struct CoveringDecision {
    /// The decided row (the `monitor_decision_ref`).
    pub decided_event_id: String,
    /// The `effect_id` the decision was for (`used` references it).
    pub effect_id: String,
    /// The covering handle id.
    pub handle_id: String,
}

/// Does the view contain a `decided{decision = allow}` whose handle set grants
/// `secret_access` covering `secret:<channel_id>` for `holder` — with the
/// handle still live (not revoked)?
///
/// - `decision_ref`: when `Some`, only that decided event is considered
///   (the `bind` path's `monitor_decision_ref`); when `None`, any covering
///   decided row counts (the `mediate` re-check).
/// - coverage: `grant.effect.domain == secret_access` ∧ `scope_covers(grant.
///   scope, "secret:<channel_id>")` ∧ `handle.holder == holder`.
pub fn covering_secret_access(
    view: &DecisionView,
    decision_ref: Option<&str>,
    channel_id: &str,
    holder: &str,
) -> Option<CoveringDecision> {
    let scope = crate::channel::SecretChannel::grant_scope(channel_id);
    let rows: Vec<&DecidedRow> = match decision_ref {
        Some(r) => view.decided.get(r).into_iter().collect(),
        None => view.decided.values().collect(),
    };
    for row in rows {
        if row.decision != "allow" {
            continue;
        }
        for hid in &row.handle_ids {
            if view.revoked_handles.contains(hid) {
                continue;
            }
            let Some(h) = view.handles.get(hid) else {
                continue;
            };
            if h.holder.semantic_id != holder {
                continue;
            }
            let covers = h.grants.iter().any(|g| {
                g.effect.domain == EffectDomain::SecretAccess
                    && scope_covers(&g.scope, Some(scope.as_str()))
            });
            if covers {
                return Some(CoveringDecision {
                    decided_event_id: row.event_id.clone(),
                    effect_id: row.effect_id.clone(),
                    handle_id: hid.clone(),
                });
            }
        }
    }
    None
}
