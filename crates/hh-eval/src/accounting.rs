//! Model-plane accounting completeness (S3.7; AC-R-2.3.1-10, AC-R-2.3.2-3,
//! AC-R-2.3.4-6).
//!
//! `model_accounting` walks the model-plane members of one run's
//! [`LedgerFacts`] and returns the typed violations — never a warning, never
//! a dropped row. The checks are the executable form of the §5b accounting
//! contract:
//!
//! - exactly one `control.budget.consumed{dimension: model_calls}` charge per
//!   `model.call.completed`/`model.call.failed` terminal, and one attempt
//!   span (`attempt.started` + exactly one `attempt.completed`/`failed`) per
//!   `(model_call_id, attempt_no)` (AC-R-2.3.1-10);
//! - probe/discovery calls and their lookups charge `instrument`, never the
//!   subject (AC-R-2.3.1-10);
//! - a charge's `attribution.model_ref` equals the latest
//!   `model.route.decided.selected` / `model.rerouted.to` for the call
//!   (AC-R-2.3.2-3 — the charge equals the decision);
//! - a `served_from_cache` terminal has a `model.cache.resolved` row naming
//!   it and a zero-amount `model_calls` charge with `cache_hit = true`
//!   (AC-R-2.3.4-6);
//! - an `avoided{…}` estimate carries `provenance = estimated_from_pricing`
//!   (AC-R-2.3.4-6);
//! - every charge/spend row carries `charged_to ∈ {subject, instrument}` —
//!   subject + instrument sums are the raw totals by construction
//!   (AC-R-2.3.4-6);
//! - a served terminal stamps `timing = n/a{not_run}` — a cache hit never
//!   reports a live latency (AC-R-2.3.4-11).

use std::collections::BTreeMap;

use crate::facts::{AttemptPhase, LedgerFacts};

/// The call purposes whose charges attribute to `instrument` (probes and
/// discovery never charge the subject — §5b.1's accounting row).
const INSTRUMENT_PURPOSES: &[&str] = &["probe", "discovery"];

/// A model-plane accounting violation — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum AccountingViolation {
    /// A `model.call.completed`/`model.call.failed` terminal joined to other
    /// than exactly one `control.budget.consumed{dimension: model_calls}`
    /// charge.
    CallChargeCount {
        /// The call id.
        model_call_id: String,
        /// The `model_calls`-dimension charge count found.
        charges: usize,
    },
    /// A charge pricing a model (`attribution.model_ref` set) whose
    /// `source_event` resolves to no `model.call.*` row — an unaccounted
    /// charge source.
    ChargeSourceUnresolved {
        /// The charge row's seq.
        seq: u64,
    },
    /// An attempt span is malformed: `attempt.started` without a terminal,
    /// a terminal without `started`, a duplicated phase, or `attempt_no`
    /// absent.
    AttemptSpan {
        /// The call id.
        model_call_id: String,
        /// The detail.
        detail: String,
    },
    /// A probe/discovery-purpose call whose charge is not
    /// `charged_to = instrument`.
    InstrumentCharge {
        /// The call id.
        model_call_id: String,
        /// The charge's declared `charged_to`.
        charged_to: Option<String>,
    },
    /// The charge's `attribution.model_ref` disagrees with the latest
    /// `model.route.decided`/`model.rerouted` selection for the call
    /// (the charge equals the decision — AC-R-2.3.2-3).
    ChargeRouteMismatch {
        /// The call id.
        model_call_id: String,
        /// The detail.
        detail: String,
    },
    /// A `served_from_cache` terminal without a `model.cache.resolved` row
    /// naming it, or whose `model_calls` charge is not a zero-amount
    /// `cache_hit` charge, or whose terminal lacks the `timing = n/a{…}`
    /// stamp.
    CacheServeUnaccounted {
        /// The call id.
        model_call_id: String,
        /// The detail.
        detail: String,
    },
    /// A `model.cache.resolved.avoided{…}` estimate without
    /// `provenance = estimated_from_pricing`.
    AvoidedProvenance {
        /// The resolution row's seq.
        seq: u64,
    },
    /// A charge or spend row carrying no `charged_to ∈ {subject,
    /// instrument}` — the subject/instrument split must sum to the raw
    /// totals, which an unclassified row breaks.
    ChargedToMissing {
        /// The row's seq.
        seq: u64,
        /// `charge` (`control.budget.consumed`) or `spend`
        /// (`measurement.cost.attributed`).
        class: &'static str,
    },
}

impl std::fmt::Display for AccountingViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountingViolation::CallChargeCount {
                model_call_id,
                charges,
            } => write!(
                f,
                "call {model_call_id}: {charges} model_calls charges (exactly one required)"
            ),
            AccountingViolation::ChargeSourceUnresolved { seq } => {
                write!(f, "charge at seq {seq}: model-priced charge joins no call")
            }
            AccountingViolation::AttemptSpan {
                model_call_id,
                detail,
            } => write!(f, "call {model_call_id}: attempt span: {detail}"),
            AccountingViolation::InstrumentCharge {
                model_call_id,
                charged_to,
            } => write!(
                f,
                "call {model_call_id}: probe/discovery charge is {charged_to:?}, not instrument"
            ),
            AccountingViolation::ChargeRouteMismatch {
                model_call_id,
                detail,
            } => write!(f, "call {model_call_id}: charge/route mismatch: {detail}"),
            AccountingViolation::CacheServeUnaccounted {
                model_call_id,
                detail,
            } => write!(f, "call {model_call_id}: cache serve unaccounted: {detail}"),
            AccountingViolation::AvoidedProvenance { seq } => write!(
                f,
                "cache resolution at seq {seq}: avoided estimate lacks estimated_from_pricing"
            ),
            AccountingViolation::ChargedToMissing { seq, class } => {
                write!(
                    f,
                    "{class} row at seq {seq}: no subject|instrument charged_to"
                )
            }
        }
    }
}

impl std::error::Error for AccountingViolation {}

/// Whether the charge's `model_ref` equals the route row's selection.
fn charge_matches_route(model_ref: &hh_wire::Json, route: &crate::facts::RouteRow) -> bool {
    if route.rerouted {
        // `model.rerouted.to` is the provider-model id — the reroute row's
        // `selected` slot is empty by shape.
        model_ref
            .get("provider_model_id")
            .and_then(hh_wire::Json::as_str)
            == route.to.as_deref()
    } else {
        route.selected.as_ref() == Some(model_ref)
    }
}

/// The model-plane accounting check — the violations in emission order.
/// An empty return is the green state (AC-R-2.3.1-10, AC-R-2.3.2-3,
/// AC-R-2.3.4-6).
pub fn model_accounting(facts: &LedgerFacts) -> Vec<AccountingViolation> {
    let mut out = Vec::new();

    // ── attempt spans ──────────────────────────────────────────────────
    // Group by (model_call_id, attempt_no): exactly one `started` + exactly
    // one of {completed, failed}; `attempt_no` must be carried.
    let mut spans: BTreeMap<(String, u64), Vec<&crate::facts::AttemptRow>> = BTreeMap::new();
    for a in &facts.attempts {
        match a.attempt_no {
            Some(no) => spans
                .entry((a.model_call_id.clone(), no))
                .or_default()
                .push(a),
            None => out.push(AccountingViolation::AttemptSpan {
                model_call_id: a.model_call_id.clone(),
                detail: format!("attempt {} carries no attempt_no", a.phase.as_str()),
            }),
        }
    }
    for ((call, no), rows) in &spans {
        let started = rows
            .iter()
            .filter(|r| r.phase == AttemptPhase::Started)
            .count();
        let terminal = rows
            .iter()
            .filter(|r| r.phase != AttemptPhase::Started)
            .count();
        if started != 1 || terminal != 1 {
            out.push(AccountingViolation::AttemptSpan {
                model_call_id: call.clone(),
                detail: format!(
                    "attempt {no}: {started} started + {terminal} terminal rows (one span each)"
                ),
            });
        }
    }

    // ── per-call charge coverage + instrument/served checks ────────────
    for t in &facts.call_terminals {
        let call_charges: Vec<&crate::facts::ChargeRow> = facts
            .charges
            .iter()
            .filter(|c| c.model_call_id.as_deref() == Some(t.model_call_id.as_str()))
            .collect();
        let call_charges_n = call_charges
            .iter()
            .filter(|c| c.dimension == "model_calls")
            .count();
        if call_charges_n != 1 {
            out.push(AccountingViolation::CallChargeCount {
                model_call_id: t.model_call_id.clone(),
                charges: call_charges_n,
            });
        }
        // Probe/discovery calls charge `instrument` (AC-R-2.3.1-10).
        let instrument = facts
            .call_requests
            .get(&t.model_call_id)
            .and_then(|r| r.purpose.as_deref())
            .map(|p| INSTRUMENT_PURPOSES.contains(&p))
            .unwrap_or(false);
        if instrument {
            for c in &call_charges {
                if c.charged_to.as_deref() != Some("instrument") {
                    out.push(AccountingViolation::InstrumentCharge {
                        model_call_id: t.model_call_id.clone(),
                        charged_to: c.charged_to.clone(),
                    });
                }
            }
        }
        // Charge equals the latest route decision/reroute (AC-R-2.3.2-3).
        let latest_route = facts
            .routes
            .iter()
            .filter(|r| r.model_call_id == t.model_call_id)
            .max_by_key(|r| r.seq);
        if let Some(route) = latest_route {
            for c in &call_charges {
                match &c.model_ref {
                    Some(mr) if charge_matches_route(mr, route) => {}
                    Some(_) => out.push(AccountingViolation::ChargeRouteMismatch {
                        model_call_id: t.model_call_id.clone(),
                        detail: "charge's model_ref ≠ latest route selection".into(),
                    }),
                    None => out.push(AccountingViolation::ChargeRouteMismatch {
                        model_call_id: t.model_call_id.clone(),
                        detail: "charge carries no attribution.model_ref".into(),
                    }),
                }
            }
        }
        // Cache serves: a `served_from_cache` terminal needs the resolved
        // row, the zero-amount `cache_hit` charge, and the n/a timing stamp.
        if t.served_from_cache {
            let resolved = facts
                .cache_resolutions
                .iter()
                .any(|r| r.served_by.as_deref() == Some(t.model_call_id.as_str()));
            if !resolved {
                out.push(AccountingViolation::CacheServeUnaccounted {
                    model_call_id: t.model_call_id.clone(),
                    detail: "no model.cache.resolved row names the served call".into(),
                });
            }
            let hit_charge = call_charges
                .iter()
                .find(|c| c.dimension == "model_calls")
                .map(|c| c.cache_hit && c.amount == 0)
                .unwrap_or(false);
            if !hit_charge {
                out.push(AccountingViolation::CacheServeUnaccounted {
                    model_call_id: t.model_call_id.clone(),
                    detail: "served call's model_calls charge is not a zero-amount cache_hit"
                        .into(),
                });
            }
            if !t.timing_na {
                out.push(AccountingViolation::CacheServeUnaccounted {
                    model_call_id: t.model_call_id.clone(),
                    detail: "served terminal lacks timing = n/a{not_run}".into(),
                });
            }
        }
    }

    // ── charge/spend row classification + source joins ─────────────────
    for c in &facts.charges {
        match c.charged_to.as_deref() {
            Some("subject") | Some("instrument") => {}
            _ => out.push(AccountingViolation::ChargedToMissing {
                seq: c.seq,
                class: "charge",
            }),
        }
        // A model-priced charge must join a call — else the charge has no
        // accountable source.
        if c.model_ref.is_some() && c.model_call_id.is_none() {
            out.push(AccountingViolation::ChargeSourceUnresolved { seq: c.seq });
        }
    }
    for r in &facts.spend_rows {
        match r.charged_to.as_deref() {
            Some("subject") | Some("instrument") => {}
            _ => out.push(AccountingViolation::ChargedToMissing {
                seq: r.seq,
                class: "spend",
            }),
        }
    }

    // ── avoided-cost provenance (AC-R-2.3.4-6) ─────────────────────────
    for r in &facts.cache_resolutions {
        if let Some(avoided) = &r.avoided {
            let ok = avoided.get("provenance").and_then(hh_wire::Json::as_str)
                == Some("estimated_from_pricing")
                || avoided.get("estimated_from_pricing") == Some(&hh_wire::Json::Bool(true));
            if !ok {
                out.push(AccountingViolation::AvoidedProvenance { seq: r.seq });
            }
        }
        // Probe/discovery lookups charge instrument too.
        if r.purpose
            .as_deref()
            .map(|p| INSTRUMENT_PURPOSES.contains(&p))
            == Some(true)
            && r.attribution.as_deref() != Some("instrument")
        {
            out.push(AccountingViolation::ChargedToMissing {
                seq: r.seq,
                class: "charge",
            });
        }
    }
    out
}
