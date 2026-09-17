//! The audit trail (§5g.6 R-2.8.6, C0/Stage 1; ADR-0066/0067/0068) — the run
//! ledger read through `audit_view`. There is no second audit store: the view
//! is a pure fold over the durable prefix (`derived_from` watermark +
//! `view_hash`, like every `project` kind), and `Store::verify` is the
//! byte-exact chain check.
//!
//! Stage-1 reach (the §5g.6 §9 stage map): the audit-grade class list with the
//! `audit_fields`/`content_refs` partition (Rule C), kernel-only producers and
//! `authority = kernel` provenance (Rule P) — both enforced at `append` and
//! re-checked by `verify` — the `AuditObligation` record set evaluated here,
//! and `content_refs` presence accounting over the blob pool. Compact-range
//! tree heads, signed checkpoints, proofs, cross-run anchors, redaction
//! endorsement and the completeness veto are Stage 2+ — the corresponding
//! `AuditView` members render honestly (`[]` / `n/a`), never fabricated.

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

use crate::classes::{self, ScopeKind};
use crate::effect::{EffectFold, EffectPhase};
use crate::event::EventEnvelope;
use crate::views::{View, ViewKind};

/// `AuditObligation` (§5g.6 §3; ADR-0066 D5) — `{id, class, quantifier,
/// target_class, link_field, terminal_only?}`: MUST-data, versioned with the
/// `AuditPolicy`. One row of the Rule-O set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditObligation {
    /// The obligation id (the Rule-O row's name).
    pub id: &'static str,
    /// The source class — the event that must have the link.
    pub class: &'static str,
    /// How many targets the source needs.
    pub quantifier: ObligationQuantifier,
    /// The class(es) the link must resolve to (`|` separates alternatives).
    pub target_class: &'static str,
    /// The payload member carrying the link.
    pub link_field: &'static str,
    /// Evaluate only once `lifecycle.run.finished` has committed — an in-flight
    /// subject is pending work, not a violation (AC-H6-5's fixture reads at
    /// `finished`).
    pub terminal_only: bool,
}

/// The `quantifier` sum — `at_least_one` (a link exists) / `exactly_one`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationQuantifier {
    /// At least one matching target row.
    AtLeastOne,
    /// Exactly one matching target row.
    ExactlyOne,
}

impl ObligationQuantifier {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObligationQuantifier::AtLeastOne => "at_least_one",
            ObligationQuantifier::ExactlyOne => "exactly_one",
        }
    }
}

const fn obligation(
    id: &'static str,
    class: &'static str,
    quantifier: ObligationQuantifier,
    target_class: &'static str,
    link_field: &'static str,
    terminal_only: bool,
) -> AuditObligation {
    AuditObligation {
        id,
        class,
        quantifier,
        target_class,
        link_field,
        terminal_only,
    }
}

/// The Stage-1 obligation set (I-A3; ADR-0066 D5 + P4 log) — the rows whose
/// classes exist at this stage. `terminal_only` rows evaluate only once the run
/// has committed `lifecycle.run.finished`.
pub const OBLIGATIONS: &[AuditObligation] = &[
    // Every non-`read_only` `action.tool.proposed` has exactly one
    // `security.permission.decided` or an `action.effect.refused` (link:
    // `effect_id` — read from the scope or the payload).
    obligation(
        "tool_mediation",
        "action.tool.proposed",
        ObligationQuantifier::ExactlyOne,
        "security.permission.decided|action.effect.refused",
        "effect_id",
        true,
    ),
    // Every opened `effect_id` closes with one terminal (ADR-0030).
    obligation(
        "effect_terminal",
        "action.effect.intended",
        ObligationQuantifier::ExactlyOne,
        "action.effect.observed|action.effect.probed|action.effect.compensated|action.effect.reverted|action.effect.abandoned|action.effect.refused",
        "effect_id",
        true,
    ),
    // Every `decided{decision_scope ≠ once}` has a `granted{scope}` (link:
    // `permission_id` — one act, linked events, one permission_id; I-A5).
    obligation(
        "grant_scope",
        "security.permission.decided",
        ObligationQuantifier::AtLeastOne,
        "security.permission.granted",
        "permission_id",
        false,
    ),
    // Every `security.credential.used` references an `effect_id` with an
    // `action.effect.*` (CF-130).
    obligation(
        "credential_use",
        "security.credential.used",
        ObligationQuantifier::AtLeastOne,
        "action.effect.intended",
        "effect_id",
        false,
    ),
    // Every `action.effect.abandoned` carries an `escalation_ref` resolving to a
    // `lifecycle.escalation.raised` row (§5a.2 `abandon` contract).
    obligation(
        "abandoned_escalated",
        "action.effect.abandoned",
        ObligationQuantifier::ExactlyOne,
        "lifecycle.escalation.raised",
        "escalation_ref",
        false,
    ),
    // Every `unknown` effect open at `finished` has an escalation naming it.
    obligation(
        "unknown_escalated",
        "action.effect.unknown",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.escalation.raised",
        "effect_id",
        true,
    ),
    // Every `control.budget.exceeded` (the hard-exhaustion row — `limit` is the
    // hard bound) has an escalation naming the budget (or `kind = budget_hard`).
    obligation(
        "budget_hard_escalated",
        "control.budget.exceeded",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.escalation.raised",
        "budget_id",
        false,
    ),
    // Every `security.label.endorsed` carries a basis from the ADR-0035 closed
    // list (the append-time check 5 is the enforcement; the obligation is the
    // audit-side recheck).
    obligation(
        "endorsement_basis",
        "security.label.endorsed",
        ObligationQuantifier::ExactlyOne,
        "security.label.endorsed",
        "basis",
        false,
    ),
    // Every persisted widening grant (`authority_delta = widening`,
    // `scope = persisted`) has a human-origin `lifecycle.definition.changed`.
    obligation(
        "persisted_widening",
        "security.permission.granted",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.definition.changed",
        "rule_ref",
        false,
    ),
];

/// Obligations of the I-A3 set whose link machinery lands at a later stage —
/// declared (the set is MUST-data) and reported under `obligations_deferred`,
/// never silently absent:
/// - `subagent_anchor` (`control.subagent.spawned` ↔ child `lifecycle.run.created`
///   + `result|cancelled` carrying the child head — cross-run, Stage 2+);
/// - `evolution_link` (`transitioned{to: proposed}`/`applied` paired by
///   `hypothesis_ref`/`evidence_refs` — the evolution pipeline);
/// - `producer_resolution` (`component_variant_ref` resolves to a loaded,
///   pin-attested extension or a sealed-definition member — the registry fold);
/// - `fleet_anchor` (activation `lifecycle.run.created.causes[]` ↔ the fleet
///   run's `control.work_item.dispatched` — §05i).
pub const DEFERRED_OBLIGATIONS: &[&str] = &[
    "subagent_anchor",
    "evolution_link",
    "producer_resolution",
    "fleet_anchor",
];

/// One unmet obligation — `{obligation_id, subject}` (the source event's id or
/// the scoped subject it names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmet {
    /// The obligation.
    pub obligation_id: &'static str,
    /// The subject (the source `event_id`, or the linked id when absent).
    pub subject: String,
}

fn str_member<'a>(e: &'a EventEnvelope, k: &str) -> Option<&'a str> {
    e.payload.get(k).and_then(Json::as_str)
}

/// Evaluate [`OBLIGATIONS`] over the durable prefix. Returns `(checked, unmet)`
/// — `checked` lists every obligation id that ran (the deferred set is declared
/// in [`DEFERRED_OBLIGATIONS`], reported separately).
pub fn eval_obligations(
    events: &[&EventEnvelope],
    effects: &BTreeMap<String, EffectFold>,
    finished: bool,
) -> (Vec<&'static str>, Vec<Unmet>) {
    let mut checked = Vec::new();
    let mut unmet = Vec::new();
    for ob in OBLIGATIONS {
        if ob.terminal_only && !finished {
            continue;
        }
        checked.push(ob.id);
        eval_obligation(ob, events, effects, &mut unmet);
    }
    (checked, unmet)
}

fn eval_obligation(
    ob: &AuditObligation,
    events: &[&EventEnvelope],
    effects: &BTreeMap<String, EffectFold>,
    unmet: &mut Vec<Unmet>,
) {
    let targets: Vec<&EventEnvelope> = events
        .iter()
        .copied()
        .filter(|e| ob.target_class.split('|').any(|c| c == e.class))
        .collect();
    match ob.id {
        "tool_mediation" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                if e.payload.get("read_only") == Some(&Json::Bool(true)) {
                    continue;
                }
                let effect = e
                    .scope
                    .effect_id
                    .as_deref()
                    .or_else(|| str_member(e, "effect_id"));
                let count = match effect {
                    Some(eid) => targets
                        .iter()
                        .filter(|t| str_member(t, "effect_id") == Some(eid))
                        .count(),
                    None => 0,
                };
                if count != 1 {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "effect_terminal" => {
            for f in effects.values() {
                if !f.is_terminal() {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: f.effect_id.clone(),
                    });
                }
            }
        }
        "grant_scope" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let scope = str_member(e, "decision_scope").unwrap_or("once");
                if scope == "once" {
                    continue;
                }
                let pid = str_member(e, "permission_id");
                let ok = match pid {
                    Some(pid) => targets
                        .iter()
                        .any(|t| str_member(t, "permission_id") == Some(pid)),
                    None => false,
                };
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "credential_use" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "effect_id")
                    .map(|eid| effects.contains_key(eid))
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "abandoned_escalated" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "escalation_ref")
                    .map(|r| targets.iter().any(|t| t.event_id == r))
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "unknown_escalated" => {
            for f in effects.values() {
                if f.phase != EffectPhase::Unknown {
                    continue;
                }
                let ok = targets.iter().any(|t| {
                    str_member(t, "effect_id") == Some(f.effect_id.as_str())
                        || str_member(t, "kind") == Some("effect_unknown")
                });
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: f.effect_id.clone(),
                    });
                }
            }
        }
        "budget_hard_escalated" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = targets.iter().any(|t| {
                    str_member(t, "budget_id") == str_member(e, "budget_id")
                        || str_member(t, "kind") == Some("budget_hard")
                });
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "endorsement_basis" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "basis")
                    .map(|b| {
                        hh_provenance::EndorsementBasis::ALL
                            .iter()
                            .any(|k| k.as_str() == b)
                    })
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "persisted_widening" => {
            let human_changed = events.iter().any(|e| {
                e.class == "lifecycle.definition.changed"
                    && e.provenance
                        .as_ref()
                        .map(|p| matches!(p.origin, hh_provenance::Origin::Human { .. }))
                        .unwrap_or(false)
            });
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let widening = str_member(e, "authority_delta") == Some("widening");
                let persisted = str_member(e, "scope") == Some("persisted");
                if widening && persisted && !human_changed {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        _ => {}
    }
}

/// `project(audit_view)` — the §5g.6 §2 record over the durable prefix:
/// `events_seen`, `chain_ok` (the pure recompute — `Store::verify` is the
/// byte-exact disk check), `checkpoints` (empty until Stage 2),
/// `scopes_unclosed`, `content_refs` presence accounting, `producer_violations`
/// (Rule-P recheck over stored rows), `coverage` (the obligation evaluation),
/// `cross_run`, `redactions`, `sink_deliveries`, and the `completeness` vector
/// with per-component `n/a` for the Stage-2 halves (never `false` by absence).
pub fn audit_view(
    run_id: &str,
    events: &[EventEnvelope],
    open_scopes: &BTreeMap<String, ScopeKind>,
    finished: bool,
    blob_present: impl Fn(&str) -> bool,
    until: Option<u64>,
) -> View {
    let events: Vec<&EventEnvelope> = events
        .iter()
        .filter(|e| until.map(|u| e.seq <= u).unwrap_or(true))
        .collect();
    let mut watermark = None;

    // chain_ok — recompute the hash chain over the folded prefix (the same rule
    // `verify` runs over the WAL bytes).
    let mut chain_ok = true;
    let mut prev_hash = crate::ids::GENESIS_HASH.to_string();
    for (i, e) in events.iter().enumerate() {
        if e.seq != i as u64 || e.prev_hash != prev_hash || e.recompute_hash() != e.hash {
            chain_ok = false;
        }
        prev_hash = e.hash.clone();
        watermark = Some(e.seq);
    }

    // producer_violations — Rule P rechecked over the stored rows (empty on any
    // run written through `append`; a forged row shows here and fails `verify`).
    let mut producer_violations = Vec::new();
    for e in &events {
        if let Some(spec) = classes::lookup(&e.class) {
            if !spec.audit_grade {
                continue;
            }
            let producer_ok = spec
                .producers
                .iter()
                .any(|p| *p == e.producer.component_class);
            let authority_ok = e
                .provenance
                .as_ref()
                .map(|p| p.authority == hh_provenance::AuthorityClass::Kernel)
                .unwrap_or(false);
            if !producer_ok || !authority_ok {
                producer_violations.push(Json::obj([
                    ("seq", Json::Int(e.seq as i64)),
                    ("event_id", Json::str(&e.event_id)),
                    ("class", Json::str(&e.class)),
                    ("producer", Json::str(e.producer.component_class.as_str())),
                ]));
            }
        }
    }

    // content_refs accounting — declared content-ref members resolved against
    // the blob pool; `redacted`/`gc` name the tombstoned addresses.
    let mut refs_present: BTreeSet<String> = BTreeSet::new();
    let mut refs_missing: BTreeSet<String> = BTreeSet::new();
    let mut refs_redacted: BTreeSet<String> = BTreeSet::new();
    let mut refs_gc: BTreeSet<String> = BTreeSet::new();
    let mut redaction_rows = Vec::new();
    for e in &events {
        if e.class == "lifecycle.ledger.redacted" {
            redaction_rows.push(Json::str(&e.event_id));
            if let Some(Json::Arr(ts)) = e.payload.get("targets") {
                for t in ts.iter().filter_map(Json::as_str) {
                    refs_redacted.insert(t.to_string());
                }
            }
        }
        if e.class == "lifecycle.ledger.gc" {
            if let Some(Json::Arr(ts)) = e.payload.get("addresses") {
                for t in ts.iter().filter_map(Json::as_str) {
                    refs_gc.insert(t.to_string());
                }
            }
        }
    }
    for e in &events {
        let Some(spec) = classes::lookup(&e.class) else {
            continue;
        };
        if !spec.audit_grade {
            continue;
        }
        let Json::Obj(m) = &e.payload else { continue };
        for (name, v) in m {
            if !spec.content_refs.contains(&name.as_str()) {
                continue;
            }
            let addrs: Vec<&str> = match v {
                Json::Str(s) => vec![s.as_str()],
                Json::Arr(items) => items.iter().filter_map(Json::as_str).collect(),
                _ => vec![],
            };
            for a in addrs {
                if refs_redacted.contains(a) || refs_gc.contains(a) {
                    continue;
                }
                if blob_present(a) {
                    refs_present.insert(a.to_string());
                } else {
                    refs_missing.insert(a.to_string());
                }
            }
        }
    }
    for a in refs_redacted.iter().chain(refs_gc.iter()) {
        refs_present.remove(a);
        refs_missing.remove(a);
    }

    // coverage — the Rule-O obligation set, evaluated.
    let mut effects = BTreeMap::new();
    for e in &events {
        crate::effect::fold_event(&mut effects, e);
    }
    let (checked, unmet) = eval_obligations(&events, &effects, finished);
    let coverage = Json::obj([
        (
            "obligations_checked",
            Json::Arr(checked.iter().map(|id| Json::str(*id)).collect()),
        ),
        (
            "obligations_deferred",
            Json::Arr(
                DEFERRED_OBLIGATIONS
                    .iter()
                    .map(|id| Json::str(*id))
                    .collect(),
            ),
        ),
        (
            "unmet",
            Json::Arr(
                unmet
                    .iter()
                    .map(|u| {
                        Json::obj([
                            ("obligation_id", Json::str(u.obligation_id)),
                            ("subject", Json::str(&u.subject)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);

    // sink_deliveries — every `measurement.export.delivered` row (what left the
    // run through a declared sink).
    let sink_deliveries: Vec<Json> = events
        .iter()
        .filter(|e| e.class == "measurement.export.delivered")
        .map(|e| {
            Json::obj([
                ("seq", Json::Int(e.seq as i64)),
                ("event_id", Json::str(&e.event_id)),
                (
                    "sink_id",
                    str_member(e, "sink_id")
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
                (
                    "view_kind",
                    str_member(e, "view_kind")
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
            ])
        })
        .collect();

    let na = |reason: &str| Json::obj([("n/a", Json::str(reason))]);
    let completeness = Json::obj([
        ("chain_ok", Json::Bool(chain_ok)),
        ("checkpoints_ok", na("checkpoint emitters land at Stage 2")),
        ("scopes_closed", Json::Bool(open_scopes.is_empty())),
        ("blobs_accounted", Json::Bool(refs_missing.is_empty())),
        ("producers_ok", Json::Bool(producer_violations.is_empty())),
        ("coverage_ok", Json::Bool(unmet.is_empty())),
        ("cross_run_ok", na("cross-run anchors land at Stage 2")),
        (
            "headline",
            Json::Bool(
                chain_ok
                    && open_scopes.is_empty()
                    && refs_missing.is_empty()
                    && producer_violations.is_empty()
                    && unmet.is_empty(),
            ),
        ),
    ]);

    let payload = Json::obj([
        ("kind", Json::str("audit_view")),
        ("events_seen", Json::Int(events.len() as i64)),
        ("chain_ok", Json::Bool(chain_ok)),
        ("checkpoints", Json::Arr(vec![])),
        (
            "scopes_unclosed",
            Json::Arr(open_scopes.keys().map(Json::str).collect()),
        ),
        (
            "content_refs",
            Json::obj([
                (
                    "present",
                    Json::Arr(refs_present.iter().map(Json::str).collect()),
                ),
                (
                    "redacted",
                    Json::Arr(refs_redacted.iter().map(Json::str).collect()),
                ),
                ("gc", Json::Arr(refs_gc.iter().map(Json::str).collect())),
                (
                    "missing",
                    Json::Arr(refs_missing.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        ("producer_violations", Json::Arr(producer_violations)),
        ("coverage", coverage),
        ("cross_run", Json::Arr(vec![])),
        ("redactions", Json::Arr(redaction_rows)),
        ("sink_deliveries", Json::Arr(sink_deliveries)),
        ("completeness", completeness),
    ]);
    View::stamped(run_id, ViewKind::AuditView, watermark, payload)
}
