//! The **declared persistence-policy table** (§5a.1 §3; ADR-0026 §3): one central table,
//! never producer-local code, stating per class `durability`, the inline offload
//! threshold, the minimum `observability_level`, the audit-grade flag (ADR-0066 Rule P),
//! the kernel-origin rule (§5a.1 §5 "origin = kernel on lifecycle/environment rows"),
//! provenance mandatory-ness (ADR-0035 §4 via [`hh_provenance::required_provenance`]
//! where the spellings coincide, plus the class-declared flag), ephemeral payload
//! members, and the scope open/close rules (§5a.1 §3 "scope ids refer to opened
//! scopes").
//!
//! Registered classes are the **C0/Stage-1 set** (the ticket's class list: lifecycle +
//! model + action incl. effect phases + `security.permission.*`), plus the
//! `security.label.*`/`security.policy.evaluated`/`context.observation.recorded` rows the
//! §5a.1 append invariants and the `context_view` projection need to be executable. The
//! `control.*`/`measurement.*` families and the wildcard families land with their owning
//! producers (additive rows — CC8). The `action.tool.*` classes that ADR-0212's OQ-235
//! deferral leaves unregistered (`exposure.planned`, `discovery.searched`,
//! `surface.evicted`, `call.refused`, `catalog.*`) are deliberately absent.
//!
//! Hosting-ABI lowering is `none` for every row — the hosted lowering lands with
//! R-2.10.6/ADR-0164.

#[cfg(test)]
use hh_provenance::ProvenanceEventKind;

/// Whether the §8.1 #7 mandatory-provenance table covers this class spelling
/// (`security.permission.granted`, `security.label.*` — exact or `*` suffix match).
/// Checked against the table's `requires_provenance` column in tests (the table is the
/// declared union: kernel-origin classes + mandatory-table classes + declared rows).
#[cfg(test)]
fn mandatory_table_covers(class: &str) -> bool {
    ProvenanceEventKind::ALL.iter().any(|k| {
        let spelling = k.as_str();
        match spelling.strip_suffix('*') {
            Some(prefix) => class.starts_with(prefix),
            None => class == spelling,
        }
    })
}

/// `durability` — the persistence class (ADR-0026 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// Durable — written to the hash-chained log.
    Ledger,
    /// Ephemeral — `subscribe`-only, never durable, never in `read`.
    Ephemeral,
}

impl Durability {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Durability::Ledger => "ledger",
            Durability::Ephemeral => "ephemeral",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Durability> {
        match s {
            "ledger" => Some(Durability::Ledger),
            "ephemeral" => Some(Durability::Ephemeral),
            _ => None,
        }
    }
}

/// The scope field a class opens or closes (§5a.1 §3 scope chain `run ⊃ turn ⊃
/// model_call ⊃ tool_call`; `effect_id`/`child_run_id`/`branch_id` sit beside it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// `turn_id` — opened by `lifecycle.turn.started`, closed by `lifecycle.turn.finished`.
    Turn,
    /// `model_call_id` — opened by `model.call.requested`, closed by
    /// `model.call.completed`/`failed`.
    ModelCall,
    /// `tool_call_id` — opened by `action.tool.proposed`, closed by
    /// `action.tool.completed`/`rejected`/`surface_rejected`.
    ToolCall,
    /// `effect_id` — opened by `action.effect.intended`, closed by the effect terminals
    /// (`observed`/`refused`/`reverted`/`abandoned`).
    Effect,
    /// `child_run_id` — no Stage-1 opener (`control.subagent.spawned` lands at Stage 4).
    ChildRun,
    /// `branch_id` — no Stage-1 opener (`lifecycle.branch.*`/navigate land at Stage 2).
    Branch,
}

impl ScopeKind {
    /// The scope-field spelling.
    pub fn field(self) -> &'static str {
        match self {
            ScopeKind::Turn => "turn_id",
            ScopeKind::ModelCall => "model_call_id",
            ScopeKind::ToolCall => "tool_call_id",
            ScopeKind::Effect => "effect_id",
            ScopeKind::ChildRun => "child_run_id",
            ScopeKind::Branch => "branch_id",
        }
    }
}

/// One row of the declared persistence-policy table.
pub struct ClassSpec {
    /// The class spelling (`plane.noun.verb`).
    pub class: &'static str,
    /// `ledger` | `ephemeral` — audit-grade classes are always `ledger`.
    pub durability: Durability,
    /// The inline offload threshold in canonical bytes (default [`DEFAULT_OFFLOAD_BYTES`];
    /// class-tunable — OQ-080). A `ledger` payload at-or-above the threshold is refused
    /// (`SchemaViolation`) — offload through `put_blob` and reference.
    pub offload_threshold: usize,
    /// The minimum `observability_level` at which the class is expected (AC-5's
    /// `requires_observability` read).
    pub min_observability: crate::manifest::ObservabilityLevel,
    /// ADR-0066 Rule P: only a kernel component may produce this row, and the payload
    /// is content-free (`AuditFieldsTooLarge` past [`AUDIT_PAYLOAD_MAX_BYTES`]).
    pub audit_grade: bool,
    /// `provenance.origin = kernel` required (§5a.1 §5 lifecycle/environment rows).
    pub kernel_origin: bool,
    /// Provenance is mandatory (ADR-0035 §4 table ∪ the class-declared flag).
    pub requires_provenance: bool,
    /// Payload members that are `ephemeral` (the event is `ledger`; the member is
    /// stripped from the durable form and delivered on `subscribe` — the
    /// `security.permission.requested{rendering}` rule).
    pub ephemeral_fields: &'static [&'static str],
    /// The scope this class opens (its own `scope.<field>` is the new id).
    pub opens_scope: Option<ScopeKind>,
    /// The scope this class closes (its own `scope.<field>` names an open scope).
    pub closes_scope: Option<ScopeKind>,
    /// The Hosting-ABI lowering — `none` for every Stage-1 row.
    pub lowering: &'static str,
}

/// The default inline offload threshold — 64 KiB canonical (ADR-0029 §5; OQ-080).
pub const DEFAULT_OFFLOAD_BYTES: usize = 64 * 1024;

/// The content-free audit-payload cap — the interim Rule-C enforcement (ADR-0066 Rule C;
/// the per-class `audit_fields` whitelists are the §05g audit catalogue's — ADR-0234).
pub const AUDIT_PAYLOAD_MAX_BYTES: usize = 4 * 1024;

use crate::manifest::ObservabilityLevel as O;
use Durability::{Ephemeral as Eph, Ledger as Led};
use ScopeKind::{Effect, ModelCall, ToolCall, Turn};

/// The one table (CC7). Order is irrelevant; `lookup` is exact-match.
#[rustfmt::skip]
pub const CLASS_TABLE: &[ClassSpec] = &[
    // ── lifecycle (plane = run_lifecycle; origin = kernel; provenance mandatory) ──
    // audit-grade: lease.*, ledger.{redacted,gc}, run.{created,finished,resumed,forked,
    // rolled_back}, head.moved; run.suspended joins them per §05a.3's audit list.
    row("lifecycle.run.created",           Led, O::Events, true,  true,  None, None),
    row("lifecycle.run.resumed",           Led, O::Events, true,  true,  None, None),
    row("lifecycle.run.suspended",         Led, O::Events, true,  true,  None, None),
    row("lifecycle.run.finished",          Led, O::Events, true,  true,  None, None),
    row("lifecycle.run.forked",            Led, O::Events, true,  true,  None, None),
    row("lifecycle.run.rolled_back",       Led, O::Events, true,  true,  None, None),
    row("lifecycle.head.moved",            Led, O::Events, true,  true,  None, None),
    row("lifecycle.lease.acquired",        Led, O::Events, true,  true,  None, None),
    row("lifecycle.lease.renewed",         Led, O::Events, true,  true,  None, None),
    row("lifecycle.lease.released",        Led, O::Events, true,  true,  None, None),
    row("lifecycle.lease.fenced",          Led, O::Events, true,  true,  None, None),
    row("lifecycle.ledger.redacted",       Led, O::Events, true,  true,  None, None),
    row("lifecycle.ledger.gc",             Led, O::Events, true,  true,  None, None),
    // Non-audit lifecycle rows — still kernel-origin + provenance-bearing.
    row("lifecycle.turn.started",          Led, O::Events, false, true,  Some(Turn), None),
    row("lifecycle.turn.finished",         Led, O::Events, false, true,  None, Some(Turn)),
    row("lifecycle.branch.opened",         Led, O::Events, false, true,  None, None),
    row("lifecycle.branch.disposed",       Led, O::Events, false, true,  None, None),
    row("lifecycle.replay.started",        Led, O::Events, false, true,  None, None),
    row("lifecycle.replay.finished",       Led, O::Events, false, true,  None, None),
    row("lifecycle.escalation.raised",     Led, O::Events, false, true,  None, None),
    row("lifecycle.escalation.resolved",   Led, O::Events, false, true,  None, None),
    row("lifecycle.component.bound",       Led, O::Events, false, true,  None, None),
    row("lifecycle.component.invoked",     Led, O::Events, false, true,  None, None),
    row("lifecycle.definition.changed",    Led, O::Events, false, true,  None, None),
    row("lifecycle.surface.invoked",       Led, O::Events, false, true,  None, None),
    row("lifecycle.session.attached",      Led, O::Events, false, true,  None, None),
    row("lifecycle.session.detached",      Led, O::Events, false, true,  None, None),
    row("lifecycle.contract.deprecated_use", Led, O::Events, false, true, None, None),

    // ── model boundary ───────────────────────────────────────────────────
    // `model.call.{requested,completed,failed}` are audit-grade (§05b.1); attempt
    // spans are not; the stream delta is ephemeral.
    row("model.call.requested",            Led, O::Events, true,  false, Some(ModelCall), None),
    row("model.call.completed",            Led, O::Events, true,  false, None, Some(ModelCall)),
    row("model.call.failed",               Led, O::Events, true,  false, None, Some(ModelCall)),
    row("model.call.attempt.started",      Led, O::ModelIo, false, false, None, None),
    row("model.call.attempt.completed",    Led, O::ModelIo, false, false, None, None),
    row("model.call.attempt.failed",       Led, O::ModelIo, false, false, None, None),
    row("model.stream.delta",              Eph, O::ModelIo, false, false, None, None),
    row("model.route.decided",             Led, O::ModelIo, false, false, None, None),
    row("model.cache.resolved",            Led, O::ModelIo, false, false, None, None),
    row("model.surface.relowered",         Led, O::Events, false, false, None, None),

    // ── action (incl. the twelve effect phases) ──────────────────────────
    // Every `action.effect.*` phase is audit-grade with a content-free envelope
    // (§05a.2 §6); `intended` opens the `effect_id` scope, the four terminals close it.
    row("action.effect.intended",          Led, O::Events, true,  false, Some(Effect), None),
    row("action.effect.authorized",        Led, O::Events, true,  false, None, None),
    row("action.effect.refused",           Led, O::Events, true,  false, None, Some(Effect)),
    row("action.effect.prepared",          Led, O::Events, true,  false, None, None),
    row("action.effect.deferred",          Led, O::Events, true,  false, None, None),
    row("action.effect.committed",         Led, O::Events, true,  false, None, None),
    row("action.effect.observed",          Led, O::Events, true,  false, None, Some(Effect)),
    row("action.effect.unknown",           Led, O::Events, true,  false, None, None),
    row("action.effect.probed",            Led, O::Events, true,  false, None, None),
    row("action.effect.compensated",       Led, O::Events, true,  false, None, None),
    row("action.effect.reverted",          Led, O::Events, true,  false, None, Some(Effect)),
    row("action.effect.abandoned",         Led, O::Events, true,  false, None, Some(Effect)),
    // The `action.tool.*` set — `proposed` opens the tool_call scope, the three
    // terminals close it; the two streaming-item classes are ephemeral.
    row("action.tool.proposed",            Led, O::Events, false, false, Some(ToolCall), None),
    row("action.tool.started",             Led, O::Events, false, false, None, None),
    row("action.tool.completed",           Led, O::Events, false, false, None, Some(ToolCall)),
    row("action.tool.rejected",            Led, O::Events, false, false, None, Some(ToolCall)),
    row("action.tool.surface_rejected",    Led, O::Events, false, false, None, Some(ToolCall)),
    row("action.tool.surface.revealed",    Led, O::Events, false, false, None, None),
    row("action.tool.output_chunk",        Eph, O::Events, false, false, None, None),
    row("action.tool.progress",            Eph, O::Events, false, false, None, None),
    // The `action.environment.*` lifecycle family (ADR-0136 §7) — kernel-origin rows.
    row("action.environment.provision.requested",  Led, O::Events, false, true, None, None),
    row("action.environment.provisioned",          Led, O::Events, false, true, None, None),
    row("action.environment.ready",                Led, O::Events, false, true, None, None),
    row("action.environment.fenced",               Led, O::Events, false, true, None, None),
    row("action.environment.suspend.requested",    Led, O::Events, false, true, None, None),
    row("action.environment.suspended",            Led, O::Events, false, true, None, None),
    row("action.environment.resume.requested",     Led, O::Events, false, true, None, None),
    row("action.environment.resumed",              Led, O::Events, false, true, None, None),
    row("action.environment.release.requested",    Led, O::Events, false, true, None, None),
    row("action.environment.released",             Led, O::Events, false, true, None, None),
    row("action.environment.lost",                 Led, O::Events, false, true, None, None),
    row("action.environment.probe.started",        Led, O::Events, false, true, None, None),
    row("action.environment.probe.completed",      Led, O::Events, false, true, None, None),
    row("action.environment.evidence.attached",    Led, O::Events, false, true, None, None),
    row("action.environment.verdict.recorded",     Led, O::Events, false, true, None, None),
    row("action.environment.snapshot.uploaded",    Led, O::Events, false, true, None, None),
    row("action.environment.snapshot.listed",      Led, O::Events, false, true, None, None),
    row("action.environment.drift.detected",       Led, O::Events, false, true, None, None),
    row("action.world_state.snapshot",             Led, O::Events, false, true, None, None),
    row("action.world_state.patch",                Led, O::Events, false, true, None, None),
    row("action.world_state.discontinuity",        Led, O::Events, false, true, None, None),

    // ── security ─────────────────────────────────────────────────────────
    // `requested` is ledger; its `rendering` member is ephemeral. `pending` is ledger
    // (ADR-0026 Phase 2 log). The family is provenance-mandatory at this stage
    // (ADR-0234 — `granted` is the §8.1-table row; the request/decision/grant/revoke
    // chain is the same fact class and §5a.1 §3's provenance rule is applied
    // uniformly pending the §8.1/§5a.1 spelling reconciliation).
    row_eph("security.permission.requested", Led, O::Events, false, false, true, &["rendering"]),
    row_prov("security.permission.pending",  Led, O::Events, false, false, true,  None, None),
    row_prov("security.permission.decided",  Led, O::Events, false, false, true,  None, None),
    row_prov("security.permission.granted",  Led, O::Events, false, false, true,  None, None),
    row_prov("security.permission.revoked",  Led, O::Events, false, false, true,  None, None),
    row_prov("security.label.applied",       Led, O::Events, false, false, true,  None, None),
    row_prov("security.label.endorsed",      Led, O::Events, false, false, true,  None, None),
    row_prov("security.label.declassified",  Led, O::Events, false, false, true,  None, None),
    row("security.policy.evaluated",       Led, O::Events, false, false, None, None),

    // ── context (P1) ─────────────────────────────────────────────────────
    // The one context-plane row this slice needs so `context_view` is non-vacuous
    // (ADR-0029 §5 names it; the artefact/compaction/retrieval classes land with
    // the context builder, §05c).
    row("context.observation.recorded",    Led, O::Events, false, false, None, None),
    // `context.compaction.completed` — the E4 re-arm signal for budget soft
    // thresholds (§8.2 E4; ADR-0040/0107): advice re-arms only after a
    // `status ∈ {applied, fallback_applied}` completion. Kernel-produced.
    row("context.compaction.completed",    Led, O::Events, false, true,  None, None),

    // ── control (P3) — the budget/accounting classes (§8.2; ADR-0039/0040/0041)
    // and the audit-grade decision row. Every `control.*` row is kernel-origin +
    // provenance-bearing; `control.decision` is "the audit-grade record" (§05h).
    // Budget payloads carry ceilings/amounts — not content-free — so only
    // `control.decision` takes the audit-grade (Rule P/C) flags.
    row("control.budget.allocated",        Led, O::Events, false, true,  None, None),
    row("control.budget.reserved",         Led, O::Events, false, true,  None, None),
    row("control.budget.consumed",         Led, O::Events, false, true,  None, None),
    row("control.budget.released",         Led, O::Events, false, true,  None, None),
    row("control.budget.exceeded",         Led, O::Events, false, true,  None, None),
    row("control.budget.amended",          Led, O::Events, false, true,  None, None),
    row("control.decision",                Led, O::Events, true,  true,  None, None),

    // ── measurement (P7) — the spend-attribution row (§8.2
    // `measurement.cost.attributed{scope?, subject_ref, dimension, quantity|money?,
    // provenance, basis}`; ADR-0043's measurement stamps ride the payload).
    // Kernel-produced (the account derives it), provenance mandatory.
    row("measurement.cost.attributed",     Led, O::Events, false, true,  None, None),

    // ── verification (P4) — `verification.validator.invoked` is an accountable
    // event class (R-ACC-2): every invocation is charged (to the instrument).
    row("verification.validator.invoked",  Led, O::Events, false, false, None, None),
];

const fn row(
    class: &'static str,
    durability: Durability,
    min_observability: O,
    audit_grade: bool,
    kernel_origin: bool,
    opens_scope: Option<ScopeKind>,
    closes_scope: Option<ScopeKind>,
) -> ClassSpec {
    ClassSpec {
        class,
        durability,
        offload_threshold: DEFAULT_OFFLOAD_BYTES,
        min_observability,
        audit_grade,
        kernel_origin,
        requires_provenance: kernel_origin,
        ephemeral_fields: &[],
        opens_scope,
        closes_scope,
        lowering: "none",
    }
}

#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the table's.
const fn row_prov(
    class: &'static str,
    durability: Durability,
    min_observability: O,
    audit_grade: bool,
    kernel_origin: bool,
    prov: bool,
    opens_scope: Option<ScopeKind>,
    closes_scope: Option<ScopeKind>,
) -> ClassSpec {
    ClassSpec {
        class,
        durability,
        offload_threshold: DEFAULT_OFFLOAD_BYTES,
        min_observability,
        audit_grade,
        kernel_origin,
        requires_provenance: prov || kernel_origin,
        ephemeral_fields: &[],
        opens_scope,
        closes_scope,
        lowering: "none",
    }
}

#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the table's.
const fn row_eph(
    class: &'static str,
    durability: Durability,
    min_observability: O,
    audit_grade: bool,
    kernel_origin: bool,
    requires_provenance: bool,
    ephemeral_fields: &'static [&'static str],
) -> ClassSpec {
    ClassSpec {
        class,
        durability,
        offload_threshold: DEFAULT_OFFLOAD_BYTES,
        min_observability,
        audit_grade,
        kernel_origin,
        requires_provenance,
        ephemeral_fields,
        opens_scope: None,
        closes_scope: None,
        lowering: "none",
    }
}

/// Look up a class. `None` ⇒ `SchemaViolation` at append (unknown classes are refused,
/// never silently admitted).
pub fn lookup(class: &str) -> Option<&'static ClassSpec> {
    CLASS_TABLE.iter().find(|r| r.class == class)
}

/// Every registered class — for the coverage sweep (AC-5) and dialect tooling.
pub fn registered_classes() -> impl Iterator<Item = &'static ClassSpec> {
    CLASS_TABLE.iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ontology::planes::EventFamily;

    #[test]
    fn every_registered_class_has_a_known_family_prefix() {
        for spec in CLASS_TABLE {
            let prefix = spec.class.split('.').next().unwrap();
            assert!(
                EventFamily::from_prefix(prefix).is_some(),
                "{} has an unknown family prefix",
                spec.class
            );
        }
    }

    #[test]
    fn audit_grade_rows_are_ledger_and_kernel() {
        for spec in CLASS_TABLE {
            if spec.audit_grade {
                assert_eq!(spec.durability, Durability::Ledger, "{}", spec.class);
            }
        }
        // The spec's audit-grade list is a subset of the table (§5a.1 §5).
        for c in [
            "lifecycle.lease.acquired",
            "lifecycle.lease.fenced",
            "lifecycle.ledger.redacted",
            "lifecycle.ledger.gc",
            "lifecycle.run.created",
            "lifecycle.run.finished",
            "lifecycle.head.moved",
        ] {
            assert!(lookup(c).unwrap().audit_grade, "{c}");
        }
        for spec in CLASS_TABLE {
            if spec.class.starts_with("action.effect.") {
                assert!(spec.audit_grade, "{}", spec.class);
            }
        }
    }

    #[test]
    fn ephemeral_set_is_the_declared_c0_set() {
        let eph: Vec<&str> = CLASS_TABLE
            .iter()
            .filter(|r| r.durability == Durability::Ephemeral)
            .map(|r| r.class)
            .collect();
        assert_eq!(
            eph,
            [
                "model.stream.delta",
                "action.tool.output_chunk",
                "action.tool.progress"
            ]
        );
        assert_eq!(
            lookup("security.permission.requested")
                .unwrap()
                .ephemeral_fields,
            &["rendering"]
        );
        assert_eq!(
            lookup("security.permission.pending").unwrap().durability,
            Durability::Ledger
        );
    }

    #[test]
    fn mandatory_table_membership_is_declared() {
        // The declared `requires_provenance` column must cover every §8.1 #7 spelling
        // the table registers — the union is declared, not recomputed.
        for spec in CLASS_TABLE {
            if mandatory_table_covers(spec.class) {
                assert!(
                    spec.requires_provenance,
                    "{} covered by §8.1 #7 but not declared",
                    spec.class
                );
            }
        }
    }

    #[test]
    fn permission_family_is_provenance_mandatory() {
        for spec in CLASS_TABLE {
            if spec.class.starts_with("security.permission.") {
                assert!(spec.requires_provenance, "{}", spec.class);
            }
        }
    }

    #[test]
    fn scope_rules_form_the_chain() {
        assert_eq!(
            lookup("lifecycle.turn.started").unwrap().opens_scope,
            Some(ScopeKind::Turn)
        );
        assert_eq!(
            lookup("model.call.requested").unwrap().opens_scope,
            Some(ScopeKind::ModelCall)
        );
        assert_eq!(
            lookup("action.tool.proposed").unwrap().opens_scope,
            Some(ScopeKind::ToolCall)
        );
        assert_eq!(
            lookup("action.effect.intended").unwrap().opens_scope,
            Some(ScopeKind::Effect)
        );
        assert_eq!(
            lookup("action.effect.observed").unwrap().closes_scope,
            Some(ScopeKind::Effect)
        );
    }
}
