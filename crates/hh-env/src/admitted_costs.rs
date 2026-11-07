//! `evaluate_admitted_costs` — the paid-study cost leg (manifest
//! 267/R6.14; spec §4.5 REQ-XC-117, §3.10; design/33 F12; AR-16).
//! Records-in: every fact is supplied; nothing is recomputed from a
//! store. The four round-5 `$0` evaluators stay byte-unchanged — this
//! leg never reuses them. Unadmitted or over-cap spend refuses.

use crate::preflight::LegVerdict;

/// One packet's spend facts (per F12's `per_packet[]` member).
#[derive(Debug, Clone, PartialEq)]
pub struct PacketCost {
    /// The packet id.
    pub packet: String,
    /// Declared model exposure in µ$.
    pub exposure_micros: i64,
    /// The admission (gate) record ref — required when exposure > 0.
    pub admission_ref: Option<String>,
    /// The packet's admitted cap (from the admission).
    pub cap_micros: Option<i64>,
    /// The settlement status (REQ-XC-121 vocabulary).
    pub settlement: SettlementStatus,
    /// The compute leg (§3.10).
    pub compute: ComputeLeg,
}

/// The model-side settlement status.
#[derive(Debug, Clone, PartialEq)]
pub enum SettlementStatus {
    /// No paid model spend — nothing to settle.
    None,
    /// Reads pending (before the REQ-XC-127 clock or disagreeing).
    Pending,
    /// Reconciled within tolerance.
    Settled,
    /// Reconciled beyond tolerance — refused unless the operator
    /// recorded an acceptance ref.
    SettledDiscrepant {
        /// An operator-recorded acceptance ref, when present.
        accepted_ref: Option<String>,
    },
    /// `unreconcilable{reason}` — e.g. non_exclusive.
    Unreconcilable(String),
}

/// The compute leg (§3.10 predicates).
#[derive(Debug, Clone, PartialEq)]
pub enum ComputeLeg {
    /// Round-5 packets carry no compute member — folds as
    /// `unrecorded{pre_schema}`, never $0.
    UnrecordedPreSchema,
    /// Per-window `hh-compute-cost/1` facts.
    Recorded(Vec<ComputeWindow>),
}

/// One window's compute facts.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeWindow {
    /// The window id.
    pub window: String,
    /// The `hh-compute-cost/1` record ref (absent = missing record).
    pub record_ref: Option<String>,
    /// The teardown `post_state` was empty.
    pub post_state_empty: bool,
    /// Estimate total (µ$).
    pub estimate_micros: i64,
    /// Authorized ceiling (µ$).
    pub ceiling_micros: i64,
    /// The billed status (`pending | within | outside | unavailable`).
    pub billed_status: String,
}

/// The full facts the evaluator folds.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmittedCostFacts {
    /// Per-packet facts.
    pub per_packet: Vec<PacketCost>,
    /// The admissions (gate authorizations) packets may cite.
    pub admissions: Vec<Admission>,
    /// The program cap (µ$).
    pub program_cap_micros: i64,
}

/// One admission: a gate authorization's ref, its cap, its phase.
#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    /// The gate record ref.
    pub ref_: String,
    /// The authorized cap (µ$).
    pub cap_micros: i64,
    /// The phase the admission covers.
    pub phase: String,
}

/// `evaluate_admitted_costs` — the truth-table fold (AC-4.5-2).
pub fn evaluate_admitted_costs(f: &AdmittedCostFacts) -> LegVerdict {
    let mut total = 0i64;
    for p in &f.per_packet {
        total += p.exposure_micros;
        // model leg: exposure 0, or admitted and within cap (F12)
        if p.exposure_micros > 0 {
            let adm = match &p.admission_ref {
                None => return LegVerdict::Refused(format!("unadmitted_spend:{}", p.packet)),
                Some(r) => r,
            };
            let cap = f
                .admissions
                .iter()
                .find(|a| a.ref_ == *adm)
                .map(|a| a.cap_micros)
                .or(p.cap_micros);
            match cap {
                None => return LegVerdict::Refused(format!("unadmitted_spend:{}", p.packet)),
                Some(c) if p.exposure_micros > c => {
                    return LegVerdict::Refused(format!("over_cap:{}", p.packet))
                }
                _ => {}
            }
            match &p.settlement {
                SettlementStatus::Pending => {
                    return LegVerdict::Inconclusive("settlement_pending".into())
                }
                SettlementStatus::SettledDiscrepant { accepted_ref } => {
                    if accepted_ref.is_none() {
                        return LegVerdict::Refused("settlement_discrepant".into());
                    }
                }
                SettlementStatus::Unreconcilable(r) => {
                    return LegVerdict::Inconclusive(format!("unreconcilable:{r}"))
                }
                _ => {}
            }
        }
        // compute leg (§3.10 predicates)
        if let ComputeLeg::Recorded(ws) = &p.compute {
            for w in ws {
                if w.record_ref.is_none() {
                    return LegVerdict::Refused(format!("compute_record_missing:{}", w.window));
                }
                if !w.post_state_empty {
                    return LegVerdict::Refused(format!("post_state_nonempty:{}", w.window));
                }
                if w.estimate_micros > w.ceiling_micros {
                    return LegVerdict::Refused(format!("compute_over_authorization:{}", w.window));
                }
                if w.billed_status == "pending" || w.billed_status == "unavailable" {
                    // billing-pending is inconclusive for publication (XB-218)
                    return LegVerdict::Inconclusive(format!("billing_pending:{}", w.window));
                }
            }
        }
        // UnrecordedPreSchema folds compute: unrecorded{pre_schema} — never $0.
    }
    if total > f.program_cap_micros {
        return LegVerdict::Refused(format!("program_cap:{total}"));
    }
    LegVerdict::Verified
}
