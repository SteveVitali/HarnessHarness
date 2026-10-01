//! `evaluate_expiry(rule, observables, now)` (§5b.3 "Expiry/revalidation
//! contract", ADR-0126; R-2.3.3) — the profile-home instance of the
//! ADR-0197 debt state machine. Every transition is a *ledger fact*: the
//! function is pure over declared observables, and its output is a
//! `StatusTransition[]` the caller appends as
//! `model.profile.status.changed{profile_ref, rule_id?, from, to, trigger,
//! evidence_ref, causes[]}` rows — expiry is never an in-memory-only state.
//!
//! Triggers are **observables, never version strings** (§5b.3):
//!
//! - `model_version_change` ⇐ (i) `served_model ≠ model_ref.provider_model_id`
//!   on a call of the binding, (ii) `capabilities.compatibility_token` ≠ the
//!   token in the latest discovery claim, (iii) fingerprint `DRIFT`, or (iv)
//!   `resolve_profile` finds no listed model beyond `grace_period` — plus a
//!   `CompatibilityRecord{status: drifted{rules[]}}` naming the rule
//!   (ADR-0197 D5/ADR-0203);
//! - `date` ⇐ `now ≥ expiry.params.until` (or the record's `value` bound) —
//!   the *warning* trigger — or `retirement_at` passed, which expires;
//! - `probe_failure` ⇐ a `ConformanceRecord{DRIFT | UNSUPPORTED}` on a
//!   capability in the rule's dependency set
//!   ([`crate::profile_test::capability_dependencies`]);
//! - `evidence_refresh_due` ⇐ `max(age(evidence_refs)) > evidence_max_age`
//!   (the operand comes from `debt.expiry.params.evidence_max_age_ms`);
//! - `experiment_ref` ⇐ the bound `ComparisonReport` reports non-inferiority.
//!
//! The state machine (per rule; the profile's status is the worst of its
//! rules' — [`crate::profile::profile_status`]):
//!
//! ```text
//! active ──any trigger──▶ expiring ──revalidated{evidence_ref}──▶ active
//! active | expiring ──probe_failure | (iv) beyond grace | retirement_at──▶ expired
//! expired ──fresh evidence refresh ∧ probe pass──▶ active
//! expired ──▶ retired   (only via `retire()` — never produced here)
//! ```
//!
//! `evaluate_expiry` is monotone-honest: no transition ⇒ `[]`; it never emits
//! `retired`, and re-evaluation with unchanged observables is idempotent (the
//! `to` state equals `from` produces no row).

use hh_ontology::debt::{DebtStatus, ExpiryKind};
use hh_wire::json::Json;

use crate::profile::ProfileRule;
use crate::profile_test::ConformanceVerdict;

/// `ExpiryObservables` — the closed set of observations
/// [`evaluate_expiry`] consumes. Every member is a measured fact the caller
/// projects from the ledger (discovery claims, conformance records,
/// `CompatibilityRecord`s, evidence ages); an absent observation is `false`/
/// `None`, never a fabricated trigger.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExpiryObservables {
    /// `model_version_change` (i): `served_model ≠ model_ref.provider_model_id`
    /// observed on ≥ 1 call of the binding (the `SnapshotClaim` is the
    /// caller's record).
    pub served_model_mismatch: bool,
    /// `model_version_change` (ii): the latest discovery claim's
    /// `capabilities.compatibility_token` differs from the pinned token
    /// (also a re-lowering trigger — the caller mints the
    /// `model.surface.relowered{reason: compatibility_token_changed}` row).
    pub compatibility_token_changed: bool,
    /// `model_version_change` (iii): a fingerprint `DRIFT` was observed.
    pub fingerprint_drift: bool,
    /// `model_version_change` (iv): `resolve_profile` found no listed model
    /// and `grace_period` elapsed — an *expiring*-skip trigger (straight to
    /// `expired`).
    pub unresolved_beyond_grace: bool,
    /// The selector's `retirement_at` bound passed (or the debt's own
    /// retirement date) — an *expiring*-skip trigger.
    pub retirement_at_passed: bool,
    /// `probe_failure` evidence: `(capability, verdict)` rows projected from
    /// the probe's `ConformanceRecord`s. A `DRIFT`/`UNSUPPORTED` verdict on a
    /// capability in the rule's dependency set expires the rule outright.
    pub probe_records: Vec<(String, ConformanceVerdict)>,
    /// `evidence_refresh_due` operand: `max(age(evidence_refs))` in ms, as
    /// measured by the caller (instrument-produced evidence only — ADR-0197
    /// D5; Tier-A source refs never age out).
    pub max_evidence_age_ms: Option<u64>,
    /// `experiment_ref` operand: `Some(true)` when the bound removal
    /// `ComparisonReport` reports non-inferiority.
    pub experiment_non_inferior: Option<bool>,
    /// A `revalidated{evidence_ref}` observation — the `expiring → active`
    /// leg, and one half of `expired → active` (which also requires a probe
    /// pass).
    pub revalidation: Option<RevalidationObs>,
    /// A `CompatibilityRecord{status: drifted{rules[]}}` regression verdict —
    /// the rule ids the record names (a `model_version_change` observable,
    /// ADR-0203).
    pub regression_drifted_rules: Vec<String>,
}

/// `RevalidationObs{evidence_ref, probe_passed}` — the revalidation
/// observation. `probe_passed` is required only for the `expired → active`
/// leg (a fresh evidence refresh *and* a probe pass).
#[derive(Debug, Clone, PartialEq)]
pub struct RevalidationObs {
    /// The fresh evidence ref the revalidation rides.
    pub evidence_ref: String,
    /// Whether the bound probe suite passed under the new evidence.
    pub probe_passed: bool,
}

/// `StatusTransition{rule_id, from, to, trigger, evidence_ref?, causes[]}` —
/// one row of the `model.profile.status.changed` stream (ADR-0126 d.3;
/// `causes[]` names every observable that fired — nothing silently folded).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusTransition {
    /// The rule this transition is for (`None` = the profile's own `expiry`
    /// record — `evaluate_profile_expiry` emits rule-level rows plus this).
    pub rule_id: Option<String>,
    /// The status observed going in.
    pub from: DebtStatus,
    /// The status the observables produce.
    pub to: DebtStatus,
    /// The trigger spelling — the `ExpiryKind` name or `revalidated`.
    pub trigger: String,
    /// The evidence ref a `revalidated`/`experiment_ref` transition carries.
    pub evidence_ref: Option<String>,
    /// `causes[]` — every observable that fired, spelled closed.
    pub causes: Vec<String>,
}

impl StatusTransition {
    /// The `model.profile.status.changed` payload (§5b.3 event row; the
    /// gateway's `events::profile_status_changed` emits the same shape —
    /// this is the profile-home projection used where the ledger row is
    /// minted by the caller).
    pub fn payload(&self, profile_ref: &str) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("profile_ref".into(), Json::str(profile_ref));
        if let Some(r) = &self.rule_id {
            m.insert("rule_id".into(), Json::str(r.clone()));
        }
        m.insert("from".into(), Json::str(self.from.name()));
        m.insert("to".into(), Json::str(self.to.name()));
        m.insert("trigger".into(), Json::str(self.trigger.clone()));
        if let Some(e) = &self.evidence_ref {
            m.insert("evidence_ref".into(), Json::str(e.clone()));
        }
        m.insert(
            "causes".into(),
            Json::Arr(self.causes.iter().map(|c| Json::str(c.clone())).collect()),
        );
        Json::Obj(m)
    }
}

/// The observable spellings `causes[]` carries (closed).
mod cause {
    /// `served_model ≠ provider_model_id` observed.
    pub const SERVED_MODEL_MISMATCH: &str = "served_model_mismatch";
    /// The discovery claim's `compatibility_token` changed.
    pub const COMPATIBILITY_TOKEN_CHANGED: &str = "compatibility_token_changed";
    /// The fingerprint probe returned `drift`.
    pub const FINGERPRINT_DRIFT: &str = "fingerprint_drift";
    /// No listed model resolved beyond `grace_period`.
    pub const NO_MODEL_BEYOND_GRACE: &str = "no_listed_model_beyond_grace";
    /// A `drifted{rules[]}` compatibility record named the rule.
    pub const REGRESSION_DRIFTED: &str = "regression_drifted";
    /// `now ≥ until` — the date bound's warn leg.
    pub const UNTIL_PASSED: &str = "until_passed";
    /// `retirement_at` passed — the date bound's expire leg.
    pub const RETIREMENT_AT_PASSED: &str = "retirement_at_passed";
    /// A dependency-set capability probed `DRIFT`/`UNSUPPORTED`.
    pub const DEPENDENCY_PROBE_FAILURE: &str = "dependency_probe_failure";
    /// `max(age(evidence_refs)) > evidence_max_age`.
    pub const EVIDENCE_STALE: &str = "evidence_refresh_due";
    /// The bound experiment reported non-inferiority.
    pub const EXPERIMENT_SETTLED: &str = "experiment_non_inferior";
    /// `revalidated{evidence_ref}` (+ probe pass where `expired` requires it).
    pub const REVALIDATED: &str = "revalidated";
}

/// The `expiry_condition.value` operand for a `date`/`evidence_refresh_due`/
/// `experiment_ref` record — `value` decodes as an ms-epoch or an
/// `YYYY-MM-DD[THH:MM:SS[.mmm]Z]` date string (the record's declared operand;
/// `None` = the operand is absent/unparseable — an incomplete record `link`
/// already refuses, so `None` here just means "no bound to fire").
fn operand_ms(record: &crate::profile::ProfileDebtRecord) -> Option<u64> {
    if let Some(e) = &record.expiry {
        if let Some(until) = e.params.until {
            return Some(until);
        }
    }
    record
        .expiry_condition
        .value
        .as_deref()
        .and_then(date_operand_ms)
}

/// `value` → ms epoch: a bare `u64` is an epoch; an `YYYY-MM-DD` date is the
/// day-start epoch; the full `YYYY-MM-DDTHH:MM:SS[.mmm]Z` form parses too.
/// Civil-days arithmetic only — no clock read (CC4: the input is the record).
fn date_operand_ms(s: &str) -> Option<u64> {
    if let Ok(n) = s.parse::<u64>() {
        return Some(n);
    }
    let (date, time) = match s.split_once('T') {
        Some((d, t)) => (d, t.trim_end_matches('Z')),
        None => (s.trim_end_matches('Z'), "00:00:00"),
    };
    let mut dp = date.split('-');
    let (y, mo, d) = (
        dp.next()?.parse::<i64>().ok()?,
        dp.next()?.parse::<i64>().ok()?,
        dp.next()?.parse::<i64>().ok()?,
    );
    if dp.next().is_some() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut tp = time.split(':');
    let (h, mi, se) = (
        tp.next()?.parse::<i64>().ok()?,
        tp.next().and_then(|x| x.parse::<i64>().ok()).unwrap_or(0),
        tp.next()
            .and_then(|x| x.split('.').next().unwrap_or("0").parse::<i64>().ok())
            .unwrap_or(0),
    );
    if h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // Hinnant's civil-from-days (days since 1970-01-01) — fixed calendar, no
    // tz/env reads.
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    if days < 0 {
        return None;
    }
    Some(
        (days as u64) * 86_400_000
            + (h as u64) * 3_600_000
            + (mi as u64) * 60_000
            + (se as u64) * 1_000,
    )
}

/// `evaluate_expiry(rule, observables, now) → StatusTransition[]` (§5b.3).
///
/// The rule's `debt.expiry_condition.kind` gates which observables can fire —
/// a `probe_failure`-kind debt reads the probe records, a `date` debt reads
/// the clock operands; a trigger the kind doesn't name can never fire the
/// rule (a wrong-kind observable is *evidence for another rule*, not this
/// one's expiry).
pub fn evaluate_expiry(
    rule: &ProfileRule,
    obs: &ExpiryObservables,
    now_ms: u64,
) -> Vec<StatusTransition> {
    evaluate_record(&rule.debt, Some(&rule.rule_id), Some(rule), obs, now_ms)
}

/// `evaluate_profile_expiry(profile, observables, now)` — the profile's own
/// `expiry` record plus every rule's (§5b.3 "profile status = worst of its
/// rules'"). The profile-level record evaluates with no rule context (its
/// probe dependency set is empty — a `probe_failure` only reaches it through
/// `probe_failure`-kind debt against the profile's own axes, which are the
/// capability rows the caller names in `probe_records` without a rule
/// intersection).
pub fn evaluate_profile_expiry(
    profile: &crate::profile::ModelProfile,
    obs: &ExpiryObservables,
    now_ms: u64,
) -> Vec<StatusTransition> {
    let mut out = evaluate_record(&profile.expiry, None, None, obs, now_ms);
    for rule in &profile.rules {
        out.extend(evaluate_expiry(rule, obs, now_ms));
    }
    out
}

/// `evaluate_debt_record(record, rule_id, rule, observables, now)` — the
/// record-level form of [`evaluate_expiry`], exported for the non-profile
/// debt homes (R-2.9.6¹'s `evaluate_debt` merges these transitions with the
/// manager-level trigger families; §5h.6 §2). `rule` supplies the
/// dependency set a `probe_failure` intersects (`None` = a rule-free
/// record, where every named capability counts as in-scope).
pub fn evaluate_debt_record(
    debt: &crate::profile::ProfileDebtRecord,
    rule_id: Option<&str>,
    rule: Option<&ProfileRule>,
    obs: &ExpiryObservables,
    now_ms: u64,
) -> Vec<StatusTransition> {
    evaluate_record(debt, rule_id, rule, obs, now_ms)
}

/// The shared evaluator. `rule` supplies the dependency set a `probe_failure`
/// intersects (`None` for a rule-free record — the profile's own `expiry` —
/// where every named capability counts as in-scope).
fn evaluate_record(
    debt: &crate::profile::ProfileDebtRecord,
    rule_id: Option<&str>,
    rule: Option<&ProfileRule>,
    obs: &ExpiryObservables,
    now_ms: u64,
) -> Vec<StatusTransition> {
    let from = debt.status;
    // `retired` is terminal here — only `retire()` (a human-sealed
    // supersession, ADR-0126 P4) moves a record out of `expired`.
    if from == DebtStatus::Retired {
        return Vec::new();
    }

    // ── Revalidation legs (ADR-0126): they apply regardless of the record's
    // expiry kind — `revalidated{evidence_ref}` is the recorded re-proof. ────
    if let Some(rev) = &obs.revalidation {
        match from {
            DebtStatus::Expiring => {
                return vec![StatusTransition {
                    rule_id: rule_id.map(str::to_string),
                    from,
                    to: DebtStatus::Active,
                    trigger: cause::REVALIDATED.to_string(),
                    evidence_ref: Some(rev.evidence_ref.clone()),
                    causes: vec![cause::REVALIDATED.to_string()],
                }]
            }
            DebtStatus::Expired => {
                if rev.probe_passed {
                    // `expired → active` only on a fresh evidence refresh
                    // *and* a probe pass (§5b.3).
                    return vec![StatusTransition {
                        rule_id: rule_id.map(str::to_string),
                        from,
                        to: DebtStatus::Active,
                        trigger: cause::REVALIDATED.to_string(),
                        evidence_ref: Some(rev.evidence_ref.clone()),
                        causes: vec![cause::REVALIDATED.to_string(), "probe_passed".to_string()],
                    }];
                }
                return Vec::new(); // evidence without a probe pass never reopens
            }
            _ => {}
        }
    }

    // ── Fire the kind's triggers. ────────────────────────────────────────────
    let mut warn: Vec<String> = Vec::new(); // → expiring
    let mut hard: Vec<String> = Vec::new(); // → expired
    match debt.expiry_condition.kind {
        ExpiryKind::ModelVersionChange => {
            if obs.served_model_mismatch {
                warn.push(cause::SERVED_MODEL_MISMATCH.into());
            }
            if obs.compatibility_token_changed {
                warn.push(cause::COMPATIBILITY_TOKEN_CHANGED.into());
            }
            if obs.fingerprint_drift {
                warn.push(cause::FINGERPRINT_DRIFT.into());
            }
            if rule_id.is_some_and(|id| obs.regression_drifted_rules.iter().any(|r| r == id)) {
                warn.push(cause::REGRESSION_DRIFTED.into());
            }
            // (iv) — no listed model beyond `grace_period`: a hard trigger.
            if obs.unresolved_beyond_grace {
                hard.push(cause::NO_MODEL_BEYOND_GRACE.into());
            }
        }
        ExpiryKind::Date => {
            if obs.retirement_at_passed {
                hard.push(cause::RETIREMENT_AT_PASSED.into());
            }
            if let Some(until) = operand_ms(debt) {
                if now_ms >= until {
                    warn.push(cause::UNTIL_PASSED.into());
                }
            }
        }
        ExpiryKind::ProbeFailure => {
            // A `DRIFT`/`UNSUPPORTED` record on a capability in the rule's
            // dependency set is a *hard* trigger (§5b.3: "expired on
            // dependency probe_failure").
            let deps: &[&'static str] = match rule {
                Some(r) => crate::profile_test::capability_dependencies(r.kind),
                None => &[],
            };
            let failed = obs.probe_records.iter().any(|(cap, verdict)| {
                matches!(
                    verdict,
                    ConformanceVerdict::Drift | ConformanceVerdict::Unsupported
                ) && (rule.is_none() || deps.contains(&cap.as_str()))
            });
            if failed {
                hard.push(cause::DEPENDENCY_PROBE_FAILURE.into());
            }
        }
        ExpiryKind::EvidenceRefreshDue => {
            let bound = debt
                .expiry
                .as_ref()
                .and_then(|e| e.params.evidence_max_age_ms)
                .or_else(|| operand_ms(debt));
            if let (Some(max_age), Some(age)) = (bound, obs.max_evidence_age_ms) {
                if age > max_age {
                    warn.push(cause::EVIDENCE_STALE.into());
                }
            }
        }
        ExpiryKind::ExperimentRef => {
            if obs.experiment_non_inferior == Some(true) {
                warn.push(cause::EXPERIMENT_SETTLED.into());
            }
        }
    }
    if hard.is_empty() && warn.is_empty() {
        return Vec::new();
    }
    let to = if !hard.is_empty() {
        DebtStatus::Expired
    } else {
        DebtStatus::Expiring
    };
    let mut causes = hard;
    causes.append(&mut warn);
    causes.dedup();
    let to = match (from, to) {
        // An already-`expired` record stays `expired` — triggers re-fire as
        // observability, never a second transition row.
        (DebtStatus::Expired, _) => return Vec::new(),
        // `expiring` still moves to `expired` on a hard trigger.
        (f, t) if f == t => return Vec::new(),
        (_, t) => t,
    };
    vec![StatusTransition {
        rule_id: rule_id.map(str::to_string),
        from,
        to,
        trigger: debt.expiry_condition.kind.name().to_string(),
        evidence_ref: None,
        causes,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::*;

    fn rule_with_expiry(kind: ExpiryKind, value: Option<&str>) -> ProfileRule {
        ProfileRule {
            rule_id: "r1".to_string(),
            kind: ProfileRuleKind::Naming,
            owned_fields: vec![],
            params: Json::Null,
            debt: ProfileDebtRecord {
                rule_id: "r1".to_string(),
                hypothesis: "h".to_string(),
                evidence_refs: vec![EvidenceRef::legacy("e/1")],
                owner: "o".to_string(),
                reach_via: vec![],
                expiry_condition: ExpiryCondition {
                    kind,
                    value: value.map(str::to_string),
                },
                removal_test_ref: "t/1".to_string(),
                removal_test: None,
                status: DebtStatus::Active,
                debt_class: None,
                hypothesis_typed: None,
                scope: None,
                expiry: None,
                runway_ms: None,
                revalidation: None,
                created_at: None,
                supersedes: None,
            },
            scope: None,
            supersedes: None,
            compliance: Compliance {
                detector_class: ComplianceDetector::Deterministic,
                followed_predicate_ref: None,
            },
        }
    }

    #[test]
    fn no_observables_no_transition() {
        let r = rule_with_expiry(ExpiryKind::ModelVersionChange, None);
        assert!(evaluate_expiry(&r, &ExpiryObservables::default(), 0).is_empty());
    }

    #[test]
    fn model_version_change_soft_leg_goes_expiring() {
        let r = rule_with_expiry(ExpiryKind::ModelVersionChange, None);
        let obs = ExpiryObservables {
            served_model_mismatch: true,
            fingerprint_drift: true,
            ..Default::default()
        };
        let ts = evaluate_expiry(&r, &obs, 100);
        assert_eq!(ts.len(), 1);
        let t = &ts[0];
        assert_eq!(t.from, DebtStatus::Active);
        assert_eq!(t.to, DebtStatus::Expiring);
        assert_eq!(t.trigger, "model_version_change");
        assert!(t.causes.contains(&"served_model_mismatch".to_string()));
        assert!(t.causes.contains(&"fingerprint_drift".to_string()));
    }

    #[test]
    fn unresolved_beyond_grace_expires() {
        let r = rule_with_expiry(ExpiryKind::ModelVersionChange, None);
        let obs = ExpiryObservables {
            unresolved_beyond_grace: true,
            ..Default::default()
        };
        let t = &evaluate_expiry(&r, &obs, 0)[0];
        assert_eq!(t.to, DebtStatus::Expired);
    }

    #[test]
    fn probe_failure_on_dependency_expires() {
        // `naming` depends on `native_function_calling`.
        let r = rule_with_expiry(ExpiryKind::ProbeFailure, None);
        let obs = ExpiryObservables {
            probe_records: vec![(
                "native_function_calling".to_string(),
                ConformanceVerdict::Drift,
            )],
            ..Default::default()
        };
        let t = &evaluate_expiry(&r, &obs, 0)[0];
        assert_eq!(t.to, DebtStatus::Expired);
        assert_eq!(t.trigger, "probe_failure");
        // An unrelated capability's DRIFT never fires this rule.
        let obs2 = ExpiryObservables {
            probe_records: vec![("image_input".to_string(), ConformanceVerdict::Drift)],
            ..Default::default()
        };
        assert!(evaluate_expiry(&r, &obs2, 0).is_empty());
    }

    #[test]
    fn date_until_warns_retirement_expires() {
        let mut r = rule_with_expiry(ExpiryKind::Date, Some("1000"));
        let obs = ExpiryObservables::default();
        let t = &evaluate_expiry(&r, &obs, 1_000)[0];
        assert_eq!(t.to, DebtStatus::Expiring);
        // `retirement_at` passes the warn leg — `expired`.
        let obs2 = ExpiryObservables {
            retirement_at_passed: true,
            ..Default::default()
        };
        let t2 = &evaluate_expiry(&r, &obs2, 0)[0];
        assert_eq!(t2.to, DebtStatus::Expired);
        // Once expiring, a hard trigger still expires.
        r.debt.status = DebtStatus::Expiring;
        let t3 = &evaluate_expiry(&r, &obs2, 0)[0];
        assert_eq!(t3.from, DebtStatus::Expiring);
        assert_eq!(t3.to, DebtStatus::Expired);
    }

    #[test]
    fn evidence_refresh_due_warns() {
        let mut r = rule_with_expiry(ExpiryKind::EvidenceRefreshDue, Some("500"));
        r.debt.expiry = Some(hh_ontology::debt::DebtExpiry {
            condition: ExpiryKind::EvidenceRefreshDue,
            params: hh_ontology::debt::ExpiryParams {
                until: None,
                evidence_max_age_ms: Some(500),
                dependency_capabilities: vec![],
                design_ref: None,
            },
        });
        let obs = ExpiryObservables {
            max_evidence_age_ms: Some(600),
            ..Default::default()
        };
        let t = &evaluate_expiry(&r, &obs, 0)[0];
        assert_eq!(t.to, DebtStatus::Expiring);
        assert_eq!(t.causes, vec!["evidence_refresh_due".to_string()]);
        let obs2 = ExpiryObservables {
            max_evidence_age_ms: Some(499),
            ..Default::default()
        };
        assert!(evaluate_expiry(&r, &obs2, 0).is_empty());
    }

    #[test]
    fn experiment_ref_settles() {
        let r = rule_with_expiry(ExpiryKind::ExperimentRef, Some("design/1"));
        let obs = ExpiryObservables {
            experiment_non_inferior: Some(true),
            ..Default::default()
        };
        assert_eq!(evaluate_expiry(&r, &obs, 0)[0].to, DebtStatus::Expiring);
    }

    #[test]
    fn revalidation_legs() {
        let mut r = rule_with_expiry(ExpiryKind::ModelVersionChange, None);
        r.debt.status = DebtStatus::Expiring;
        let obs = ExpiryObservables {
            revalidation: Some(RevalidationObs {
                evidence_ref: "ev/2".to_string(),
                probe_passed: false,
            }),
            ..Default::default()
        };
        let t = &evaluate_expiry(&r, &obs, 0)[0];
        assert_eq!(t.to, DebtStatus::Active);
        assert_eq!(t.trigger, "revalidated");
        assert_eq!(t.evidence_ref.as_deref(), Some("ev/2"));
        // `expired → active` needs evidence *and* a probe pass.
        r.debt.status = DebtStatus::Expired;
        assert!(evaluate_expiry(&r, &obs, 0).is_empty());
        let obs2 = ExpiryObservables {
            revalidation: Some(RevalidationObs {
                evidence_ref: "ev/3".to_string(),
                probe_passed: true,
            }),
            ..Default::default()
        };
        let t2 = &evaluate_expiry(&r, &obs2, 0)[0];
        assert_eq!(t2.to, DebtStatus::Active);
        assert!(t2.causes.contains(&"probe_passed".to_string()));
    }

    #[test]
    fn expired_and_retired_never_transition_here() {
        let mut r = rule_with_expiry(ExpiryKind::ModelVersionChange, None);
        r.debt.status = DebtStatus::Expired;
        let obs = ExpiryObservables {
            fingerprint_drift: true,
            ..Default::default()
        };
        assert!(evaluate_expiry(&r, &obs, 0).is_empty());
        r.debt.status = DebtStatus::Retired;
        let obs2 = ExpiryObservables {
            unresolved_beyond_grace: true,
            ..Default::default()
        };
        assert!(evaluate_expiry(&r, &obs2, 0).is_empty());
    }

    #[test]
    fn transition_payload_shape() {
        let t = StatusTransition {
            rule_id: Some("r1".to_string()),
            from: DebtStatus::Active,
            to: DebtStatus::Expiring,
            trigger: "model_version_change".to_string(),
            evidence_ref: None,
            causes: vec!["fingerprint_drift".to_string()],
        };
        let p = t.payload("prof/x");
        assert_eq!(p.get("profile_ref").and_then(Json::as_str), Some("prof/x"));
        assert_eq!(p.get("rule_id").and_then(Json::as_str), Some("r1"));
        assert_eq!(p.get("from").and_then(Json::as_str), Some("active"));
        assert_eq!(p.get("to").and_then(Json::as_str), Some("expiring"));
        assert_eq!(
            p.get("trigger").and_then(Json::as_str),
            Some("model_version_change")
        );
        let Json::Arr(causes) = p.get("causes").unwrap() else {
            panic!("causes must be an array");
        };
        assert_eq!(causes.len(), 1);
    }

    #[test]
    fn date_operand_parses_epoch_and_civil_date() {
        assert_eq!(date_operand_ms("1000"), Some(1000));
        assert_eq!(date_operand_ms("1970-01-02"), Some(86_400_000));
        assert_eq!(date_operand_ms("1970-01-01T00:00:01Z"), Some(1_000));
        assert_eq!(date_operand_ms("not-a-date"), None);
        assert_eq!(date_operand_ms("1969-12-31"), None); // pre-epoch ⇒ None
    }
}
