//! The telemetry scope kinds and the closed measurement-point table
//! (§5h.1 §2.1/§2.5; ADR-0043 D1 — adding a point is a spec change, so the table
//! is declared data, not producer-local code).
//!
//! The sixteen scope kinds are the spec's closed `scope_kind` sum; Stage 1 wires
//! **M1–M12** (§5h.1 §9). Points whose opener/closer classes are not yet
//! registered stay declared: the fold simply never fires until the owning slice
//! lands the class (CC8 — a later stage adds emitters, never renames). The
//! pointless rows (M16 budget, M17 export, M18 hosted-run, M-ENV-1…3 gauges)
//! measure counters/records rather than open→closed scope pairs — they carry
//! `scope: None`.

use crate::errors::TelemetryError;

/// The closed `scope_kind` sum (§5h.1 §2.1 — sixteen spellings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// M1 — the run.
    Run,
    /// M2 — a turn.
    Turn,
    /// M3 — a logical model call.
    ModelCall,
    /// M4 — one attempt of a model call.
    ModelAttempt,
    /// M5 — a context assembly.
    ContextAssembly,
    /// M6 — a compaction.
    Compaction,
    /// M7 — a logical tool call.
    ToolCall,
    /// M8 — one attempt of a tool call (an effect attempt).
    ToolAttempt,
    /// M9 — an effect lifecycle.
    Effect,
    /// M10 — a permission wait (proposal → decision).
    Permission,
    /// M11 — a validation.
    Validation,
    /// M12 — a subagent run.
    Subagent,
    /// M13 — an environment operation.
    EnvironmentOp,
    /// M14 — a memory operation.
    MemoryOp,
    /// M15 — a wakeup.
    Wakeup,
    /// M19 — a component call across the variant-host boundary.
    ComponentCall,
}

impl ScopeKind {
    /// Every scope kind, in spec order.
    pub const ALL: [ScopeKind; 16] = [
        ScopeKind::Run,
        ScopeKind::Turn,
        ScopeKind::ModelCall,
        ScopeKind::ModelAttempt,
        ScopeKind::ContextAssembly,
        ScopeKind::Compaction,
        ScopeKind::ToolCall,
        ScopeKind::ToolAttempt,
        ScopeKind::Effect,
        ScopeKind::Permission,
        ScopeKind::Validation,
        ScopeKind::Subagent,
        ScopeKind::EnvironmentOp,
        ScopeKind::MemoryOp,
        ScopeKind::Wakeup,
        ScopeKind::ComponentCall,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::Run => "run",
            ScopeKind::Turn => "turn",
            ScopeKind::ModelCall => "model_call",
            ScopeKind::ModelAttempt => "model_attempt",
            ScopeKind::ContextAssembly => "context_assembly",
            ScopeKind::Compaction => "compaction",
            ScopeKind::ToolCall => "tool_call",
            ScopeKind::ToolAttempt => "tool_attempt",
            ScopeKind::Effect => "effect",
            ScopeKind::Permission => "permission",
            ScopeKind::Validation => "validation",
            ScopeKind::Subagent => "subagent",
            ScopeKind::EnvironmentOp => "environment_op",
            ScopeKind::MemoryOp => "memory_op",
            ScopeKind::Wakeup => "wakeup",
            ScopeKind::ComponentCall => "component_call",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Result<ScopeKind, TelemetryError> {
        ScopeKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| TelemetryError::UnknownScopeKind {
                spelling: s.to_string(),
            })
    }
}

/// Where a span's `duration_ms` comes from (§5h.1 §2.6 — payload-measured,
/// `measured_at`-stamped; never a `ts` difference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationSource {
    /// The terminal event's payload carries `<field>` (integer ms) beside
    /// `measured_at`.
    PayloadField(&'static str),
    /// The terminal event's `timing{}` member carries monotonic-clock stamps —
    /// the duration is `end − start` of the named members (e.g.
    /// `last_byte_ms − request_sent_ms` for M4).
    TimingPair {
        /// The start stamp member inside `timing`.
        start: &'static str,
        /// The end stamp member inside `timing`.
        end: &'static str,
    },
    /// No single payload duration is declared (the measured members are
    /// per-phase — the span reports `duration_ms: absent`, an honest omission,
    /// never a `ts` difference).
    Unmeasured,
}

/// One row of the closed measurement-point table (§5h.1 §2.5).
pub struct MeasurementPoint {
    /// The point id (`M1`…`M19`, `M-ENV-1`…`M-ENV-3`).
    pub id: &'static str,
    /// The scope kind, or `None` for point/counter measurements (M16–M18,
    /// M-ENV) that are not open→closed span pairs.
    pub scope: Option<ScopeKind>,
    /// The class spellings that open the scope (durable events only — the
    /// fold reads the registered subset).
    pub opens: &'static [&'static str],
    /// The class spellings that close it.
    pub closes: &'static [&'static str],
    /// Where the span's payload-measured duration comes from.
    pub duration: DurationSource,
    /// What the point measures (the spec's "Measured (unit)" column).
    pub measured: &'static [&'static str],
    /// The producing component.
    pub producer: &'static str,
    /// The stage the point's emitters land (§5h.1 §9).
    pub stage: &'static str,
}

/// The whole closed table (§5h.1 §2.5 — verbatim ids, scope kinds, opened→closed
/// classes and measured units).
#[rustfmt::skip]
pub const MEASUREMENT_POINTS: &[MeasurementPoint] = &[
    MeasurementPoint { id: "M1", scope: Some(ScopeKind::Run),
        opens: &["lifecycle.run.created"], closes: &["lifecycle.run.finished"],
        duration: DurationSource::PayloadField("wall_ms"),
        measured: &["wall_ms", "cost_view totals", "stop_reason", "subagent_count"],
        producer: "runtime", stage: "C0/S1" },
    MeasurementPoint { id: "M2", scope: Some(ScopeKind::Turn),
        opens: &["lifecycle.turn.started"], closes: &["lifecycle.turn.finished"],
        duration: DurationSource::PayloadField("turn_e2e_ms"),
        measured: &["turn_e2e_ms", "turn_phase_profile", "ttft_ms", "ttfm_ms"],
        producer: "control strategy + gateway", stage: "C0/S1" },
    MeasurementPoint { id: "M3", scope: Some(ScopeKind::ModelCall),
        opens: &["model.call.requested"],
        closes: &["model.call.completed", "model.call.failed"],
        duration: DurationSource::PayloadField("latency_ms"),
        measured: &["TokenVector", "Money", "latency_ms", "attempts", "stop_reason", "error.class", "rerouted?"],
        producer: "gateway (§05b) / router", stage: "C0/S1" },
    MeasurementPoint { id: "M4", scope: Some(ScopeKind::ModelAttempt),
        opens: &["model.call.attempt.started"],
        closes: &["model.call.attempt.completed", "model.call.attempt.failed"],
        duration: DurationSource::TimingPair { start: "request_sent_ms", end: "last_byte_ms" },
        measured: &["Timing{request_sent_ms,first_byte_ms?,first_token_ms?,last_byte_ms}", "http_status_class", "retry_reason", "measured_at"],
        producer: "gateway", stage: "C0/S1" },
    MeasurementPoint { id: "M5", scope: Some(ScopeKind::ContextAssembly),
        // A point event — `assembly_ms` is a payload field, not a new class (CF-092).
        opens: &["context.assembled"], closes: &["context.assembled"],
        duration: DurationSource::PayloadField("assembly_ms"),
        measured: &["assembly_ms", "view_hash", "item count", "tokens by provenance class", "opacity_dynamic", "artifacts delivered"],
        producer: "context builder (§05c)", stage: "C0/S2" },
    MeasurementPoint { id: "M6", scope: Some(ScopeKind::Compaction),
        opens: &["context.compaction.started"], closes: &["context.compaction.completed"],
        duration: DurationSource::PayloadField("compaction_ms"),
        measured: &["compaction_ms", "tokens_before/after", "model calls consumed (harness_overhead.compaction)", "variant ref"],
        producer: "compaction variant (§05c)", stage: "C0/S2" },
    MeasurementPoint { id: "M7", scope: Some(ScopeKind::ToolCall),
        opens: &["action.tool.proposed"],
        closes: &["action.tool.completed", "action.tool.rejected", "action.tool.surface_rejected"],
        duration: DurationSource::PayloadField("tool_ms"),
        measured: &["tool_ms (incl. permission wait)", "executor_ms", "observation_bytes", "truncated?", "error.class"],
        producer: "tool executor (§05d)", stage: "C0/S1" },
    MeasurementPoint { id: "M8", scope: Some(ScopeKind::ToolAttempt),
        opens: &["action.effect.committed"],
        closes: &["action.effect.observed", "action.effect.unknown"],
        duration: DurationSource::PayloadField("attempt_ms"),
        measured: &["attempt_ms", "isolation_mode", "exit status"],
        producer: "executor", stage: "C0/S1" },
    MeasurementPoint { id: "M9", scope: Some(ScopeKind::Effect),
        opens: &["action.effect.intended"],
        closes: &["action.effect.observed", "action.effect.refused", "action.effect.reverted",
                  "action.effect.abandoned", "action.effect.probed"],
        duration: DurationSource::Unmeasured, // per-phase: authorize_ms/prepare_ms/dispatch_to_observed_ms/compensation_ms
        measured: &["authorize_ms", "prepare_ms", "dispatch_to_observed_ms", "unknown→probed cycles", "compensation_ms", "risk class"],
        producer: "runtime (§05a)", stage: "C0/S1" },
    MeasurementPoint { id: "M10", scope: Some(ScopeKind::Permission),
        // Opened by the durable proposal, closed by the decision naming it
        // (`decided.proposal` = the proposing event's id).
        opens: &["action.tool.proposed", "action.effect.intended"],
        closes: &["security.permission.decided"],
        duration: DurationSource::PayloadField("approval_wait_ms"),
        measured: &["approval_wait_ms", "decider", "cached?", "monitor decision latency"],
        producer: "reference monitor (§05g)", stage: "C0/S1" },
    MeasurementPoint { id: "M11", scope: Some(ScopeKind::Validation),
        opens: &["verification.validator.invoked"], closes: &["verification.validator.verdict"],
        duration: DurationSource::PayloadField("validation_ms"),
        measured: &["validation_ms", "validator ResourceVector → harness_overhead.verification"],
        producer: "validator (§05f)", stage: "C0/S1" },
    MeasurementPoint { id: "M12", scope: Some(ScopeKind::Subagent),
        opens: &["control.subagent.spawned"],
        closes: &["control.subagent.result", "control.subagent.cancelled"],
        duration: DurationSource::PayloadField("delegation_ms"),
        measured: &["child run link", "budget_slice", "child totals rolled up once", "delegation_ms"],
        producer: "orchestrator (§05e)", stage: "C1/S4" },
    // ── Stage-2+/C1 points — declared now (the closed list is spec data), wired
    // when their owning slices land the classes (CC8). ──
    MeasurementPoint { id: "M13", scope: Some(ScopeKind::EnvironmentOp),
        opens: &["action.environment.attached"],
        closes: &["action.environment.detached", "action.environment.snapshot"],
        duration: DurationSource::PayloadField("env_ms"),
        measured: &["env_ms", "env_cpu_ms?", "network_bytes?", "snapshot_ms", "snapshot_bytes"],
        producer: "environment service (§05a)", stage: "C0/S2" },
    MeasurementPoint { id: "M14", scope: Some(ScopeKind::MemoryOp),
        opens: &["context.memory.read", "context.memory.written", "context.memory.invalidated"],
        closes: &["context.memory.read", "context.memory.written", "context.memory.invalidated"],
        duration: DurationSource::Unmeasured,
        measured: &["count", "bytes", "authority", "taint", "withheld_count", "invalidation_reason"],
        producer: "memory (§05c)", stage: "C0/S2" },
    MeasurementPoint { id: "M15", scope: Some(ScopeKind::Wakeup),
        opens: &["control.wakeup.scheduled"], closes: &["control.wakeup.fired"],
        duration: DurationSource::PayloadField("sleep_ms"),
        measured: &["sleep_ms", "trigger"],
        producer: "durability (§05a)", stage: "C0/S2" },
    MeasurementPoint { id: "M19", scope: Some(ScopeKind::ComponentCall),
        opens: &["lifecycle.component.invoked"], closes: &["lifecycle.component.invoked"],
        duration: DurationSource::PayloadField("boundary_overhead_ms"),
        measured: &["boundary_overhead_ms per (class_id, placement)"],
        producer: "variant host (§06)", stage: "C0/S3" },
    // ── Point measurements — counters/records, not open→closed spans. ──
    MeasurementPoint { id: "M16", scope: None,
        opens: &["control.budget.consumed", "control.budget.exceeded"], closes: &[],
        duration: DurationSource::Unmeasured,
        measured: &["running totals vs limit per dimension"],
        producer: "envelope (§05e)", stage: "C0/S1" },
    MeasurementPoint { id: "M17", scope: None,
        opens: &["measurement.export.delivered"], closes: &[],
        duration: DurationSource::Unmeasured,
        measured: &["sink", "classes", "seq_range", "loss report"],
        producer: "exporter", stage: "C0/S1" },
    MeasurementPoint { id: "M18", scope: None,
        opens: &["lifecycle.hosted.native_record"], closes: &[],
        duration: DurationSource::Unmeasured,
        measured: &["participant usage lowered to model.call.completed{usage} / measurement.cost.attributed{reconstructed_from_native_log}"],
        producer: "hosting adapter (§06)", stage: "C1/S4" },
    MeasurementPoint { id: "M-ENV-1", scope: None,
        opens: &["action.environment.provisioned"], closes: &[],
        duration: DurationSource::Unmeasured, measured: &["env.reserved_ms"],
        producer: "environment service", stage: "C0/S2" },
    MeasurementPoint { id: "M-ENV-2", scope: None,
        opens: &["action.environment.ready"], closes: &[],
        duration: DurationSource::Unmeasured, measured: &["env.active_ms"],
        producer: "environment service", stage: "C0/S2" },
    MeasurementPoint { id: "M-ENV-3", scope: None,
        opens: &["action.environment.suspended"], closes: &[],
        duration: DurationSource::Unmeasured, measured: &["env.suspended_ms"],
        producer: "environment service", stage: "C0/S2" },
];

/// The point row for `id` (`"M1"`…`"M19"`, `"M-ENV-1"`…`"M-ENV-3"`).
pub fn measurement_point(id: &str) -> Option<&'static MeasurementPoint> {
    MEASUREMENT_POINTS.iter().find(|p| p.id == id)
}

/// The point(s) an event class opens a scope for — `(point, scope)` pairs where
/// `scope` is `Some` (point/counter rows open no span).
pub fn openers(class: &str) -> impl Iterator<Item = &'static MeasurementPoint> + '_ {
    MEASUREMENT_POINTS
        .iter()
        .filter(move |p| p.scope.is_some() && p.opens.contains(&class))
}

/// The point(s) an event class closes a scope for.
pub fn closers(class: &str) -> impl Iterator<Item = &'static MeasurementPoint> + '_ {
    MEASUREMENT_POINTS
        .iter()
        .filter(move |p| p.scope.is_some() && p.closes.contains(&class))
}

/// The closed ancestor chain for `parent_scope` resolution — innermost-first.
/// A span's parent is the innermost still-open ancestor-kind span at open
/// time (structural derivation from the taxonomy, never a guessed edge).
pub fn ancestors(kind: ScopeKind) -> &'static [ScopeKind] {
    use ScopeKind::*;
    match kind {
        Run => &[],
        Turn => &[Run],
        ModelCall | ContextAssembly | Compaction | ToolCall | Validation | Subagent | MemoryOp => {
            &[Turn, Run]
        }
        ModelAttempt => &[ModelCall, Turn, Run],
        ToolAttempt => &[ToolCall, Turn, Run],
        Effect => &[ToolCall, Turn, Run],
        // M10 opens on `action.tool.proposed`/`action.effect.intended` — the
        // spans those events also open (`tool_call`/`effect`) contain the wait.
        Permission => &[ToolCall, Effect, Turn, Run],
        EnvironmentOp | Wakeup | ComponentCall => &[Run],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_kind_sum_is_the_spec_sixteen() {
        let spellings: Vec<&str> = ScopeKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            spellings,
            [
                "run",
                "turn",
                "model_call",
                "model_attempt",
                "context_assembly",
                "compaction",
                "tool_call",
                "tool_attempt",
                "effect",
                "permission",
                "validation",
                "subagent",
                "environment_op",
                "memory_op",
                "wakeup",
                "component_call"
            ]
        );
        for k in ScopeKind::ALL {
            assert_eq!(ScopeKind::parse(k.as_str()).unwrap(), k);
        }
        assert!(matches!(
            ScopeKind::parse("bogus"),
            Err(TelemetryError::UnknownScopeKind { .. })
        ));
    }

    #[test]
    fn m1_through_m12_are_wired_to_ledger_classes() {
        // §9 Stage 1: the M1–M12 scope kinds are wired to the ledger classes —
        // every opener/closer a Stage-1 point names is a registered class.
        for p in MEASUREMENT_POINTS
            .iter()
            .filter(|p| p.stage == "C0/S1" && p.scope.is_some())
        {
            for c in p.opens.iter().chain(p.closes.iter()) {
                // `verification.validator.verdict` is declared (the verdict row
                // lands with §05f) — a point may name a not-yet-registered
                // spelling only as declared data, never silently.
                if *c == "verification.validator.verdict" {
                    continue;
                }
                assert!(
                    hh_ledger::classes::lookup(c).is_some(),
                    "{} names unregistered class {c}",
                    p.id
                );
            }
            assert!(p.scope.is_some(), "{} has a scope", p.id);
        }
        // The Stage-1-wired set is exactly M1–M12.
        let wired: Vec<&str> = MEASUREMENT_POINTS
            .iter()
            .filter(|p| p.stage == "C0/S1" && p.scope.is_some())
            .map(|p| p.id)
            .collect();
        assert_eq!(
            wired,
            ["M1", "M2", "M3", "M4", "M7", "M8", "M9", "M10", "M11"]
        );
    }

    #[test]
    fn the_table_is_closed_and_unique() {
        let mut ids = std::collections::BTreeSet::new();
        for p in MEASUREMENT_POINTS {
            assert!(ids.insert(p.id), "duplicate point {}", p.id);
        }
        assert_eq!(ids.len(), 22);
    }
}
