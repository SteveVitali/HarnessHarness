//! The process-metric catalogue (§5h.1 §3 "MetricDeclaration catalogue rows" +
//! §8's registered names; ADR-0044 D5): every row is a declaration — `name`,
//! `requires_observability`, `applies_to_classes`, `computed_from` (event
//! classes), `unit`, and the fold status. The catalogue enters R-2.9.2's
//! registry as data; `metric_view` computes the Stage-1-computable rows and
//! renders a typed `n/a{reason}` for the rest (T-LCD-15 — never 0, never a
//! proxy).
//!
//! [`registry_check`] is the metric-registry half of AC-R-2.2.1-5
//! (DF-S1.5-2): it resolves each row's `requires_observability` against the
//! declared `min_observability` of every registered class the metric reads —
//! a metric computed from `model_io` rows must declare `model_io`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ledger::manifest::ObservabilityLevel;
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::participant::{Observability, ParticipantClass};

use crate::errors::TelemetryError;

/// `ledger`'s `ObservabilityLevel` ↔ the ontology's `Observability` — the same
/// four spellings, two homes (manifest vocabulary vs participant-descriptor
/// vocabulary). One mapping, never two.
pub fn to_ontology(level: ObservabilityLevel) -> Observability {
    match level {
        ObservabilityLevel::Events => Observability::Events,
        ObservabilityLevel::ModelIo => Observability::ModelIo,
        ObservabilityLevel::EndState => Observability::EndState,
        ObservabilityLevel::Ledger => Observability::Ledger,
    }
}

/// The unit a metric reports in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    /// A count.
    Count,
    /// Integer milliseconds.
    Milliseconds,
    /// `Money` micro-units (per-currency map — never summed across).
    MicroUnits,
    /// Parts-per-million of 1.0.
    Ppm,
    /// A token vector map.
    Tokens,
    /// A `{tag: count}` map.
    CountMap,
    /// A `Money`/attribution structure.
    Attribution,
}

impl MetricUnit {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricUnit::Count => "count",
            MetricUnit::Milliseconds => "ms",
            MetricUnit::MicroUnits => "micro_units",
            MetricUnit::Ppm => "ppm",
            MetricUnit::Tokens => "tokens",
            MetricUnit::CountMap => "count_map",
            MetricUnit::Attribution => "attribution",
        }
    }
}

/// The fold status — what `metric_view` does with the row at Stage 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldStatus {
    /// Computable over the registered classes now.
    Computable,
    /// A typed `n/a{reason}` — the classes/estimator/stage the row needs are
    /// not live (never 0, never a proxy).
    NotComputed(hh_ontology::compliance::NaReason),
}

/// One catalogue row — the §5h.1 §3 `MetricDeclaration` shape plus the
/// `computed_from` class list and fold status.
pub struct ProcessMetric {
    /// The metric name (§8's registered spellings).
    pub name: &'static str,
    /// The observability the metric requires (`requires_observability`).
    pub requires_observability: &'static [Observability],
    /// The participant classes the metric applies to.
    pub applies_to: &'static [ParticipantClass],
    /// The event classes the metric reads (`computed_from`).
    pub computed_from: &'static [&'static str],
    /// The unit.
    pub unit: MetricUnit,
    /// The Stage-1 fold status.
    pub fold: FoldStatus,
}

impl ProcessMetric {
    /// The §2.6.3 `MetricDeclaration` form (the registry record shape).
    pub fn declaration(&self) -> MetricDeclaration {
        MetricDeclaration {
            name: self.name.to_string(),
            applies_to_classes: self.applies_to.iter().copied().collect(),
            requires_observability: self.requires_observability.iter().copied().collect(),
            detector_classes_allowed: [hh_ontology::compliance::Detector::Deterministic]
                .into_iter()
                .collect(),
        }
    }
}

use hh_ontology::compliance::NaReason as NA;
use FoldStatus::{Computable as C, NotComputed as N};
use Observability::{Events as EV, Ledger as LG, ModelIo as IO};
use ParticipantClass::{Hosted as H, Native as N_};

const BOTH: &[ParticipantClass] = &[N_, H];

/// The catalogue — §5h.1 §8's process-metric names as declared rows
/// (efficiency, reliability, grounding/compliance, autonomy, security,
/// evolvability, budget discipline; thresholds and targets are out of scope).
#[rustfmt::skip]
pub const PROCESS_METRICS: &[ProcessMetric] = &[
    // ── efficiency ──────────────────────────────────────────────────────
    ProcessMetric { name: "tokens_total", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Tokens, fold: C },
    ProcessMetric { name: "tokens_by_bucket", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Tokens, fold: C },
    ProcessMetric { name: "cost_money", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.cost.attributed"], unit: MetricUnit::MicroUnits, fold: C },
    ProcessMetric { name: "cache_hit_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "latency_e2e_ms", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.finished"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "ttft_ms", requires_observability: &[IO], applies_to: BOTH,
        computed_from: &["model.call.attempt.completed"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "ttfm_ms", requires_observability: &[IO], applies_to: BOTH,
        computed_from: &["model.call.attempt.completed"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "turn_phase_profile", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.turn.started", "lifecycle.turn.finished"],
        unit: MetricUnit::Attribution, fold: N(NA::NotRun) }, // the profile is Stage 2 (§9)
    ProcessMetric { name: "context_tokens_per_call", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.assembled", "model.call.requested"],
        unit: MetricUnit::Tokens, fold: N(NA::Capability) }, // `context.assembled` is §05c's
    ProcessMetric { name: "context_growth_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.assembled"], unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "compaction_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.compaction.completed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "compaction_token_reduction", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.compaction.started", "context.compaction.completed"],
        unit: MetricUnit::Tokens, fold: N(NA::Capability) }, // tokens_before/after fields are §05c's
    ProcessMetric { name: "harness_overhead_share", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.cost.attributed"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "model_calls", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.requested"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "tool_calls", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.tool.proposed"], unit: MetricUnit::Count, fold: C },
    // ── reliability ─────────────────────────────────────────────────────
    ProcessMetric { name: "tool_error_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.tool.completed", "action.tool.rejected", "action.tool.surface_rejected"],
        unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "invalid_action_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.failed"], unit: MetricUnit::Ppm,
        fold: N(NA::Capability) }, // the closed error.class sum is §05d.5's
    ProcessMetric { name: "retries", requires_observability: &[EV, IO], applies_to: BOTH,
        computed_from: &["control.retry.fired", "model.call.attempt.started"],
        unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "duplicate_effect_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.committed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "abandoned_effect_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.abandoned"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "unknown_effect_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.unknown"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "crash_recovery_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.resumed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "resume_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.resumed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "fenced_writer_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.lease.fenced"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "loop_detection", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.turn.started"], unit: MetricUnit::Count,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "rollback_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.rolled_back"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "surface_rejection_rate", requires_observability: &[EV, LG], applies_to: &[N_],
        computed_from: &["action.tool.surface_rejected", "action.tool.proposed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // per (profile, family) — C0/S3
    // ── exposure (ADR-0094 D6; §5d.3 §5) — declarations land at C0/S1;
    // computation is C0/S3 (fixtures + the per-plan fold) ─────────────────
    ProcessMetric { name: "tool_surface_tokens", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Tokens,
        fold: N(NA::Capability) },
    ProcessMetric { name: "catalog_exposure_ratio", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned", "action.tool.catalog.built"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_calls", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_tokens", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Tokens,
        fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_latency_ms", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Milliseconds,
        fold: N(NA::Capability) },
    ProcessMetric { name: "unrevealed_call_rate", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.call.refused", "action.tool.proposed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "first_call_success_after_reveal", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.surface.revealed", "action.tool.completed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "reveal_to_use_distance", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.surface.revealed", "action.tool.proposed"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    ProcessMetric { name: "catalog_drift_events", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.catalog.delta"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "epochs_adopted", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.catalog.epoch"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "selection_recall_at_need", requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Ppm,
        fold: N(NA::NoDetector) }, // needs labels — OQ-237
    ProcessMetric { name: "selection_precision", requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Ppm,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "unrevealed_need", requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.call.refused"], unit: MetricUnit::Count,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "index_recall_at_k", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // offline, instrument-charged
    ProcessMetric { name: "index_ndcg_at_k", requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) },
    // ── grounding / compliance (T-LCD-13) ────────────────────────────────
    ProcessMetric { name: "validator_pass_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["verification.validator.invoked", "verification.validator.verdict"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // `verdict` is §05f's
    ProcessMetric { name: "claim_state_agreement", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["verification.claim.assessed"], unit: MetricUnit::Ppm,
        fold: N(NA::Capability) },
    ProcessMetric { name: "evidence_traceability", requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["verification.evidence.recorded"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // the evidence corpus is Stage 3's
    ProcessMetric { name: "artifact_activation_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.artefact.activated"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // `activated` is §05c's
    ProcessMetric { name: "artifact_follow_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.activated", "verification.artefact.followed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "delivered_but_never_activated", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.artefact.activated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    // ── autonomy ────────────────────────────────────────────────────────
    ProcessMetric { name: "human_interventions", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "approval_requests", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.requested"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "approval_wait_ms", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "approval_cache_hit_rate", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "steering_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.steer.issued"], unit: MetricUnit::Count,
        fold: N(NA::Capability) }, // the steering classes are §05e's
    // ── security ────────────────────────────────────────────────────────
    ProcessMetric { name: "permission_denials", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "policy_evaluations", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::CountMap, fold: C },
    ProcessMetric { name: "egress_blocked", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.egress.denied", "security.containment.violated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) }, // `security.egress.*` is S2.4's
    ProcessMetric { name: "credential_mediations", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.credential.used"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "taint_declassifications", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.label.declassified"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "audit_completeness", requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["lifecycle.run.finished"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // `audit_view` + §05g.6 attestation is Stage 2's
    // ── evolvability ────────────────────────────────────────────────────
    ProcessMetric { name: "time_to_diagnose", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.evolution.candidate.transitioned"],
        unit: MetricUnit::Milliseconds, fold: N(NA::Capability) }, // CF-422's re-key; §05h.5's
    ProcessMetric { name: "boundary_overhead_ms", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.component.invoked"], unit: MetricUnit::Milliseconds, fold: C },
    // ── budget discipline ────────────────────────────────────────────────
    ProcessMetric { name: "budget_exceeded_count", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.exceeded"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "search_budget_consumed", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.consumed"], unit: MetricUnit::Attribution,
        fold: N(NA::NotRun) }, // arm-level (`search_budget`) — the Stage-3 harness's
    ProcessMetric { name: "eval_budget_consumed", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.consumed"], unit: MetricUnit::Attribution,
        fold: N(NA::NotRun) }, // arm-level (`eval_budget`) — the Stage-3 harness's
    ProcessMetric { name: "attribution_coverage", requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.cost.attributed"], unit: MetricUnit::Ppm, fold: C },
];

/// Look a metric up by name.
pub fn metric(name: &str) -> Option<&'static ProcessMetric> {
    PROCESS_METRICS.iter().find(|m| m.name == name)
}

/// The registry check (DF-S1.5-2 — AC-R-2.2.1-5's second half): resolve a
/// metric's `requires_observability` against the declared `min_observability`
/// of every class it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryCheck {
    /// `class → min_observability` for every registered class the metric reads.
    pub resolved: BTreeMap<&'static str, Observability>,
    /// Classes the metric reads that are not registered yet (declared —
    /// land with their owning slices; not a violation).
    pub pending: Vec<&'static str>,
    /// The declared requirement's deficit — resolved levels not covered by
    /// `requires_observability` (empty = pass).
    pub violations: Vec<Observability>,
}

/// Run the check over one declaration.
pub fn registry_check(m: &ProcessMetric) -> RegistryCheck {
    let mut resolved = BTreeMap::new();
    let mut pending = Vec::new();
    for c in m.computed_from {
        match hh_ledger::classes::lookup(c) {
            Some(spec) => {
                resolved.insert(*c, to_ontology(spec.min_observability));
            }
            None => pending.push(*c),
        }
    }
    let declared: BTreeSet<Observability> = m.requires_observability.iter().copied().collect();
    let violations = resolved
        .values()
        .copied()
        .filter(|o| !declared.contains(o))
        .collect();
    RegistryCheck {
        resolved,
        pending,
        violations,
    }
}

/// The whole-catalogue sweep — every row must pass the resolved-union check
/// (its `requires_observability` covers every registered class it reads).
pub fn check_catalogue() -> Result<(), TelemetryError> {
    for m in PROCESS_METRICS {
        let check = registry_check(m);
        if !check.violations.is_empty() {
            return Err(TelemetryError::RegistryViolation {
                detail: format!(
                    "{}: requires_observability misses {:?} (reads {:?})",
                    m.name, check.violations, check.resolved
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_row_resolves_against_the_class_table() {
        // DF-S1.5-2 / AC-R-2.2.1-5's registry half: requires_observability
        // covers the declared min_observability of every registered class a
        // metric reads.
        check_catalogue().unwrap();
        // Spot-check the resolution itself — `ttft_ms` reads model_io classes
        // and must declare model_io.
        let t = registry_check(metric("ttft_ms").unwrap());
        assert_eq!(
            t.resolved["model.call.attempt.completed"],
            Observability::ModelIo
        );
        // And a model_io-requiring metric on an events-level run is n/a —
        // the check is what makes that honest.
        assert!(metric("ttft_ms")
            .unwrap()
            .requires_observability
            .contains(&Observability::ModelIo));
    }

    #[test]
    fn a_metric_underdeclaring_observability_fails_the_check() {
        let bad = ProcessMetric {
            name: "test:underdeclared",
            requires_observability: &[Observability::Events],
            applies_to: BOTH,
            computed_from: &["model.call.attempt.started"], // min = model_io
            unit: MetricUnit::Count,
            fold: C,
        };
        let check = registry_check(&bad);
        assert_eq!(check.violations, vec![Observability::ModelIo]);
    }

    #[test]
    fn pending_classes_are_declared_not_violations() {
        // `context.artefact.activated` landed with the context builder
        // (S1.19); `verification.artefact.followed` is still a declared-
        // pending Stage-3 class — `artifact_follow_rate` reads both.
        let m = metric("artifact_follow_rate").unwrap();
        let check = registry_check(m);
        assert!(check.pending.contains(&"verification.artefact.followed"));
        assert!(check.violations.is_empty());
    }

    #[test]
    fn the_catalogue_covers_the_spec_names() {
        // §5h.1 §8's process-metric list is the registry's population.
        for name in [
            "tokens_total",
            "tokens_by_bucket",
            "cost_money",
            "cache_hit_rate",
            "model_calls",
            "tool_calls",
            "retries",
            "harness_overhead_share",
            "latency_e2e_ms",
            "ttft_ms",
            "ttfm_ms",
            "turn_phase_profile",
            "context_tokens_per_call",
            "context_growth_rate",
            "compaction_count",
            "compaction_token_reduction",
            "tool_error_rate",
            "invalid_action_rate",
            "duplicate_effect_count",
            "abandoned_effect_count",
            "unknown_effect_count",
            "crash_recovery_count",
            "resume_count",
            "fenced_writer_count",
            "loop_detection",
            "validator_pass_rate",
            "claim_state_agreement",
            "evidence_traceability",
            "artifact_activation_rate",
            "artifact_follow_rate",
            "delivered_but_never_activated",
            "human_interventions",
            "approval_requests",
            "approval_wait_ms",
            "approval_cache_hit_rate",
            "steering_count",
            "permission_denials",
            "policy_evaluations",
            "egress_blocked",
            "credential_mediations",
            "taint_declassifications",
            "audit_completeness",
            "time_to_diagnose",
            "attribution_coverage",
            "rollback_count",
            "search_budget_consumed",
            "eval_budget_consumed",
            "boundary_overhead_ms",
        ] {
            assert!(metric(name).is_some(), "{name} missing from the catalogue");
        }
        let mut names = std::collections::BTreeSet::new();
        for m in PROCESS_METRICS {
            assert!(names.insert(m.name), "duplicate {}", m.name);
        }
    }
}
