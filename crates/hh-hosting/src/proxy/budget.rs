//! `proxy::budget` — the §4.4 mechanical spend layer (R6.11 / BUDGET;
//! REQ-XC-112/113/114/115/126/210/004, REQ-XH-005/120; design/33 F10,
//! F14 (m only)).
//!
//! Everything here is a pure function over declared inputs or a read of
//! a harvested proxy ledger — enforcement itself rides `Account` under
//! the phase proxy run's fenced writer (F10(a): one root pool + one
//! `slice` per slot; slot exhaustion is a reserve refusal, never
//! `exhaust`; only root exhaustion emits `control.budget.exceeded`).

use std::path::Path;

use hharness_budget::pricing::{PricingUnit, RateUnit};
use hharness_ledger::store::Store;
use hharness_wire::json::Json;

use super::ProxyError;

/// REQ-XC-004 / REQ-XH-005 / REQ-XH-120 — the HH-recorded,
/// operator-authorized spend basis of one phase proxy run. The
/// allocation record cites `verbatim`; the provider balance or a
/// period reset is never authorization (X-02).
#[derive(Debug, Clone)]
pub struct ProxyAuthorization {
    /// The verbatim operator authorization (cited on the run's
    /// allocation record; empty refuses typed).
    pub verbatim: String,
    /// The gate record the readout cites (`FP-S`/`FP-E` …).
    pub gate_record_ref: String,
    /// `A` — the authorized amount in µ$.
    pub authorized_micros: i64,
    /// The model credit available to the pass (included + purchased).
    pub available_credit_micros: i64,
    /// The settlement margin `m` in ppm of 1.0, read from the gate
    /// record (.20 → 200_000; the proxy never derives it — Q-X36).
    pub m_ppm: i64,
    /// Σ prior windows' harvested spends (REQ-XH-120 carry-over).
    pub prior_windows: Vec<WindowSpend>,
}

/// One prior window's harvested proxy-ledger spend (REQ-XH-120).
#[derive(Debug, Clone)]
pub struct WindowSpend {
    /// The harvested run id (the window's proxy run).
    pub window_id: String,
    /// Σ `measurement.cost.attributed` micro-units (orphans at hi).
    pub consumed_micros: i64,
    /// `proxy_aborted` terminal count.
    pub orphan_calls: u64,
    /// Σ `bounded.hi` charged for orphaned calls (already inside
    /// `consumed_micros`; recorded for the readout).
    pub orphan_bounded_hi_micros: i64,
    /// The ledger was read.
    pub harvested: bool,
    /// `store.verify` passed byte-exact.
    pub verified: bool,
}

impl ProxyAuthorization {
    /// `A_eff = min(A, available credit)` — credit can only lower the
    /// ceiling, never raise it above the recorded authorization (a
    /// provider balance reset is never new authorization).
    pub fn a_eff_micros(&self) -> i64 {
        self.authorized_micros.min(self.available_credit_micros)
    }

    /// `root_cap = A_eff × (1 − m)` (C-15, REQ-XH-005).
    pub fn root_cap_micros(&self) -> i64 {
        self.a_eff_micros() * (1_000_000 - self.m_ppm) / 1_000_000
    }

    /// The window's root pool = `cap − Σ prior windows' consumed`
    /// (orphans already inside consumed at `bounded.hi`). An
    /// unharvested or unverified ledger refuses the next window
    /// (REQ-XH-120); a negative carried pool refuses typed.
    pub fn pool_micros(&self) -> Result<i64, ProxyError> {
        for w in &self.prior_windows {
            if !w.harvested || !w.verified {
                return Err(ProxyError::Config(format!(
                    "unharvested_or_unverified_ledger: {} blocks the next window",
                    w.window_id
                )));
            }
        }
        let prior: i64 = self.prior_windows.iter().map(|w| w.consumed_micros).sum();
        let pool = self.root_cap_micros() - prior;
        if pool < 0 {
            return Err(ProxyError::Config(format!(
                "carried_pool_exhausted: cap {} − prior {} < 0",
                self.root_cap_micros(),
                prior
            )));
        }
        Ok(pool)
    }

    /// The typed refusal of REQ-XC-004: a missing authorization refuses
    /// before any allocation.
    pub fn validate(&self) -> Result<(), ProxyError> {
        if self.verbatim.trim().is_empty() {
            return Err(ProxyError::Config(
                "missing_authorization: no verbatim operator authorization".into(),
            ));
        }
        if self.gate_record_ref.trim().is_empty() {
            return Err(ProxyError::Config(
                "missing_authorization: no gate record ref".into(),
            ));
        }
        if !(0..1_000_000).contains(&self.m_ppm) {
            return Err(ProxyError::Config(format!(
                "missing_authorization: m_ppm {} out of range",
                self.m_ppm
            )));
        }
        Ok(())
    }

    /// The allocation record's `authorization` member (manifest
    /// `extra.proxy.authorization`) — the verbatim citation.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("verbatim", Json::str(&self.verbatim)),
            ("gate_record_ref", Json::str(&self.gate_record_ref)),
            ("a_micros", Json::Int(self.authorized_micros)),
            (
                "available_credit_micros",
                Json::Int(self.available_credit_micros),
            ),
            ("a_eff_micros", Json::Int(self.a_eff_micros())),
            ("m_ppm", Json::Int(self.m_ppm)),
            ("root_cap_micros", Json::Int(self.root_cap_micros())),
            (
                "prior_windows",
                Json::Arr(
                    self.prior_windows
                        .iter()
                        .map(|w| {
                            Json::obj([
                                ("window_id", Json::str(&w.window_id)),
                                ("consumed_micros", Json::Int(w.consumed_micros)),
                                ("orphan_calls", Json::Int(w.orphan_calls as i64)),
                                (
                                    "orphan_bounded_hi_micros",
                                    Json::Int(w.orphan_bounded_hi_micros),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `harvest_window_spend(store_root, run_id)` — fold one finished
/// proxy ledger into a [`WindowSpend`]: `verified` rides `Store::verify`
/// (the byte-exact WAL replay), `consumed` is Σ
/// `measurement.cost.attributed` micro-units, and `proxy_aborted`
/// terminals carry their `bounded{lo:0,hi}` charges at hi (REQ-XC-124,
/// REQ-XH-120).
pub fn harvest_window_spend(store_root: &Path, run_id: &str) -> Result<WindowSpend, ProxyError> {
    let store = Store::open(store_root).map_err(|e| ProxyError::Ledger(format!("{e}")))?;
    if !store.has_run(run_id) {
        return Err(ProxyError::Config(format!(
            "unharvested ledger: no run {run_id}"
        )));
    }
    store
        .verify(run_id)
        .map_err(|e| ProxyError::Ledger(format!("unverified ledger {run_id}: {e}")))?;
    let envs = store
        .envelopes(run_id)
        .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
    let aborted: std::collections::BTreeSet<String> = envs
        .iter()
        .filter(|e| {
            e.class == "model.call.failed"
                && e.payload.get("disposition").and_then(Json::as_str) == Some("proxy_aborted")
        })
        .map(|e| e.event_id.clone())
        .collect();
    let mut consumed = 0i64;
    let mut orphan_hi = 0i64;
    for e in envs {
        if e.class != "measurement.cost.attributed" {
            continue;
        }
        let micros = e
            .payload
            .get("money")
            .and_then(|m| m.get("micro_units"))
            .and_then(Json::as_int)
            .unwrap_or(0);
        consumed += micros;
        if aborted.contains(
            e.payload
                .get("source_event")
                .and_then(|s| s.get("event_id"))
                .and_then(Json::as_str)
                .unwrap_or(""),
        ) {
            orphan_hi += micros;
        }
    }
    Ok(WindowSpend {
        window_id: run_id.to_string(),
        consumed_micros: consumed,
        orphan_calls: aborted.len() as u64,
        orphan_bounded_hi_micros: orphan_hi,
        harvested: true,
        verified: true,
    })
}

/// `ceil(rate_in·(body_bytes + template_overhead) + rate_out·max_output_bound)`
/// at the uncached input rate — REQ-XC-113's worst-case reservation,
/// REQ-XC-210's no-cache planning value until a measured stage records
/// `cached_share_ppm` (then the cached share prices at the cache-read
/// rate). The exact nano-sum rounds once (REQ-XC-116's ceil rule).
pub fn reservation_micros(
    body_bytes: i64,
    template_overhead_tokens: i64,
    max_output_bound: i64,
    cached_share_ppm: Option<i64>,
    table: &impl PricingUnit,
) -> Result<i64, ProxyError> {
    let row = table
        .pricing_table()
        .rows
        .first()
        .ok_or_else(|| ProxyError::Config("pricing table has no rows".into()))?;
    // nano-µ$ per (rate × token): `micro_per_1k` products are already
    // nano-µ$; `micro_per_token` products scale ×1000 (AR-09).
    let nano_scale = match table.rate_unit() {
        RateUnit::MicroPer1kTokens => 1i64,
        RateUnit::MicroPerToken => 1_000i64,
    };
    let input_tokens = body_bytes + template_overhead_tokens;
    let (uncached, cached) = match cached_share_ppm {
        Some(ppm) if ppm > 0 => (
            input_tokens * (1_000_000 - ppm.min(1_000_000)) / 1_000_000,
            input_tokens * ppm.min(1_000_000) / 1_000_000,
        ),
        _ => (input_tokens, 0),
    };
    // rate×qty products are already nano-µ$ for `micro_per_1k`
    // (µ$/1k-token × tokens) and scale ×1000 for `micro_per_token`
    // (AR-09's single-rounding rule keeps the exact nano-sum).
    let nano = nano_scale
        * (row.input_uncached * uncached
            + row.input_cache_read * cached
            + row.output_visible * max_output_bound);
    Ok((nano + 999) / 1_000)
}

/// `w` — one worst-case call reservation at the declared body-bytes
/// bound (REQ-XC-126; ≈ $0.10 at glm rates for a 200 KB request).
pub fn worst_case_call_micros(
    declared_body_bytes: i64,
    template_overhead_tokens: i64,
    max_output_bound: i64,
    cached_share_ppm: Option<i64>,
    table: &impl PricingUnit,
) -> Result<i64, ProxyError> {
    reservation_micros(
        declared_body_bytes,
        template_overhead_tokens,
        max_output_bound,
        cached_share_ppm,
        table,
    )
}

/// The slice floor: `p90_$/trial + w` (the planning prior until ≥ 10
/// trials are measured — REQ-XC-126).
pub fn slice_floor_micros(p90_trial_micros: i64, worst_case_call_micros: i64) -> i64 {
    p90_trial_micros + worst_case_call_micros
}

/// The slice-validator failure (AC-4.4-11 leg 1).
#[derive(Debug, Clone, PartialEq)]
pub enum SliceViolation {
    /// A slice below `P90 + w`.
    BelowFloor {
        /// The slot id.
        slot_id: String,
        /// The declared slice.
        slice_micros: i64,
        /// The floor.
        floor_micros: i64,
    },
    /// Unequal slices inside one comparison (the common per-trial
    /// condition).
    Unequal {
        /// The first slice seen.
        expected_micros: i64,
        /// The slot whose slice differed.
        slot_id: String,
        /// Its slice.
        actual_micros: i64,
    },
}

/// `validate_slices(slots, floor)` — every slice ≥ `floor` and equal
/// within the comparison (REQ-XC-126).
pub fn validate_slices(slots: &[(String, i64)], floor_micros: i64) -> Result<(), SliceViolation> {
    let mut first: Option<i64> = None;
    for (slot_id, slice) in slots {
        if *slice < floor_micros {
            return Err(SliceViolation::BelowFloor {
                slot_id: slot_id.clone(),
                slice_micros: *slice,
                floor_micros,
            });
        }
        match first {
            None => first = Some(*slice),
            Some(e) if e != *slice => {
                return Err(SliceViolation::Unequal {
                    expected_micros: e,
                    slot_id: slot_id.clone(),
                    actual_micros: *slice,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

/// The feasibility check: `C × s` in-flight slices must fit the root
/// pool (REQ-XC-126 — counts slices, never mean costs). Returns the
/// remaining pool after `c` slices.
pub fn feasibility_remaining_micros(
    root_pool_micros: i64,
    c_in_flight: i64,
    slice_micros: i64,
) -> i64 {
    root_pool_micros - c_in_flight * slice_micros
}

/// REQ-XC-126's premature-refusal predicate: a slot refusal while
/// `consumed < s − w` is premature (`inconclusive{infra:budget_reservation}`,
/// never scored); at or above it is `budget_refused`.
pub fn refusal_is_premature(slot_consumed_micros: i64, slice_micros: i64, w_micros: i64) -> bool {
    slot_consumed_micros < slice_micros - w_micros
}

/// `parse_rfc3339_ms(s)` — the instant inside an Ollama `modified_at`
/// (`2026-08-26T14:00:00Z` / `…+02:00` / fractional seconds). The
/// catalog tuple compares instants, never strings (§4.6): a cosmetic
/// formatting change (`+00:00` vs `Z`, `.000`) is not drift.
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut dp = date.split('-');
    let (y, mo, d): (i64, i64, i64) = (
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
    );
    let (hms, off) = match time.split_once('Z') {
        Some((h, _)) => (h, 0i64),
        None => {
            let (h, sign, o) = if let Some((h, o)) = time.split_once('+') {
                (h, 1i64, o)
            } else {
                let (h, o) = time.split_once('-')?;
                (h, -1i64, o)
            };
            let mut op = o.split(':');
            let oh: i64 = op.next()?.parse().ok()?;
            let om: i64 = op.next().unwrap_or("0").parse().ok()?;
            (h, sign * (oh * 3600 + om * 60))
        }
    };
    let (hms, frac) = hms.split_once('.').unwrap_or((hms, "0"));
    let mut tp = hms.split(':');
    let (h, mi, sec): (i64, i64, i64) = (
        tp.next()?.parse().ok()?,
        tp.next()?.parse().ok()?,
        tp.next()?.parse().ok()?,
    );
    let frac_ms: i64 = {
        let f: String = frac.chars().take(3).collect();
        let pad = format!("{:0<3}", f);
        pad.parse().ok()?
    };
    // days-from-civil (the inverse of store.rs::rfc3339_ms).
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + sec - off) * 1000) + frac_ms)
}
