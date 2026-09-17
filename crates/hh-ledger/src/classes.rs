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
    /// ADR-0066 Rule P: only a kernel component may produce this row, and the
    /// payload partitions into `audit_fields`/`content_refs` (Rule C).
    pub audit_grade: bool,
    /// Rule P: the `producer.component_class` values admitted for this class —
    /// [`KERNEL_PRODUCERS`] for every audit-grade row, empty for unrestricted
    /// (non-audit) classes.
    pub producers: &'static [&'static str],
    /// Rule C: the payload members that are inline audit fields (`"*"` = every
    /// member). Every member not in `content_refs` must be covered here.
    pub audit_fields: &'static [AuditField],
    /// Rule C: the payload members that are content-addressed references — the
    /// only redactable payload members (`blob:`/`sha256:`/`b3:` addresses).
    pub content_refs: &'static [&'static str],
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

/// Retained for compatibility — AC-R-2.8.6-12's Rule-C bound is the class's
/// `offload_threshold` ([`DEFAULT_OFFLOAD_BYTES`]), checked in
/// `Store::append`'s `check_audit_partition`. The ADR-0234 interim cap this
/// named is superseded by the per-member [`AuditField`] bounds.
#[deprecated(
    note = "Rule C's bound is the class offload_threshold (AC-R-2.8.6-12); per-member AuditField bounds do the small-field work"
)]
pub const AUDIT_PAYLOAD_MAX_BYTES: usize = 4 * 1024;

/// One declared `audit_fields` member (§5g.6 I-A1 Rule C): the payload member `name`
/// is an inline audit field — canonical, hashed, never offloaded, never redactable —
/// bounded at `max_bytes` canonical bytes. The sentinel name `"*"` declares the
/// **open partition**: every payload member is an audit field at
/// [`AUDIT_FIELD_MAX_BYTES`] (used for classes whose payload dossier is another
/// subsystem's contract; the dossier-fixed partitions are enumerated).
#[derive(Debug, Clone, Copy)]
pub struct AuditField {
    /// The payload member name (`"*"` = every member is an audit field).
    pub name: &'static str,
    /// The member's canonical-bytes bound (ADR-0067 D4: audit fields are small —
    /// ids, hashes, closed enums, integers, `ContentAddress`es, provenance records).
    pub max_bytes: usize,
}

/// The default per-member audit-field bound — 512 canonical bytes covers every id,
/// hash, closed enum, integer and content address.
pub const AUDIT_FIELD_MAX_BYTES: usize = 512;

/// The bound for legitimately record/list-shaped audit fields — `checks[]`,
/// `grants[]`, `hits[]`, `binding_ids[]`, `cross_run_anchors[]`, the checkpoint's
/// signature/proof material.
pub const AUDIT_FIELD_LIST_BYTES: usize = 4 * 1024;

/// An audit field at the default bound.
const fn af(name: &'static str) -> AuditField {
    AuditField {
        name,
        max_bytes: AUDIT_FIELD_MAX_BYTES,
    }
}

/// An audit field at a declared bound.
const fn afb(name: &'static str, max_bytes: usize) -> AuditField {
    AuditField { name, max_bytes }
}

/// The open partition — `audit_fields = {"*"}`.
pub const OPEN_AUDIT: &[AuditField] = &[af("*")];

/// The Rule-P producer set over `producer.component_class` for every audit-grade
/// class: kernel-only (§5g.6 §2 — "accepted only if `producer.component_class ∈
/// class.producers`"; I-A2 — a hook, tool, extension, model or hosted participant
/// causes an audit-grade event only through the kernel).
pub const KERNEL_PRODUCERS: &[&str] = &[crate::event::KERNEL_COMPONENT];

// ── The enumerated Rule-C partitions (§5g.6 §3 dossier) ─────────────────────
// Each list is the class's declared `audit_fields`; members not listed here and
// not in the class's `content_refs` are refused at append (Rule C — an unknown
// member is a schema error, never an offload). `"*"` partitions (`OPEN_AUDIT`)
// cover classes whose payload dossier is the owning subsystem's contract.

/// `lifecycle.lease.*` — the `emit_lease_row` members.
const LEASE_FIELDS: &[AuditField] = &[
    af("scope"),
    af("lease_id"),
    af("holder"),
    af("generation"),
    af("stale_lease_id"),
    af("stale_generation"),
    af("reason"),
];

/// `lifecycle.registry.*` — the registry-store audit rows (content-free: ids,
/// spellings, counts — ADR-0239).
const REGISTRY_FIELDS: &[AuditField] = &[
    af("kind"),
    af("version_id"),
    af("semantic_id"),
    af("admission"),
    af("registrar_origin"),
    af("operation"),
    af("reason"),
    af("subject"),
    af("namespace"),
    af("name"),
    af("label"),
    af("supersedes"),
    af("revoker_origin"),
    af("snapshot_id"),
    af("member_count"),
    af("seq"),
    af("report_id"),
    af("subject_ref"),
    af("suite_ref"),
    af("produced_by"),
];

/// `action.effect.*` — the shared effect-phase partition (§5g.6 §3 dossier:
/// `effect_id, tool_call_id, attempt_no, capability semantic_id,
/// args_canonical_hash, declared EffectClass, effective_risk_class,
/// idempotency_key, env_handle_id, outcome?, output_refs[],
/// detached_effect_ids[], permission_id, reason_code, compensates/reverts` —
/// plus the Stage-1 mechanical members the fold reads: `fencing_token`,
/// `verdict`, `cause`, `baseline_ref`, `compensation_plan_id`,
/// `escalation_ref`, `decision_ref`, `reprepared_after`, `original_effect_id`).
const EFFECT_FIELDS: &[AuditField] = &[
    af("effect_id"),
    af("tool_call_id"),
    af("attempt_no"),
    af("capability"),
    af("capability_id"),
    af("capability_version"),
    af("capability_ref"),
    af("args_canonical_hash"),
    af("declared_risk_class"),
    af("effective_risk_class"),
    af("idempotency_key"),
    af("env_handle_id"),
    af("outcome"),
    afb("output_refs", AUDIT_FIELD_LIST_BYTES),
    afb("detached_effect_ids", AUDIT_FIELD_LIST_BYTES),
    af("permission_id"),
    af("reason"),
    af("reason_code"),
    af("compensates"),
    af("reverts"),
    af("fencing_token"),
    af("verdict"),
    af("cause"),
    af("baseline_ref"),
    af("compensation_plan_id"),
    af("escalation_ref"),
    af("decision_ref"),
    af("reprepared_after"),
    af("original_effect_id"),
    af("schedule_event_id"),
    af("not_before"),
    af("kind"),
    af("model"),
    af("branch_id"),
];

/// `security.permission.decided` — the §5g.6 §3 dossier partition (the
/// `options` member is emitted as `options_presented`; `rendering_ref` is the
/// one content ref — the human-readable rendering is redactable).
const DECIDED_FIELDS: &[AuditField] = &[
    af("permission_id"),
    af("proposal"),
    af("effect_id"),
    af("requested_at"),
    af("effective_authority"),
    af("effective_risk_class"),
    afb("handle_ids", AUDIT_FIELD_LIST_BYTES),
    af("policy_ref"),
    afb("checks", AUDIT_FIELD_LIST_BYTES),
    afb("options_presented", AUDIT_FIELD_LIST_BYTES),
    afb("remedies", AUDIT_FIELD_LIST_BYTES),
    af("reason"),
    af("decider"),
    af("decider_ref"),
    afb("decider_provenance", AUDIT_FIELD_LIST_BYTES),
    af("decision"),
    af("decision_scope"),
    af("originating_permission_id"),
    af("wait_ms"),
    af("attempt_no"),
    afb("taint", AUDIT_FIELD_LIST_BYTES),
    // The budget-path and baseline emitters' coordinates (hh-budget's
    // approvals-exhaustion `deny`; hh-baseline's headless `allow`).
    af("budget_id"),
    af("value"),
    af("limit"),
    af("tool"),
];
const DECIDED_REFS: &[&str] = &["rendering_ref"];

/// `security.permission.granted` / `revoked` — the dossier partition
/// (`handle_id, holder, capability semantic_id, scope, issuer provenance,
/// basis, basis_ref, expiry, rule_ref, authority_delta`; the record members
/// `permission_ref`, `grants`, `ceiling`, `validity`, `parent_handle`,
/// `delegable`, `origin_basis`, `budget_ref`, `cascade`, `reason`, `revoker`).
const GRANT_FIELDS: &[AuditField] = &[
    af("handle_id"),
    afb("permission_ref", AUDIT_FIELD_LIST_BYTES),
    af("holder"),
    afb("issuer", AUDIT_FIELD_LIST_BYTES),
    afb("grants", AUDIT_FIELD_LIST_BYTES),
    af("ceiling"),
    afb("validity", AUDIT_FIELD_LIST_BYTES),
    af("parent_handle"),
    af("delegable"),
    af("origin_basis"),
    af("basis"),
    af("basis_ref"),
    af("budget_ref"),
    af("authority_delta"),
    af("scope"),
    af("permission_id"),
    af("rule_ref"),
    afb("cascade", AUDIT_FIELD_LIST_BYTES),
    af("reason"),
    af("revoker"),
];

/// `security.label.*` — the endorsement rows (`{subject_ref, from, to,
/// endorser, basis, basis_ref}`; `label`/`content_kind` for the applied stamp).
const LABEL_FIELDS: &[AuditField] = &[
    af("subject_ref"),
    af("from"),
    af("to"),
    af("endorser"),
    af("basis"),
    af("basis_ref"),
    af("label"),
    af("content_kind"),
    af("reason"),
];

/// `security.containment.*` — content-free payloads; the loss/probe reports are
/// blob refs (`content_refs`), `subject` a path/host spelling (ADR-0061 D5).
const CONTAINMENT_FIELDS: &[AuditField] = &[
    af("env_handle"),
    af("policy_version_id"),
    af("effective_policy_hash"),
    af("backend"),
    af("isolation_class"),
    afb("enforcement_evidence", AUDIT_FIELD_LIST_BYTES),
    af("point"),
    af("kind"),
    af("subject"),
    af("evidence_kind"),
    af("effect_id"),
    af("field_group"),
    af("reason"),
];
const CONTAINMENT_REFS: &[&str] = &["lowering_loss_ref", "probes_ref", "evidence_ref"];

/// `security.credential.*` — refs, revisions, modes, destinations, the closed
/// `code`, `provided: yes/no` — never a secret value (SV-2; ADR-0058 D4/D6).
const CREDENTIAL_FIELDS: &[AuditField] = &[
    af("binding_id"),
    af("env_handle"),
    af("channel_id"),
    af("channel_revision"),
    af("mode"),
    af("expires_at"),
    af("holder"),
    af("decision_ref"),
    af("destination"),
    af("effect_id"),
    af("decision"),
    af("monitor_decision_ref"),
    af("provided"),
    af("reason"),
    af("caller"),
    afb("binding_ids", AUDIT_FIELD_LIST_BYTES),
    af("from_revision"),
    af("to_revision"),
];

/// `security.secret.*` — tombstone summaries and scan verdicts, never bytes.
const SECRET_FIELDS: &[AuditField] = &[
    af("target_ref"),
    afb("hits", AUDIT_FIELD_LIST_BYTES),
    af("location"),
    afb("hit", AUDIT_FIELD_LIST_BYTES),
    af("detector"),
];

/// `control.budget.exceeded` / `amended` — ceilings/amounts/closed dimensions;
/// `old`/`new` are `BudgetSpec` records (bounded).
const BUDGET_AUDIT_FIELDS: &[AuditField] = &[
    af("budget_id"),
    af("dimension"),
    af("value"),
    af("limit"),
    afb("old", AUDIT_FIELD_LIST_BYTES),
    afb("new", AUDIT_FIELD_LIST_BYTES),
    af("authority"),
];

/// `control.decision` — the audit-grade decision row (`{kind, reason?,
/// budget_id?, dimension?, triggered_by?, decider, call_no?}`).
const DECISION_FIELDS: &[AuditField] = &[
    af("kind"),
    af("reason"),
    af("budget_id"),
    af("dimension"),
    afb("triggered_by", AUDIT_FIELD_LIST_BYTES),
    af("decider"),
    af("call_no"),
];

/// `lifecycle.ledger.redacted` — the tombstone (`{targets[], reason_code,
/// endorser, basis, content_fingerprints[]}`; the fingerprints are hashes —
/// audit fields, not content).
const REDACTED_FIELDS: &[AuditField] = &[
    afb("targets", AUDIT_FIELD_LIST_BYTES),
    af("reason_code"),
    afb("endorser", AUDIT_FIELD_LIST_BYTES),
    af("basis"),
    afb("content_fingerprints", AUDIT_FIELD_LIST_BYTES),
];

/// `lifecycle.ledger.gc` — `{addresses[], policy_ref, tier, retained_until?}`.
const GC_FIELDS: &[AuditField] = &[
    afb("addresses", AUDIT_FIELD_LIST_BYTES),
    af("policy_ref"),
    af("tier"),
    af("retained_until"),
];

/// `security.audit.checkpoint` — the §5g.6 §3 record; nothing offloaded (a
/// proof path over the bound is omitted and recomputed on demand).
const CHECKPOINT_FIELDS: &[AuditField] = &[
    af("kind"),
    af("origin"),
    af("tree_size"),
    af("tree_head"),
    af("chain_hash"),
    afb("prev_checkpoint", AUDIT_FIELD_LIST_BYTES),
    afb("consistency_proof", AUDIT_FIELD_LIST_BYTES),
    afb("cross_run_anchors", AUDIT_FIELD_LIST_BYTES),
    af("idp"),
    afb("rehash", AUDIT_FIELD_LIST_BYTES),
    afb("signatures", AUDIT_FIELD_LIST_BYTES),
    afb("witness_cosignatures", AUDIT_FIELD_LIST_BYTES),
    af("audit_policy_ref"),
];

/// `context.memory.written` / `invalidated` / `read` — the structural members
/// (§05c D4/§5g.6 §3); `conflict_set_ref` is the one content ref.
const MEMORY_FIELDS: &[AuditField] = &[
    af("memory_id"),
    af("version_id"),
    af("kind"),
    af("scope"),
    afb("label", AUDIT_FIELD_LIST_BYTES),
    af("contract_hash"),
    afb("justifications", AUDIT_FIELD_LIST_BYTES),
    af("supersedes"),
    af("lease_generation"),
    af("store"),
    af("reason"),
    af("by"),
    af("replacement"),
    af("fired_stamp"),
    af("query_ref"),
    af("until_seq"),
    afb("delivered", AUDIT_FIELD_LIST_BYTES),
    afb("withheld", AUDIT_FIELD_LIST_BYTES),
    afb("filter_order_attestation", AUDIT_FIELD_LIST_BYTES),
    af("layer"),
    af("delivery_id"),
    afb("rank_evidence", AUDIT_FIELD_LIST_BYTES),
    afb("validity", AUDIT_FIELD_LIST_BYTES),
];
const MEMORY_REFS: &[&str] = &["conflict_set_ref"];

/// `measurement.export.delivered` — `{sink_id, view_kind, seq_range,
/// content_classes, loss_report_ref}` (the loss report is the one content ref).
const EXPORT_FIELDS: &[AuditField] = &[
    af("sink_id"),
    af("view_kind"),
    afb("seq_range", AUDIT_FIELD_LIST_BYTES),
    afb("content_classes", AUDIT_FIELD_LIST_BYTES),
];
const EXPORT_REFS: &[&str] = &["loss_report_ref"];

/// `measurement.evolution.candidate.transitioned` — `{candidate_id, from, to,
/// hypothesis_ref, evidence_refs[]}` (the obligation link fields).
const TRANSITION_FIELDS: &[AuditField] = &[
    af("candidate_id"),
    af("from"),
    af("to"),
    af("hypothesis_ref"),
    afb("evidence_refs", AUDIT_FIELD_LIST_BYTES),
    af("reason"),
];

use crate::manifest::ObservabilityLevel as O;
use Durability::{Ephemeral as Eph, Ledger as Led};
use ScopeKind::{Effect, ModelCall, ToolCall, Turn};

/// The one table (CC7). Order is irrelevant; `lookup` is exact-match.
#[rustfmt::skip]
pub const CLASS_TABLE: &[ClassSpec] = &[
    // ── lifecycle (plane = run_lifecycle; origin = kernel; provenance mandatory) ──
    // audit-grade (§5g.6 §3): lease.*, ledger.{redacted,gc}, run.{created,finished,
    // resumed,suspended,forked,rolled_back}, head.moved, escalation.{raised,resolved}.
    // `run.created`'s payload is the manifest — open partition (`extra` members are
    // manifest facts; the append path never sees this row — `open_run` writes it).
    row_audit("lifecycle.run.created",     O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.run.resumed",     O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.run.suspended",   O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.run.finished",    O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.run.forked",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.run.rolled_back", O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.head.moved",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.lease.acquired",  O::Events, true,  LEASE_FIELDS, &[], None, None),
    row_audit("lifecycle.lease.renewed",   O::Events, true,  LEASE_FIELDS, &[], None, None),
    row_audit("lifecycle.lease.released",  O::Events, true,  LEASE_FIELDS, &[], None, None),
    row_audit("lifecycle.lease.fenced",    O::Events, true,  LEASE_FIELDS, &[], None, None),
    row_audit("lifecycle.ledger.redacted", O::Events, true,  REDACTED_FIELDS, &[], None, None),
    row_audit("lifecycle.ledger.gc",       O::Events, true,  GC_FIELDS, &[], None, None),
    // `lifecycle.escalation.{raised,resolved}` — audit-grade per the §5g.6 §3 list;
    // the emitters land at Stage 2 (the §05e F2/B2 escalation path).
    row_audit("lifecycle.escalation.raised",   O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("lifecycle.escalation.resolved", O::Events, true,  OPEN_AUDIT, &[], None, None),
    // `lifecycle.hosted.native_record` — the hosted adapter's appended leaf
    // (§5g.6 §3; the adapter is a kernel component — Rule P holds).
    row_audit("lifecycle.hosted.native_record", O::Events, true, OPEN_AUDIT, &[], None, None),
    // Non-audit lifecycle rows — still kernel-origin + provenance-bearing.
    row("lifecycle.turn.started",          Led, O::Events, false, true,  Some(Turn), None),
    row("lifecycle.turn.finished",         Led, O::Events, false, true,  None, Some(Turn)),
    row("lifecycle.branch.opened",         Led, O::Events, false, true,  None, None),
    row("lifecycle.branch.disposed",       Led, O::Events, false, true,  None, None),
    row("lifecycle.replay.started",        Led, O::Events, false, true,  None, None),
    row("lifecycle.replay.finished",       Led, O::Events, false, true,  None, None),
    row("lifecycle.component.bound",       Led, O::Events, false, true,  None, None),
    row("lifecycle.component.invoked",     Led, O::Events, false, true,  None, None),
    row("lifecycle.definition.changed",    Led, O::Events, false, true,  None, None),
    row("lifecycle.surface.invoked",       Led, O::Events, false, true,  None, None),
    row("lifecycle.session.attached",      Led, O::Events, false, true,  None, None),
    row("lifecycle.session.detached",      Led, O::Events, false, true,  None, None),
    row("lifecycle.contract.deprecated_use", Led, O::Events, false, true, None, None),
    // The registry-store audit rows (ADR-0151 (e)/0152 (e)/0153 (e); S1.8): kernel
    // component `registry`, content-free payloads (ids/spellings only) — audit-grade.
    row_audit("lifecycle.registry.registered",        O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.admission_refused", O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.published",         O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.name_deprecated",   O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.name_yanked",       O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.version_revoked",   O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.snapshotted",       O::Events, true,  REGISTRY_FIELDS, &[], None, None),
    row_audit("lifecycle.registry.conformance_recorded", O::Events, true, REGISTRY_FIELDS, &[], None, None),

    // ── model boundary ───────────────────────────────────────────────────
    // `model.call.{requested,completed,failed}` are audit-grade (§05b.1; §5g.6 §3
    // structural fields — ids/hashes/enums/`plan_hash`/`served_model`/`usage`/
    // `credential_binding_id`; the open partition stands until the adapter's
    // dossier lands — content rides `content_refs`). Attempt spans are not
    // audit-grade; the stream delta is ephemeral.
    row_audit("model.call.requested",      O::Events, false, OPEN_AUDIT, &[], Some(ModelCall), None),
    row_audit("model.call.completed",      O::Events, false, OPEN_AUDIT, &[], None, Some(ModelCall)),
    row_audit("model.call.failed",         O::Events, false, OPEN_AUDIT, &[], None, Some(ModelCall)),
    row("model.call.attempt.started",      Led, O::ModelIo, false, false, None, None),
    row("model.call.attempt.completed",    Led, O::ModelIo, false, false, None, None),
    row("model.call.attempt.failed",       Led, O::ModelIo, false, false, None, None),
    row("model.stream.delta",              Eph, O::ModelIo, false, false, None, None),
    row("model.route.decided",             Led, O::ModelIo, false, false, None, None),
    row("model.cache.resolved",            Led, O::ModelIo, false, false, None, None),
    row("model.surface.relowered",         Led, O::Events, false, false, None, None),

    // ── action (incl. the twelve effect phases) ──────────────────────────
    // Every `action.effect.*` phase is audit-grade (§5g.6 §3; §05a.2 §6) under the
    // shared `EFFECT_FIELDS` partition; `intended` opens the `effect_id` scope, the
    // four terminals close it.
    row_audit("action.effect.intended",    O::Events, false, EFFECT_FIELDS, &[], Some(Effect), None),
    row_audit("action.effect.authorized",  O::Events, false, EFFECT_FIELDS, &[], None, None),
    row_audit("action.effect.refused",     O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    row_audit("action.effect.prepared",    O::Events, false, EFFECT_FIELDS, &[], None, None),
    row_audit("action.effect.deferred",    O::Events, false, EFFECT_FIELDS, &[], None, None),
    row_audit("action.effect.committed",   O::Events, false, EFFECT_FIELDS, &[], None, None),
    row_audit("action.effect.observed",    O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    row_audit("action.effect.unknown",     O::Events, false, EFFECT_FIELDS, &[], None, None),
    // `probed` and `compensated` close the effect scope **conditionally** —
    // `probed{undeterminable}` stays open (returns to `unknown`), and
    // `compensated`/`reverted` also admit the scope-free marker form
    // (`payload.original_effect_id`) for post-terminal saga marks. The predicate is
    // `effect::scope_close_fires` (§5a.2 states; ADR-0238).
    row_audit("action.effect.probed",      O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    row_audit("action.effect.compensated", O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    row_audit("action.effect.reverted",    O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    row_audit("action.effect.abandoned",   O::Events, false, EFFECT_FIELDS, &[], None, Some(Effect)),
    // The `action.tool.*` set — `proposed`/`started` are audit-grade (§5g.6 §3:
    // `started` carries `{execution_id, attribution_token_hash}` — CF-214);
    // `proposed` opens the tool_call scope, the three terminals close it; the two
    // streaming-item classes are ephemeral.
    row_audit("action.tool.proposed",      O::Events, false, OPEN_AUDIT, &[], Some(ToolCall), None),
    row_audit("action.tool.started",       O::Events, false, OPEN_AUDIT, &[], None, None),
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
    // `action.environment.healed` is audit-grade (§5g.6 §3 — the heal record is a
    // consequential act an auditor must answer for; the emitter lands with the
    // environment manager's heal path).
    row_audit("action.environment.healed",         O::Events, true,  OPEN_AUDIT, &[], None, None),
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
    // `pending`/`decided`/`granted`/`revoked` are audit-grade (§5g.6 §3) — the
    // permission lifecycle is the consequential act the trail exists to answer
    // for. `requested` stays non-audit (I-A4: nothing audit-grade lives only
    // there). `pending`'s payload dossier is S2.6's approval machinery — open
    // partition.
    row_audit("security.permission.pending",  O::Events, false, OPEN_AUDIT, &[], None, None),
    row_audit("security.permission.decided",  O::Events, false, DECIDED_FIELDS, DECIDED_REFS, None, None),
    row_audit("security.permission.granted",  O::Events, false, GRANT_FIELDS, &[], None, None),
    row_audit("security.permission.revoked",  O::Events, false, GRANT_FIELDS, &[], None, None),
    row_audit("security.label.applied",       O::Events, false, LABEL_FIELDS, &[], None, None),
    row_audit("security.label.endorsed",      O::Events, false, LABEL_FIELDS, &[], None, None),
    row_audit("security.label.declassified",  O::Events, false, LABEL_FIELDS, &[], None, None),
    row_audit("security.policy.evaluated",    O::Events, false, OPEN_AUDIT, &[], None, None),
    // `security.egress.{requested,decided}` — one `decided` per request with
    // `source` (§5g.4 §6; CF-482's spelling). Audit-grade; the egress mediator is
    // a kernel component.
    row_audit("security.egress.requested",    O::Events, false, OPEN_AUDIT, &[], None, None),
    row_audit("security.egress.decided",      O::Events, false, OPEN_AUDIT, &[], None, None),
    // `security.extension.loaded` (and the family per ADR-0064 D7) — the pin
    // attestation the Rule-O producer obligation resolves against.
    row_audit("security.extension.loaded",    O::Events, false, OPEN_AUDIT, &[], None, None),
    // `security.audit.checkpoint` — the signed-checkpoint row (emitter at
    // Stage 2); the enumerated §5g.6 §3 partition — nothing offloaded.
    row_audit("security.audit.checkpoint",    O::Events, true,  CHECKPOINT_FIELDS, &[], None, None),
    // The containment rows (§5g.4 §3; ADR-0061 D5) — audit-grade, kernel-origin,
    // provenance-mandatory, content-free payloads (`lowering_loss_ref`/`probes_ref`
    // are blob refs; `subject` is a path/host spelling). `security.containment.
    // amended` lands with `amend()` at Stage 2 (R-2.8.4 stage map; ADR-0062 (e)).
    row_audit("security.containment.applied",    O::Events, true, CONTAINMENT_FIELDS, CONTAINMENT_REFS, None, None),
    row_audit("security.containment.violated",   O::Events, true, CONTAINMENT_FIELDS, CONTAINMENT_REFS, None, None),
    row_audit("security.containment.unverified", O::Events, true, CONTAINMENT_FIELDS, &[], None, None),
    // The credential/secret rows (§5g.3 §3 events; ADR-0058 D4/D6) — audit-grade,
    // kernel-origin, provenance-mandatory. Payloads are content-free by contract:
    // refs, revisions, modes, destinations, the closed `code`, `provided: yes/no` —
    // never a secret value or materialising byte (SV-2). `used`/`denied` carry
    // `effect_id` (SV-5: every use audited); `denied` fires before any refusal is
    // visible; `redacted` is the at-source tombstone row; `leak_detected` is the
    // test-battery's audit+triage row (it is a scan verdict, never a runtime
    // detector firing on a live path — ADR-0059 D2).
    row_audit("security.credential.bound",        O::Events, true, CREDENTIAL_FIELDS, &[], None, None),
    row_audit("security.credential.used",         O::Events, true, CREDENTIAL_FIELDS, &[], None, None),
    row_audit("security.credential.denied",       O::Events, true, CREDENTIAL_FIELDS, &[], None, None),
    row_audit("security.credential.revoked",      O::Events, true, CREDENTIAL_FIELDS, &[], None, None),
    row_audit("security.credential.rotated",      O::Events, true, CREDENTIAL_FIELDS, &[], None, None),
    row_audit("security.secret.redacted",         O::Events, true, SECRET_FIELDS, &[], None, None),
    row_audit("security.secret.leak_detected",    O::Events, true, SECRET_FIELDS, &[], None, None),

    // ── context (P1) ─────────────────────────────────────────────────────
    // The one context-plane row this slice needs so `context_view` is non-vacuous
    // (ADR-0029 §5 names it; the artefact/compaction/retrieval classes land with
    // the context builder, §05c).
    row("context.observation.recorded",    Led, O::Events, false, false, None, None),
    // `context.artefact.delivered` — the context builder's per-artefact delivery row
    // (§5a.1: `{artefact_id, delivery_id, version, kind, activation_observable,
    // model_call_id, position, rendering_ref, provenance}`; P1; one per artefact per
    // model call). `verify_resume` (§3.3.4, S1.9) reads it as the removal witness —
    // a delivered tool may not leave the inventory (T-LCD-13). Component-emitted
    // (the context builder), provenance-bearing per the §8.1 table.
    row_prov("context.artefact.delivered", Led, O::Events, false, false, true,  None, None),
    // `context.compaction.completed` — the E4 re-arm signal for budget soft
    // thresholds (§8.2 E4; ADR-0040/0107): advice re-arms only after a
    // `status ∈ {applied, fallback_applied}` completion. Kernel-produced.
    row("context.compaction.completed",    Led, O::Events, false, true,  None, None),
    // `context.memory.written/invalidated` and the structural part of
    // `context.memory.read` are audit-grade (§5g.6 §3; ADR-0066 as amended,
    // CF-180) — the memory lifecycle is a consequential act.
    row_audit("context.memory.written",     O::Events, true, MEMORY_FIELDS, MEMORY_REFS, None, None),
    row_audit("context.memory.invalidated", O::Events, true, MEMORY_FIELDS, MEMORY_REFS, None, None),
    row_audit("context.memory.read",        O::Events, true, MEMORY_FIELDS, MEMORY_REFS, None, None),

    // ── control (P3) — the budget/accounting classes (§8.2; ADR-0039/0040/0041)
    // and the audit-grade decision row. Every `control.*` row is kernel-origin +
    // provenance-bearing; `control.decision`, `control.budget.exceeded` and
    // `control.budget.amended` are audit-grade (§5g.6 §3 — ADR-0234's interim
    // "not content-free" ruling is superseded by the Rule-C partition: the
    // payloads are bounded records of ids/ints/enums, never free text).
    row("control.budget.allocated",        Led, O::Events, false, true,  None, None),
    row("control.budget.reserved",         Led, O::Events, false, true,  None, None),
    row("control.budget.consumed",         Led, O::Events, false, true,  None, None),
    row("control.budget.released",         Led, O::Events, false, true,  None, None),
    row_audit("control.budget.exceeded",   O::Events, true,  BUDGET_AUDIT_FIELDS, &[], None, None),
    row_audit("control.budget.amended",    O::Events, true,  BUDGET_AUDIT_FIELDS, &[], None, None),
    row_audit("control.decision",          O::Events, true,  DECISION_FIELDS, &[], None, None),
    // `control.wakeup.fired` — the audit-grade wakeup record (§5g.6 §3; the
    // `control.wakeup.occurred` ingress row stays ordinary-ledger).
    row_audit("control.wakeup.fired",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    // The subagent/merge/ownership/work-item audit rows (§5g.6 §3) — declared
    // now (the class list is dialect schema); their emitters land with the
    // subagent (S2.x/§05e F3) and fleet (§05i) machinery. `spawned`'s child_run
    // scope opener lands with the spawn path (§5a.1 defers it to Stage 4).
    row_audit("control.subagent.spawned",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.subagent.result",       O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.subagent.cancelled",    O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.subagent.detached",     O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.merge.resolved",        O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.ownership.transferred", O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.dispatched",         O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.stopped",            O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.blocked",            O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.handoff",            O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.owner_changed",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.owner_acknowledged", O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("control.work_item.cancelled",          O::Events, true,  OPEN_AUDIT, &[], None, None),
    // The durable retry/timer rows of the `R-2.2.3⁰ᵃ` slice: `scheduled{scope_id,
    // attempt_no, not_before}` IS the timer (process memory is never the only copy —
    // ADR-0130 §5); `fired`/`skipped{reason}` consume a schedule so `retry_due` is a
    // pure fold. `control.timeout.fired` records a deadline force-close (§5a.3
    // recovery table "any scope past its deadline"; §5a.2 failure modes).
    row("control.retry.scheduled",         Led, O::Events, false, true,  None, None),
    row("control.retry.fired",             Led, O::Events, false, true,  None, None),
    row("control.retry.skipped",           Led, O::Events, false, true,  None, None),
    row("control.timeout.fired",           Led, O::Events, false, true,  None, None),

    // ── measurement (P7) — the spend-attribution row (§8.2
    // `measurement.cost.attributed{scope?, subject_ref, dimension, quantity|money?,
    // provenance, basis}`; ADR-0043's measurement stamps ride the payload).
    // Kernel-produced (the account derives it), provenance mandatory.
    row("measurement.cost.attributed",     Led, O::Events, false, true,  None, None),
    // `measurement.export.delivered{sink_id, view_kind, seq_range,
    // content_classes, loss_report_ref}` (§5h.1 §2.2/§6 — "the only ledger append
    // an exporter may make", a kernel-appended fact with mandatory provenance
    // per ADR-0035's table). The payload is ids/ranges/class spellings only —
    // content-free ⇒ audit-grade (ADR-0066 Rule P/C).
    row_audit("measurement.export.delivered", O::Events, true, EXPORT_FIELDS, EXPORT_REFS, None, None),
    // The evolution audit rows (§5g.6 §3) — a candidate transition and an applied
    // harness edit are consequential acts; the emitters land with the evolution
    // pipeline (§05h). `transitioned` carries the obligation link fields
    // (`hypothesis_ref`, `evidence_refs`).
    row_audit("measurement.evolution.candidate.transitioned", O::Events, true, TRANSITION_FIELDS, &[], None, None),
    row_audit("measurement.harness_edit.applied",             O::Events, true, OPEN_AUDIT, &[], None, None),
    // `measurement.metric.emitted{subject, metric_ref, value, unit,
    // detector_ref}` (§5h.1 §2.2 — restricted to observations *not derivable*
    // from other events; producers are oracles/judges over the scorer boundary,
    // not the kernel). Provenance mandatory per the same table.
    row_prov("measurement.metric.emitted", Led, O::Events, false, false, true,  None, None),

    // ── verification (P4) — `verification.validator.invoked` is an accountable
    // event class (R-ACC-2): every invocation is charged (to the instrument).
    row("verification.validator.invoked",  Led, O::Events, false, false, None, None),
    // `verification.gate.evaluated` / `verification.completion.decided` are
    // audit-grade (§5g.6 §3) — the gate verdicts are consequential decisions.
    row_audit("verification.gate.evaluated",      O::Events, true,  OPEN_AUDIT, &[], None, None),
    row_audit("verification.completion.decided",  O::Events, true,  OPEN_AUDIT, &[], None, None),
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
        producers: &[],
        audit_fields: &[],
        content_refs: &[],
        kernel_origin,
        requires_provenance: kernel_origin,
        ephemeral_fields: &[],
        opens_scope,
        closes_scope,
        lowering: "none",
    }
}

/// An audit-grade row — `audit_grade` + `requires_provenance` forced, the Rule-P
/// producer set ([`KERNEL_PRODUCERS`]) and the Rule-C partition declared.
#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the table's.
const fn row_audit(
    class: &'static str,
    min_observability: O,
    kernel_origin: bool,
    audit_fields: &'static [AuditField],
    content_refs: &'static [&'static str],
    opens_scope: Option<ScopeKind>,
    closes_scope: Option<ScopeKind>,
) -> ClassSpec {
    ClassSpec {
        class,
        durability: Led,
        offload_threshold: DEFAULT_OFFLOAD_BYTES,
        min_observability,
        audit_grade: true,
        producers: KERNEL_PRODUCERS,
        audit_fields,
        content_refs,
        kernel_origin,
        requires_provenance: true,
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
        producers: &[],
        audit_fields: &[],
        content_refs: &[],
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
        producers: &[],
        audit_fields: &[],
        content_refs: &[],
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
    fn credential_and_secret_families_are_audit_grade_and_provenance_mandatory() {
        // §5g.3 §3 (R-2.8.3; S1.13): the seven rows are registered, audit-grade,
        // kernel-origin and provenance-mandatory.
        for c in [
            "security.credential.bound",
            "security.credential.used",
            "security.credential.denied",
            "security.credential.revoked",
            "security.credential.rotated",
            "security.secret.redacted",
            "security.secret.leak_detected",
        ] {
            let spec = lookup(c).unwrap_or_else(|| panic!("{c} not registered"));
            assert!(spec.audit_grade, "{c}");
            assert!(spec.kernel_origin, "{c}");
            assert!(spec.requires_provenance, "{c}");
            assert_eq!(spec.durability, Durability::Ledger, "{c}");
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
