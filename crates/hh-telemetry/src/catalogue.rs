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
use hh_ontology::eval::{Dimension, Direction, MetricLevel, MetricValueType, OutcomeClassPolicy};
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
    /// An integer-millisecond distribution `{stratum: {n, min, p50, p95,
    /// max}}` (nearest-rank; a realised value, never an interpolated one).
    DistributionMs,
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
            MetricUnit::DistributionMs => "ms",
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
    /// The scorecard dimension (ADR-0045 D1).
    pub dimension: Dimension,
    /// Whether a higher or lower reading is better.
    pub direction: Direction,
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
    /// The full §5h.2 `MetricDeclaration` form (the registry record shape —
    /// the single complete form of ADR-0045 D1, CF-094).
    pub fn declaration(&self) -> MetricDeclaration {
        let value_type = match self.unit {
            MetricUnit::Tokens
            | MetricUnit::CountMap
            | MetricUnit::Attribution
            | MetricUnit::DistributionMs => MetricValueType::Vector,
            _ => MetricValueType::Decimal,
        };
        MetricDeclaration {
            name: self.name.to_string(),
            dimension: self.dimension,
            level: MetricLevel::Run,
            value_type,
            direction: self.direction,
            unit: self.unit.as_str().to_string(),
            requires_observability: self.requires_observability.iter().copied().collect(),
            applies_to_classes: self.applies_to.iter().copied().collect(),
            detector_classes_allowed: [hh_ontology::compliance::Detector::Deterministic]
                .into_iter()
                .collect(),
            outcome_class_policy: match self.dimension {
                Dimension::Efficiency => OutcomeClassPolicy::for_efficiency(),
                _ => OutcomeClassPolicy::for_capability(),
            },
            // The spec-named process vetoes: `audit_completeness` (ADR-0068 §6)
            // and `duplicate_effect_count` (ADR-0047 §5 — "duplicate side
            // effect after retry").
            veto: matches!(
                self.name,
                "audit_completeness"
                    | "duplicate_effect_count"
                    | "memory.revoked_delivered"
                    | "memory.stale_delivered"
            ),
            ..MetricDeclaration::default()
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
    ProcessMetric { name: "tokens_total", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Tokens, fold: C },
    ProcessMetric { name: "tokens_by_bucket", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Tokens, fold: C },
    ProcessMetric { name: "cost_money", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.cost.attributed"], unit: MetricUnit::MicroUnits, fold: C },
    ProcessMetric { name: "cache_hit_rate", dimension: Dimension::Efficiency, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.completed"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "latency_e2e_ms", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.finished"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "ttft_ms", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[IO], applies_to: BOTH,
        computed_from: &["model.call.attempt.completed"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "ttfm_ms", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[IO], applies_to: BOTH,
        computed_from: &["model.call.attempt.completed"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "turn_phase_profile", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.turn.started", "lifecycle.turn.finished"],
        unit: MetricUnit::Attribution, fold: N(NA::NotRun) }, // the profile is Stage 2 (§9)
    ProcessMetric { name: "context_tokens_per_call", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.assembled", "model.call.requested"],
        unit: MetricUnit::Tokens, fold: N(NA::Capability) }, // `context.assembled` is §05c's
    ProcessMetric { name: "context_growth_rate", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.assembled"], unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "compaction_count", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.compaction.completed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "compaction_token_reduction", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.compaction.started", "context.compaction.completed"],
        unit: MetricUnit::Tokens, fold: N(NA::Capability) }, // tokens_before/after fields are §05c's
    ProcessMetric { name: "harness_overhead_share", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.cost.attributed"], unit: MetricUnit::Ppm, fold: C },
    // `harness_overhead.execution_ms` — the helper-boundary M-point
    // (AC-R-2.5.5-11; S3.9): per-effect `execution_ms` stamped on
    // `action.tool.completed`, folded into a distribution stratified on
    // `(executor_class, isolation_class)`. §5d.5 §8 renders hosted rows
    // `n/a{observability}` — the declaration requires `ledger` (a hosted
    // participant never produces dispatch-plane rows).
    ProcessMetric { name: "harness_overhead.execution_ms", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["action.tool.completed"], unit: MetricUnit::DistributionMs, fold: C },
    ProcessMetric { name: "model_calls", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.requested"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "tool_calls", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.tool.proposed"], unit: MetricUnit::Count, fold: C },
    // ── reliability ─────────────────────────────────────────────────────
    ProcessMetric { name: "tool_error_rate", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.tool.completed", "action.tool.rejected", "action.tool.surface_rejected"],
        unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "invalid_action_rate", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["model.call.failed"], unit: MetricUnit::Ppm,
        fold: N(NA::Capability) }, // the closed error.class sum is §05d.5's
    ProcessMetric { name: "retries", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV, IO], applies_to: BOTH,
        computed_from: &["control.retry.fired", "model.call.attempt.started"],
        unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "duplicate_effect_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.committed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "abandoned_effect_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.abandoned"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "unknown_effect_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["action.effect.unknown"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "crash_recovery_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.resumed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "resume_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.resumed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "fenced_writer_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.lease.fenced"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "loop_detection", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.turn.started"], unit: MetricUnit::Count,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "rollback_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.run.rolled_back"], unit: MetricUnit::Count, fold: C },
    // ── the five recovery metrics (AC-R-2.2.3-12; §5a.3 §8 — `applies_to
    // = {native}`: a hosted participant's resume is n/a unless declared) ──
    ProcessMetric { name: "recovery_latency_ms", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["lifecycle.lease.acquired", "lifecycle.run.resumed"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "wasted_calls", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["model.call.requested", "model.call.completed", "model.call.failed", "lifecycle.run.resumed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "unknown_effects_per_resume", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.effect.unknown", "lifecycle.run.resumed"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "heal_count", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.environment.healed"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "wakeups_skipped", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["control.wakeup.skipped"], unit: MetricUnit::CountMap, fold: C },
    ProcessMetric { name: "surface_rejection_rate", dimension: Dimension::Reliability, direction: Direction::Lower, requires_observability: &[EV, LG], applies_to: &[N_],
        computed_from: &["action.tool.surface_rejected", "action.tool.proposed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // per (profile, family) — C0/S3
    // ── exposure (ADR-0094 D6; §5d.3 §5) — declarations land at C0/S1;
    // computation is C0/S3 (fixtures + the per-plan fold) ─────────────────
    ProcessMetric { name: "tool_surface_tokens", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Tokens,
        fold: N(NA::Capability) },
    ProcessMetric { name: "catalog_exposure_ratio", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned", "action.tool.catalog.built"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_calls", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_tokens", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Tokens,
        fold: N(NA::Capability) },
    ProcessMetric { name: "discovery_latency_ms", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Milliseconds,
        fold: N(NA::Capability) },
    ProcessMetric { name: "unrevealed_call_rate", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.call.refused", "action.tool.proposed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "first_call_success_after_reveal", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.surface.revealed", "action.tool.completed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "reveal_to_use_distance", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.surface.revealed", "action.tool.proposed"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    ProcessMetric { name: "catalog_drift_events", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.catalog.delta"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "epochs_adopted", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.catalog.epoch"], unit: MetricUnit::Count,
        fold: N(NA::Capability) },
    ProcessMetric { name: "selection_recall_at_need", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Ppm,
        fold: N(NA::NoDetector) }, // needs labels — OQ-237
    ProcessMetric { name: "selection_precision", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.exposure.planned"], unit: MetricUnit::Ppm,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "unrevealed_need", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV, IO], applies_to: &[N_],
        computed_from: &["action.tool.call.refused"], unit: MetricUnit::Count,
        fold: N(NA::NoDetector) },
    ProcessMetric { name: "index_recall_at_k", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // offline, instrument-charged
    ProcessMetric { name: "index_ndcg_at_k", dimension: Dimension::Grounding, direction: Direction::Lower, requires_observability: &[EV], applies_to: &[N_],
        computed_from: &["action.tool.discovery.searched"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) },
    // ── grounding / compliance (T-LCD-13) ────────────────────────────────
    ProcessMetric { name: "validator_pass_rate", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["verification.validator.invoked", "verification.validator.verdict"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // `verdict` is §05f's
    ProcessMetric { name: "claim_state_agreement", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["verification.claim.reconciled"], unit: MetricUnit::Ppm,
        fold: N(NA::Capability) }, // the class is S1.21's; the fold is Stage 3's
    ProcessMetric { name: "evidence_traceability", dimension: Dimension::Grounding, direction: Direction::Higher, requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["verification.evidence.recorded"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // the evidence corpus is Stage 3's
    ProcessMetric { name: "artifact_activation_rate", dimension: Dimension::Compliance, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.artefact.activated"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // `activated` is §05c's
    ProcessMetric { name: "artifact_follow_rate", dimension: Dimension::Compliance, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.activated", "verification.artefact.followed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "delivered_but_never_activated", dimension: Dimension::Compliance, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.artefact.activated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    // ── memory lifecycle (§5c.4; R-2.4.4; the C0/Stage-2 vetoes) ─────────
    ProcessMetric { name: "memory.revoked_delivered", dimension: Dimension::Compliance, direction: Direction::Lower, requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.memory.invalidated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) }, // veto — a revoked delivery is never legal
    ProcessMetric { name: "memory.stale_delivered", dimension: Dimension::Compliance, direction: Direction::Lower, requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["context.artefact.delivered", "context.retrieval.completed"],
        unit: MetricUnit::Count, fold: N(NA::Capability) }, // veto — a stale delivery is never legal
    ProcessMetric { name: "memory.validity_rate", dimension: Dimension::Compliance, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.retrieval.completed", "context.assembled"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) },
    ProcessMetric { name: "memory.activated", dimension: Dimension::Compliance, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.artefact.activated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    ProcessMetric { name: "memory.followed", dimension: Dimension::Compliance, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["verification.artefact.followed"],
        unit: MetricUnit::Count, fold: N(NA::Capability) },
    ProcessMetric { name: "memory.unknown_admitted_rate", dimension: Dimension::Compliance, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.assembled", "context.retrieval.completed"],
        unit: MetricUnit::Ppm, fold: N(NA::Capability) }, // `unknown`-lifecycle admissions share
    ProcessMetric { name: "memory.over_invalidation", dimension: Dimension::Compliance, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["context.retrieval.completed"],
        unit: MetricUnit::Count, fold: N(NA::NoDetector) }, // staleness flagged but still-valid — informational
    // ── autonomy ────────────────────────────────────────────────────────
    ProcessMetric { name: "human_interventions", dimension: Dimension::Autonomy, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "approval_requests", dimension: Dimension::Autonomy, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        // I-P5 (S1.23): `approvals.requested` counts the durable owed-decision
        // rows (`pending`), never the ephemeral prompt renderings.
        computed_from: &["security.permission.pending"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "approval_wait_ms", dimension: Dimension::Autonomy, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Milliseconds, fold: C },
    ProcessMetric { name: "approval_cache_hit_rate", dimension: Dimension::Autonomy, direction: Direction::Higher, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Ppm, fold: C },
    ProcessMetric { name: "steering_count", dimension: Dimension::Autonomy, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.steer.issued"], unit: MetricUnit::Count,
        fold: N(NA::Capability) }, // the steering classes are §05e's
    // ── security ────────────────────────────────────────────────────────
    ProcessMetric { name: "permission_denials", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "policy_evaluations", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.permission.decided"], unit: MetricUnit::CountMap, fold: C },
    ProcessMetric { name: "egress_blocked", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.egress.denied", "security.containment.violated"],
        unit: MetricUnit::Count, fold: N(NA::Capability) }, // `security.egress.*` is S2.4's
    ProcessMetric { name: "credential_mediations", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.credential.used"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "taint_declassifications", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["security.label.declassified"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "audit_completeness", dimension: Dimension::Security, direction: Direction::Lower, requires_observability: &[EV, LG], applies_to: BOTH,
        computed_from: &["lifecycle.run.finished"], unit: MetricUnit::Ppm,
        fold: N(NA::NotRun) }, // `audit_view` + §05g.6 attestation is Stage 2's
    // ── evolvability ────────────────────────────────────────────────────
    ProcessMetric { name: "time_to_diagnose", dimension: Dimension::Evolvability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["measurement.evolution.candidate.transitioned"],
        unit: MetricUnit::Milliseconds, fold: N(NA::Capability) }, // CF-422's re-key; §05h.5's
    ProcessMetric { name: "boundary_overhead_ms", dimension: Dimension::Evolvability, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["lifecycle.component.invoked"], unit: MetricUnit::Milliseconds, fold: C },
    // ── budget discipline ────────────────────────────────────────────────
    ProcessMetric { name: "budget_exceeded_count", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.exceeded"], unit: MetricUnit::Count, fold: C },
    ProcessMetric { name: "search_budget_consumed", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.consumed"], unit: MetricUnit::Attribution,
        fold: N(NA::NotRun) }, // arm-level (`search_budget`) — the Stage-3 harness's
    ProcessMetric { name: "eval_budget_consumed", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
        computed_from: &["control.budget.consumed"], unit: MetricUnit::Attribution,
        fold: N(NA::NotRun) }, // arm-level (`eval_budget`) — the Stage-3 harness's
    ProcessMetric { name: "attribution_coverage", dimension: Dimension::Efficiency, direction: Direction::Lower, requires_observability: &[EV], applies_to: BOTH,
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
            dimension: Dimension::Reliability,
            direction: Direction::Lower,
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
        // (S1.19); `verification.artefact.followed` landed registered with
        // the verification substrate (S1.21) — `artifact_follow_rate`'s
        // classes resolve and the metric is still fold-pending (Stage 3).
        let m = metric("artifact_follow_rate").unwrap();
        let check = registry_check(m);
        assert!(!check.pending.contains(&"verification.artefact.followed"));
        assert!(check.violations.is_empty());
        // `verification.evidence.recorded` remains a declared-pending class.
        let m = metric("evidence_traceability").unwrap();
        let check = registry_check(m);
        assert!(check.pending.contains(&"verification.evidence.recorded"));
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
            "harness_overhead.execution_ms",
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
            "memory.revoked_delivered",
            "memory.stale_delivered",
            "memory.validity_rate",
            "memory.activated",
            "memory.followed",
            "memory.unknown_admitted_rate",
            "memory.over_invalidation",
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
            "recovery_latency_ms",
            "wasted_calls",
            "unknown_effects_per_resume",
            "heal_count",
            "wakeups_skipped",
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
