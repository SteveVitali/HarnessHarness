//! The compliance-chain metrics (spec §5h.2 §2.3 + §10; ADR-0045 D4; S3.3).
//!
//! The chain `delivered → activated → followed` is measured per artefact over
//! the ledger facts — rates in **ppm** (`0..=1_000_000`), never floats. A
//! stage with no detector declared returns `n/a{no_detector}` at the
//! *declaration* level (catalogue); here the rates are plain counts — `0/0`
//! yields `None` (the cell renders `n/a{estimator_undefined}`, never 0).
//!
//! - `artefact_delivery_rate` — `|delivered ∩ planned| / |planned|` (the plan
//!   is `context.assembled` artefact items; when no plan exists the delivered
//!   set *is* the denominator — the run's own record).
//! - `artefact_activation_rate` — `|activated| / |delivered|`.
//! - `artefact_follow_rate` — `|followed| / |activated|` (followed ⊆ activated
//!   by the chain's own events; a `followed` with no `activated` is an
//!   accounting anomaly the `audit_completeness` veto owns — the rate still
//!   counts it in the numerator only if activated, keeping `≤ 1`).
//! - `detector_conformity` — decided validator verdicts whose detector is in
//!   the declared set / decided verdicts (the caller supplies the declaration;
//!   a verdict with no declared detector counts nonconforming).
//! - `attribution_completeness` — `|attributed| / |completed model calls|`
//!   (the veto predicate trips on any gap; the metric reports the rate).
//! - `intervention_rate` — `|denied effects| / |effects intended|` (the
//!   reference-monitor intervention share).

use std::collections::BTreeSet;

use hh_budget::attribution::ChargedTo;
use hh_ontology::compliance::{Detector, NaReason};
use hh_ontology::eval::MetricValueKind;
use hh_ontology::participant::ParticipantClass;

use crate::facts::LedgerFacts;
use crate::stats::PPM;

/// The compliance metric names (catalogue rows — [`crate::catalogue`]).
pub mod names {
    /// `artefact_delivery_rate`.
    pub const ARTEFACT_DELIVERY_RATE: &str = "artefact_delivery_rate";
    /// `artefact_activation_rate`.
    pub const ARTEFACT_ACTIVATION_RATE: &str = "artefact_activation_rate";
    /// `artefact_follow_rate`.
    pub const ARTEFACT_FOLLOW_RATE: &str = "artefact_follow_rate";
    /// `detector_conformity`.
    pub const DETECTOR_CONFORMITY: &str = "detector_conformity";
    /// `attribution_completeness`.
    pub const ATTRIBUTION_COMPLETENESS: &str = "attribution_completeness";
    /// `intervention_rate`.
    pub const INTERVENTION_RATE: &str = "intervention_rate";
    /// `opacity_dynamic`.
    pub const OPACITY_DYNAMIC: &str = "opacity_dynamic";
    /// `profile.rule.followed_rate` (per-rule — keyed on the rule id;
    /// AC-R-2.3.3-6).
    pub const PROFILE_RULE_FOLLOWED_RATE: &str = "profile.rule.followed_rate";
}

/// A `(numerator, denominator)` pair — the raw material every rate reports
/// beside its ppm value (nothing is ever a bare float).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateParts {
    /// The numerator.
    pub num: u64,
    /// The denominator.
    pub den: u64,
}

impl RateParts {
    /// The ppm rate; `None` on a zero denominator (`n/a`, never 0).
    pub fn ppm(self) -> Option<i64> {
        if self.den == 0 {
            return None;
        }
        Some((self.num as u128 * PPM as u128 / self.den as u128) as i64)
    }
}

fn ids<'a>(rows: impl Iterator<Item = &'a crate::facts::ArtefactRow>) -> BTreeSet<&'a str> {
    rows.map(|a| a.artefact_id.as_str()).collect()
}

/// `artefact_delivery_rate` — `|delivered ∩ planned| / |planned artefact
/// items|` where the plan is `context.assembled` items naming an artefact;
/// falls back to the delivered set when no plan was projected (a run without
/// `context.assembled` evidence reports its own deliveries as the
/// denominator — `0/0` is `n/a`, never 0).
pub fn delivery_rate(f: &LedgerFacts) -> RateParts {
    let planned: BTreeSet<&str> = f
        .assembled
        .iter()
        .flat_map(|(_, items)| items.iter())
        .filter_map(|i| i.artefact_id.as_deref())
        .collect();
    let delivered = ids(f.artefacts_delivered.iter());
    if planned.is_empty() {
        return RateParts {
            num: delivered.len() as u64,
            den: delivered.len() as u64,
        };
    }
    let num = planned.iter().filter(|p| delivered.contains(*p)).count();
    RateParts {
        num: num as u64,
        den: planned.len() as u64,
    }
}

/// `artefact_activation_rate` — `|activated| / |delivered|`.
pub fn activation_rate(f: &LedgerFacts) -> RateParts {
    let delivered = ids(f.artefacts_delivered.iter());
    let activated = ids(f.artefacts_activated.iter());
    RateParts {
        num: activated.iter().filter(|a| delivered.contains(*a)).count() as u64,
        den: delivered.len() as u64,
    }
}

/// `artefact_follow_rate` — `|followed ∩ activated| / |activated|`.
pub fn follow_rate(f: &LedgerFacts) -> RateParts {
    let activated = ids(f.artefacts_activated.iter());
    let followed = ids(f.artefacts_followed.iter());
    RateParts {
        num: followed.iter().filter(|x| activated.contains(*x)).count() as u64,
        den: activated.len() as u64,
    }
}

/// `detector_conformity` — decided verdicts produced by a declared detector.
pub fn detector_conformity(f: &LedgerFacts, declared: &BTreeSet<String>) -> RateParts {
    let decided: Vec<&crate::facts::VerdictRow> = f
        .verdicts
        .iter()
        .filter(|v| v.status == "decided")
        .collect();
    RateParts {
        num: decided
            .iter()
            .filter(|v| {
                v.detector
                    .as_ref()
                    .map(|d| declared.contains(d))
                    .unwrap_or(false)
            })
            .count() as u64,
        den: decided.len() as u64,
    }
}

/// `attribution_completeness` — `|attributed| / |completed model calls|`.
pub fn attribution_completeness(f: &LedgerFacts) -> RateParts {
    RateParts {
        num: f
            .model_calls_completed
            .iter()
            .filter(|c| f.cost_attributed_calls.contains(*c))
            .count() as u64,
        den: f.model_calls_completed.len() as u64,
    }
}

/// `intervention_rate` — `|denied| / |intended|`.
pub fn intervention_rate(f: &LedgerFacts) -> RateParts {
    RateParts {
        num: f
            .effects_intended
            .iter()
            .filter(|e| f.permission_denials.contains(&e.effect_id))
            .count() as u64,
        den: f.effects_intended.len() as u64,
    }
}

/// One declared profile-rule detector: the per-rule denominator's
/// `n/a{no_detector}`/`n/a{class}` disposition (AC-R-2.3.3-6). Mirrors the
/// compiler's `ComplianceDetector` at the projection boundary — hh-eval does
/// not depend on hh-compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDetector {
    /// The rule the detector follows (the join key).
    pub rule_id: String,
    /// The detector class (`None` when the rule declares no detector — the
    /// rate renders `n/a{no_detector}`).
    pub detector: Option<Detector>,
    /// The participant class the detector runs under (`Hosted` rows render
    /// `n/a{class}` — `ParticipantClass::hosted`-gated observability).
    pub participant_class: Option<ParticipantClass>,
    /// The predicate the followed verdict evaluates (the artefact↔rule join
    /// on `context.artefact.followed` rows).
    pub followed_predicate_ref: String,
    /// The judge calibration record ref; required for a `Judged` detector —
    /// an uncalibrated judge renders `n/a{no_detector}`.
    pub calibration_ref: Option<String>,
}

/// The per-rule rate row: the value plus the charge/attribution the spec
/// attaches to `profile.rule.followed_rate` (instrument charge; a `Judged`
/// detector additionally records its calibration ref).
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRuleRate {
    /// The rule id.
    pub rule_id: String,
    /// The metric value — `Int(ppm)` or `na{...}`, never a bare 0.
    pub value: MetricValueKind,
    /// The declared detector class that produced the rate (`None` under
    /// `n/a{no_detector}`).
    pub detector: Option<Detector>,
    /// The charge class — `Instrument` whenever a rate is produced
    /// (`profile.rule.followed_rate` is `charged_to = instrument`).
    pub charged_to: Option<ChargedTo>,
    /// The judge calibration ref (echoed for `Judged` rates).
    pub calibration_ref: Option<String>,
    /// The raw `(followed, delivered)` pair the ppm came from.
    pub parts: RateParts,
}

/// `profile.rule.followed_rate` — the per-rule compliance rate over the
/// `delivered → activated → verification.artefact.followed` chain:
/// `|delivered ∩ activated ∩ followed| / |delivered|` keyed on `rule_id`
/// (`followed_predicate_ref` joins the followed rows). `n/a{no_detector}`
/// for rules without a detector (or an uncalibrated judge);
/// `n/a{class}` when the declared participant class is `Hosted`;
/// `n/a{estimator_undefined}` on a 0/0 denominator — never 0.
pub fn profile_rule_followed_rates(
    f: &LedgerFacts,
    detectors: &[RuleDetector],
) -> Vec<ProfileRuleRate> {
    detectors
        .iter()
        .map(|d| {
            let na = |reason: NaReason| ProfileRuleRate {
                rule_id: d.rule_id.clone(),
                value: MetricValueKind::Na(reason),
                detector: d.detector,
                charged_to: None,
                calibration_ref: d.calibration_ref.clone(),
                parts: RateParts { num: 0, den: 0 },
            };
            let Some(detector) = d.detector else {
                return na(NaReason::NoDetector);
            };
            if detector == Detector::Judged && d.calibration_ref.is_none() {
                return na(NaReason::NoDetector);
            }
            if d.participant_class == Some(ParticipantClass::Hosted) {
                return na(NaReason::Class);
            }
            let in_rule = |a: &crate::facts::ArtefactRow| {
                a.rule_id.as_deref() == Some(d.rule_id.as_str())
                    || a.predicate_ref.as_deref() == Some(d.followed_predicate_ref.as_str())
            };
            let delivered: BTreeSet<&str> = f
                .artefacts_delivered
                .iter()
                .filter(|a| in_rule(a))
                .map(|a| a.artefact_id.as_str())
                .collect();
            let activated: BTreeSet<&str> = f
                .artefacts_activated
                .iter()
                .map(|a| a.artefact_id.as_str())
                .collect();
            let followed: BTreeSet<&str> = f
                .artefacts_followed
                .iter()
                .filter(|a| in_rule(a))
                .map(|a| a.artefact_id.as_str())
                .collect();
            let parts = RateParts {
                num: delivered
                    .iter()
                    .filter(|id| activated.contains(*id) && followed.contains(*id))
                    .count() as u64,
                den: delivered.len() as u64,
            };
            let value = match parts.ppm() {
                Some(ppm) => MetricValueKind::Decimal(ppm),
                None => MetricValueKind::Na(NaReason::EstimatorUndefined),
            };
            ProfileRuleRate {
                rule_id: d.rule_id.clone(),
                value,
                detector: Some(detector),
                charged_to: Some(ChargedTo::Instrument),
                calibration_ref: d.calibration_ref.clone(),
                parts,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::ArtefactRow;

    fn art(id: &str) -> ArtefactRow {
        ArtefactRow {
            artefact_id: id.into(),
            delivery_id: None,
            detector: None,
            rule_id: None,
            predicate_ref: None,
        }
    }

    fn ruled_art(id: &str, rule: &str) -> ArtefactRow {
        ArtefactRow {
            rule_id: Some(rule.into()),
            ..art(id)
        }
    }

    #[test]
    fn rates_are_exact_and_never_zero_fill() {
        let f = LedgerFacts {
            artefacts_delivered: vec![art("a"), art("b")],
            artefacts_activated: vec![art("a")],
            artefacts_followed: vec![art("a")],
            ..LedgerFacts::default()
        };
        assert_eq!(activation_rate(&f).ppm(), Some(500_000));
        assert_eq!(follow_rate(&f).ppm(), Some(1_000_000));
        // 0/0 → None (n/a, never 0).
        let empty = LedgerFacts::default();
        assert_eq!(activation_rate(&empty).ppm(), None);
        assert_eq!(delivery_rate(&empty).ppm(), None);
    }

    #[test]
    fn attribution_and_intervention() {
        let mut f = LedgerFacts {
            model_calls_completed: ["c1".into(), "c2".into()].into_iter().collect(),
            cost_attributed_calls: ["c1".into()].into_iter().collect(),
            ..LedgerFacts::default()
        };
        assert_eq!(attribution_completeness(&f).ppm(), Some(500_000));
        f.effects_intended = vec![
            crate::facts::EffectRow {
                effect_id: "e1".into(),
                idempotency_key: None,
                compensates: None,
                reverts: None,
            },
            crate::facts::EffectRow {
                effect_id: "e2".into(),
                idempotency_key: None,
                compensates: None,
                reverts: None,
            },
        ];
        f.permission_denials = ["e2".to_string()].into_iter().collect();
        assert_eq!(intervention_rate(&f).ppm(), Some(500_000));
    }

    #[test]
    fn profile_rule_followed_rate_semantics() {
        let f = LedgerFacts {
            artefacts_delivered: vec![
                ruled_art("a1", "R1"),
                ruled_art("a2", "R1"),
                ruled_art("b1", "R2"),
                art("unruled"),
            ],
            artefacts_activated: vec![ruled_art("a1", "R1"), ruled_art("a2", "R1")],
            artefacts_followed: vec![ruled_art("a1", "R1")],
            ..LedgerFacts::default()
        };
        let detectors = vec![
            RuleDetector {
                rule_id: "R1".into(),
                detector: Some(Detector::Deterministic),
                participant_class: None,
                followed_predicate_ref: "P".into(),
                calibration_ref: None,
            },
            RuleDetector {
                rule_id: "R2".into(),
                detector: Some(Detector::Deterministic),
                participant_class: Some(ParticipantClass::Hosted),
                followed_predicate_ref: "P".into(),
                calibration_ref: None,
            },
            RuleDetector {
                rule_id: "R3".into(),
                detector: None,
                participant_class: None,
                followed_predicate_ref: "P".into(),
                calibration_ref: None,
            },
            RuleDetector {
                rule_id: "R4".into(),
                detector: Some(Detector::Judged),
                participant_class: None,
                followed_predicate_ref: "P".into(),
                calibration_ref: None,
            },
        ];
        let rates = profile_rule_followed_rates(&f, &detectors);
        // R1: 1 of 2 delivered followed → 500_000 ppm, instrument charge.
        assert_eq!(rates[0].value, MetricValueKind::Decimal(500_000));
        assert_eq!(rates[0].charged_to, Some(ChargedTo::Instrument));
        assert_eq!(rates[0].parts, RateParts { num: 1, den: 2 });
        // R2: hosted class → n/a{class}.
        assert_eq!(rates[1].value, MetricValueKind::Na(NaReason::Class));
        // R3: no detector → n/a{no_detector}.
        assert_eq!(rates[2].value, MetricValueKind::Na(NaReason::NoDetector));
        // R4: uncalibrated judge → n/a{no_detector}.
        assert_eq!(rates[3].value, MetricValueKind::Na(NaReason::NoDetector));
        // Predicate-ref join: a followed row naming only the predicate counts.
        let f2 = LedgerFacts {
            artefacts_delivered: vec![ruled_art("a1", "R1")],
            artefacts_activated: vec![art("a1")],
            artefacts_followed: vec![ArtefactRow {
                predicate_ref: Some("P".into()),
                ..art("a1")
            }],
            ..LedgerFacts::default()
        };
        let rates2 = profile_rule_followed_rates(&f2, &detectors[..1]);
        assert_eq!(rates2[0].value, MetricValueKind::Decimal(1_000_000));
    }
}
