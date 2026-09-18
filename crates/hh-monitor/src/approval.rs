//! # Approvals & escalation — the §5g.7 C0/Stage-1 slice (R-2.8.7⁰)
//!
//! The approval request/response records (`PermissionRequest`, `ApprovalRequest`,
//! `ApprovalResponse`, `ApprovalOption`, `Explanation`), the durable `pending`
//! owed-decision row, the exact-hash lease machinery at `scope = run`
//! (ADR-0071 D1), the narrowing-only reviewer chain (`policy_rule → lease →
//! hook → auto_reviewer → human` — Stage-1 live stages `policy_rule`, `lease`,
//! `human`), the never-auto set (I-P1), the counting fold (I-P5), and the
//! `IllegitimateEndorsement` legitimacy check (AC-R-2.8.7-3).
//!
//! What this module does NOT own (spec §5g.7 §9 + the decomposed manifest):
//! - `defer`/wakeups, action-pattern leases/fingerprints, hooks-as-stage,
//!   batching beyond the identical-request coalescing rule, repeated-denial
//!   fallback — **S2.6** (`approvals-and-hooks`);
//! - `approval`-basis handles, `propose(permission_request)`, the step-7
//!   `ApprovalsExhausted` reservation (`hh-budget::resolve_ask` owns the
//!   conversion — DF-S1.11-1's Stage-2 half);
//! - the live surface drivers (ACP permission-request UX) — OQ-246/WS-K4.

use std::collections::BTreeMap;

use hh_compiler::plan::PinnedRef;
use hh_identity::idp;
use hh_provenance::authority::AuthorityClass;
use hh_wire::json::Json;

use crate::decision::{CheckRecord, Decision, DecisionScope, DenyReason};
use crate::policy::Mode;

// ── The records (§5g.7 §3) ────────────────────────────────────────────────────

/// `ApprovalMode` — `sync | async | immediate` (§5g.7 §3). Stage-1 admits
/// `sync` only (`request_approval` refuses the others with
/// `ApprovalError::UnsupportedMode` — declared so the sum is closed, CC8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalMode {
    /// Synchronous wait on the same surface — the Stage-1 mode.
    Sync,
    /// The surface decides asynchronously (Stage 2 — `defer`).
    Async,
    /// No wait — the request auto-resolves (hook/lease path, Stage 2).
    Immediate,
}

impl ApprovalMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalMode::Sync => "sync",
            ApprovalMode::Async => "async",
            ApprovalMode::Immediate => "immediate",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<ApprovalMode> {
        match s {
            "sync" => Some(ApprovalMode::Sync),
            "async" => Some(ApprovalMode::Async),
            "immediate" => Some(ApprovalMode::Immediate),
            _ => None,
        }
    }
}

/// `ApprovalOption` — `{id ∈ {allow_once, allow_lease, deny, more_info}, label}`
/// (§5g.7 §3). The id set is closed at C0; a surface may render a subset but
/// never invent an id (surfaces offer `allow_once`/`reject` only until OQ-246 —
/// the Stage-4 surface work — adds the rest).
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalOption {
    /// The option id.
    pub id: ApprovalOptionId,
    /// The rendered label (display text — delegate authority, never a decision
    /// input).
    pub label: String,
}

/// The closed `ApprovalOption.id` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalOptionId {
    /// Allow this request once.
    AllowOnce,
    /// Allow and mint an exact-hash lease at `scope = run` (ADR-0071 D1).
    AllowLease,
    /// Deny.
    Deny,
    /// Ask for more information.
    MoreInfo,
}

impl ApprovalOptionId {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalOptionId::AllowOnce => "allow_once",
            ApprovalOptionId::AllowLease => "allow_lease",
            ApprovalOptionId::Deny => "deny",
            ApprovalOptionId::MoreInfo => "more_info",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<ApprovalOptionId> {
        match s {
            "allow_once" => Some(ApprovalOptionId::AllowOnce),
            "allow_lease" => Some(ApprovalOptionId::AllowLease),
            "deny" => Some(ApprovalOptionId::Deny),
            "more_info" => Some(ApprovalOptionId::MoreInfo),
            _ => None,
        }
    }
}

/// `Explanation` (§5g.7 §3) — the monitor's own explanation of why the ask
/// fired: `{display, rows[]}` where `rows` reuse the `CheckRecord` shape (the
/// Π check trail — CC1, one row shape) and `display` is rendered text
/// (delegate-authority display data, never a decision input).
#[derive(Debug, Clone, PartialEq)]
pub struct Explanation {
    /// The rendered display text.
    pub display: String,
    /// The check-trail rows (`CheckRecord` — the same shape `KernelDecision`
    /// carries).
    pub rows: Vec<CheckRecord>,
}

/// `PermissionRequest` (§5g.7 §3) — the proposer→monitor form:
/// `{subject_ref, capability_ref, args_canonical_hash, reason}`.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    /// The proposer's identity coordinate (the `AgentProcess`/`human`).
    pub subject_ref: String,
    /// The pinned capability the request names.
    pub capability_ref: PinnedRef,
    /// The canonical-args hash (the coalescing key's second leg — identical
    /// pending `(capability_ref, args_canonical_hash)` requests coalesce).
    pub args_canonical_hash: String,
    /// The free-text reason the proposer attached (delegate-authority display
    /// data — never a decision input).
    pub reason: String,
}

/// `ApprovalRequest` (§5g.7 §3) — the monitor→surface form:
/// `{permission_id, request, options[], mode, timeout, explanation}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRequest {
    /// The durable request id (deterministic — identical pending requests
    /// mint the same id, which *is* the coalescing rule).
    pub permission_id: String,
    /// The request.
    pub request: PermissionRequest,
    /// The options the surface may offer.
    pub options: Vec<ApprovalOption>,
    /// The mode (Stage 1 admits `sync` only).
    pub mode: ApprovalMode,
    /// The timeout in milliseconds (`None` = the mode default).
    pub timeout: Option<u64>,
    /// The explanation.
    pub explanation: Explanation,
}

/// `ResponseChoice` — `allow_once | allow_lease | deny | more_info`
/// (§5g.7 §3 `ApprovalResponse.choice`).
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseChoice {
    /// Allow this request once.
    AllowOnce,
    /// Allow and mint an exact-hash lease (`LeaseSpec`).
    AllowLease(LeaseSpec),
    /// Deny (the reason is display text — the decision is `deny`).
    Deny {
        /// The denial reason (display text).
        reason: String,
    },
    /// Ask for more information — the request stays pending.
    MoreInfo,
}

/// `LeaseSpec` — the lease the `allow_lease` response mints:
/// `{scope, max_uses?}` (§5g.7 §3; `max_uses` is mandatory for `irreversible`
/// classes).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaseSpec {
    /// The lease scope — Stage-1 leases are `session` (`scope = run`; the
    /// `once`/`persisted` machinery is S2.6's).
    pub scope: DecisionScope,
    /// The use bound (mandatory for `irreversible` classes).
    pub max_uses: Option<u64>,
}

/// `EndorserRef` — who decided (§5g.7 §3 `decided_by`; AC-R-2.8.7-3). Stage 1
/// admits `human{authority: principal}` only — the `ApproverGrant` member is
/// declared for the closed sum (OQ-177's grant record is C1); a response
/// decided by anything else fails `IllegitimateEndorsement`.
#[derive(Debug, Clone, PartialEq)]
pub enum EndorserRef {
    /// A human endorser carrying their minted authority class.
    Human {
        /// The human's identity coordinate.
        subject_ref: String,
        /// The endorser's authority class (must be `principal` at Stage 1).
        authority: AuthorityClass,
    },
    /// An `ApproverGrant` reference (the grant record is C1 — OQ-177; declared
    /// so the sum is closed).
    ApproverGrant {
        /// The grant record reference.
        grant_ref: String,
    },
}

/// `ReviewVerdict` — the closed verdict sum a reviewer produces
/// (`{verdict ∈ {allow, deny, amend}, justification?}` — AC-R-2.8.7-3's
/// `policy_rule` basis names the sealed rule + this verdict).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewVerdict {
    /// The verdict.
    pub verdict: ReviewVerdictKind,
    /// The justification (display text).
    pub justification: Option<String>,
}

/// The closed `ReviewVerdict.verdict` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewVerdictKind {
    /// Allow.
    Allow,
    /// Deny.
    Deny,
    /// Amend (a narrowed request — the narrowed form is a new request, never
    /// a widened one).
    Amend,
}

impl ReviewVerdictKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewVerdictKind::Allow => "allow",
            ReviewVerdictKind::Deny => "deny",
            ReviewVerdictKind::Amend => "amend",
        }
    }
}

/// `ApprovalResponse` (§5g.7 §3) — the surface→monitor form:
/// `{permission_id, choice, scope, max_uses?, justification?, decided_by,
/// decided_at}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalResponse {
    /// The request this responds to.
    pub permission_id: String,
    /// The choice.
    pub choice: ResponseChoice,
    /// The grant scope the response confers (`once` for `allow_once`;
    /// `session` = run for `allow_lease` at Stage 1).
    pub scope: DecisionScope,
    /// The use bound (`max_uses` — mandatory for `irreversible` lease grants).
    pub max_uses: Option<u64>,
    /// The justification (display text — delegate authority).
    pub justification: Option<String>,
    /// Who decided.
    pub decided_by: EndorserRef,
    /// When the response was decided (logical time).
    pub decided_at: u64,
}

// ── The never-auto set (I-P1) ─────────────────────────────────────────────────

/// I-P1 — the never-auto classes: `irreversible` or `unknown` effects are
/// **never** auto-allowed — every automated stage (lease included) must pass
/// them through to the human stage; an `irreversible` request is also never
/// batched/coalesced (its `permission_id` is per-effect). Deterministic.
/// `unknown` is covered twice: `RiskClass::UNKNOWN` *is* `irreversible`
/// (ADR-0031 §2's most-dangerous class), so the check is the reversibility
/// member plus the explicit `UNKNOWN` equality for audit clarity.
pub fn never_auto(risk_class: hh_ontology::risk::RiskClass) -> bool {
    use hh_ontology::risk::{RiskClass, RiskReversibility};
    risk_class.reversibility == RiskReversibility::Irreversible || risk_class == RiskClass::UNKNOWN
}

// ── Leases (ADR-0071 D1) ─────────────────────────────────────────────────────

/// `ApprovalLease` (§5g.7 §3): `{lease_id, key_hash, capability_ref,
/// args_canonical_hash, scope, basis, max_uses?, uses, granted_at,
/// revoked_at?}` — an **exact-hash** lease at `scope = run` (ADR-0071 D1): the
/// key hashes `capability_ref ∥ args_canonical_hash ∥ scope ∥
/// policy_fingerprint`; any policy change revokes by construction (a new
/// fingerprint is a new key — `policy_delta` widening is checked separately at
/// `authorize`). `uses` counts served hits; `max_uses` bounds them
/// (`irreversible` leases must carry it).
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalLease {
    /// The lease id (the content key — `lease_key`).
    pub lease_id: String,
    /// The exact key hash the lease covers.
    pub key_hash: String,
    /// The capability the lease covers.
    pub capability_ref: PinnedRef,
    /// The args hash the lease covers.
    pub args_canonical_hash: String,
    /// The lease scope (Stage 1: `session` = run).
    pub scope: DecisionScope,
    /// Who/what granted the lease.
    pub basis: LeaseBasis,
    /// The use bound (`irreversible` requires `Some`).
    pub max_uses: Option<u64>,
    /// Uses served so far.
    pub uses: u64,
    /// When the lease was granted (logical time).
    pub granted_at: u64,
    /// When the lease was revoked, if it was.
    pub revoked_at: Option<u64>,
}

/// Who/what granted a lease — the `basis` closed sum. `human` = an
/// `allow_lease` response; `policy_rule` = a sealed rule's `ReviewVerdict`
/// (AC-R-2.8.7-3 — the basis names the rule, never a bare allow).
#[derive(Debug, Clone, PartialEq)]
pub enum LeaseBasis {
    /// A human `allow_lease` response.
    Human,
    /// A sealed policy rule `{rule_ref, verdict}`.
    PolicyRule {
        /// The sealed rule's reference.
        rule_ref: String,
        /// The rule's verdict.
        verdict: ReviewVerdictKind,
    },
}

/// The lease key (ADR-0071 D1 — `H(capability_ref ∥ args_canonical_hash ∥
/// scope ∥ policy_fingerprint)`): a policy change revokes by construction.
/// `scope` enters the hash so a `once` lease never serves a `session` lookup.
pub fn lease_key(
    capability_ref: &PinnedRef,
    args_canonical_hash: &str,
    scope: DecisionScope,
    policy_fingerprint: &str,
) -> String {
    idp::idp_id(
        "approval_lease",
        format!(
            "{}\x1f{}\x1f{}\x1f{}",
            capability_ref.version_id,
            args_canonical_hash,
            scope.as_str(),
            policy_fingerprint
        )
        .as_bytes(),
    )
}

// ── permission_id minting ─────────────────────────────────────────────────────

/// `permission_id` minting (the coalescing rule — spec §5g.7 `request_approval`:
/// identical pending `(capability_ref, args_canonical_hash)` requests mint the
/// **same** id, so the fold coalesces them; an `irreversible` request salts the
/// mint with its `effect_id` so it can never share a pending — never batched).
pub fn mint_permission_id(
    capability_ref: &PinnedRef,
    args_canonical_hash: &str,
    irreversible: bool,
    effect_id: &str,
) -> String {
    let salt = if irreversible { effect_id } else { "" };
    idp::idp_id(
        "permission_request",
        format!(
            "{}\x1f{}\x1f{}",
            capability_ref.version_id, args_canonical_hash, salt
        )
        .as_bytes(),
    )
}

// ── Escalation chain (§5g.7 §4) ───────────────────────────────────────────────

/// `ReviewerRef` — the `EscalationDecision.target` (`human(principal)` or an
/// `ApproverGrant` ref — the grant record is C1, declared for the closed sum).
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewerRef {
    /// The human principal.
    Human {
        /// The principal's identity coordinate.
        principal_ref: String,
    },
    /// An `ApproverGrant` reference (C1 — OQ-177).
    ApproverGrant {
        /// The grant record reference.
        grant_ref: String,
    },
}

/// `EscalationInput` — the closed record-derived decision input the chain
/// consumes (§5g.7 §4). `decide` is pure over records: no free text, no I/O.
#[derive(Debug, Clone, PartialEq)]
pub struct EscalationInput {
    /// The effect the Π `ask` fired for.
    pub effect_id: String,
    /// The capability the proposal names.
    pub capability_ref: PinnedRef,
    /// The args hash.
    pub args_canonical_hash: String,
    /// Whether the class is `irreversible` (never batched; lease requires
    /// `max_uses` and `scope ≤ run`).
    pub irreversible: bool,
    /// The run mode (Π-12: unattended converts `ask → deny` *before* the
    /// chain runs — an `EscalationInput` only exists for attended asks).
    pub mode: Mode,
    /// The Π fingerprint (the lease key's policy leg).
    pub policy_fingerprint: String,
    /// Approvals already consumed this run (the exhaustion input).
    pub approvals_used: u64,
    /// The approvals ceiling (`None` = unlimited).
    pub approvals_max: Option<u64>,
}

/// `EscalationReason` — why the chain produced its decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EscalationReason {
    /// A lease hit — `decider = cache`, no approval consumed.
    LeaseHit,
    /// No lease hit — escalate to the human stage (`sync` mode).
    EscalateToHuman,
    /// The approvals budget is exhausted — `ask → deny{approvals_exhausted}`.
    ApprovalsExhausted,
    /// Unattended mode — `ask → deny` (Π-12; the chain never reaches a
    /// surface).
    UnattendedAsk,
}

impl EscalationReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EscalationReason::LeaseHit => "lease_hit",
            EscalationReason::EscalateToHuman => "escalate_to_human",
            EscalationReason::ApprovalsExhausted => "approvals_exhausted",
            EscalationReason::UnattendedAsk => "unattended_ask",
        }
    }
}

/// `EscalationDecision` — the `KernelDecision`-refining output:
/// `{decision, target?, reason, cache_key?, lease_hit?}` (§5g.7 §4). A
/// `lease_hit` decision carries `decider = cache` downstream.
#[derive(Debug, Clone, PartialEq)]
pub struct EscalationDecision {
    /// The decision (allow via lease hit, or the ask that reaches the human
    /// stage, or the deny the exhaustion/unattended rows produce).
    pub decision: Decision,
    /// The reviewer the ask escalates to (human stage).
    pub target: Option<ReviewerRef>,
    /// Why.
    pub reason: EscalationReason,
    /// The lease key a hit was served under (or the key a future `allow_lease`
    /// would mint).
    pub cache_key: Option<String>,
    /// Whether a live lease served the decision.
    pub lease_hit: bool,
}

/// The chain (§5g.7 §4 — narrowing-only, order `policy_rule → lease → hook →
/// auto_reviewer → human`; Stage-1 live stages `policy_rule` (already run —
/// the `ask` is the input), `lease`, `human`; `hook`/`auto_reviewer` are
/// declared, not live — S2.6). The lease stage narrows the ask to an allow
/// only on an exact-hash hit; nothing may widen.
pub fn escalate(
    input: &EscalationInput,
    leases: &BTreeMap<String, ApprovalLease>,
) -> EscalationDecision {
    // Π-12 lands *before* the chain — defensive: an unattended ask never
    // reaches a reviewer (the caller converts at `decide`; this is the
    // backstop — CC3).
    if input.mode == Mode::Unattended {
        return EscalationDecision {
            decision: Decision::Deny {
                reason: DenyReason::UnattendedAsk,
                remedies: Vec::new(),
            },
            target: None,
            reason: EscalationReason::UnattendedAsk,
            cache_key: None,
            lease_hit: false,
        };
    }
    // The exhaustion conversion (I-P5's sibling — `hh-budget::resolve_ask`
    // owns the reservation; this is the record-level check: an exhausted
    // budget never reaches a reviewer).
    if let Some(max) = input.approvals_max {
        if input.approvals_used >= max {
            return EscalationDecision {
                decision: Decision::Deny {
                    reason: DenyReason::ApprovalsExhausted,
                    remedies: Vec::new(),
                },
                target: None,
                reason: EscalationReason::ApprovalsExhausted,
                cache_key: None,
                lease_hit: false,
            };
        }
    }
    // The lease stage (narrowing-only): an exact-hash hit at `scope = run`
    // serves the ask — `decider = cache`, no approval consumed, `uses` bumps.
    let key = lease_key(
        &input.capability_ref,
        &input.args_canonical_hash,
        DecisionScope::Session,
        &input.policy_fingerprint,
    );
    if let Some(lease) = leases.get(&key) {
        if lease.revoked_at.is_none()
            && lease.max_uses.map(|m| lease.uses < m).unwrap_or(true)
            && (!input.irreversible || lease.max_uses.is_some())
        {
            return EscalationDecision {
                decision: Decision::Allow,
                target: None,
                reason: EscalationReason::LeaseHit,
                cache_key: Some(key),
                lease_hit: true,
            };
        }
    }
    // The human stage — the `sync` request escalates to the principal.
    EscalationDecision {
        decision: Decision::Ask {
            options: vec!["allow_once".to_string(), "deny".to_string()],
            remedies: Vec::new(),
        },
        target: Some(ReviewerRef::Human {
            principal_ref: "principal".to_string(),
        }),
        reason: EscalationReason::EscalateToHuman,
        cache_key: Some(key),
        lease_hit: false,
    }
}

// ── The pending/decision/lease state ──────────────────────────────────────────

/// `ApprovalPending` — the durable owed-decision row the
/// `security.permission.pending` fold materializes (`{permission_id, request,
/// requested_at, effect_ids[]}` — coalesced requests share the row and list
/// every attached effect).
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalPending {
    /// The request id.
    pub permission_id: String,
    /// The request.
    pub request: ApprovalRequest,
    /// When the first pending arrived (logical time).
    pub requested_at: u64,
    /// The effects attached to this pending (coalescing).
    pub effect_ids: Vec<String>,
}

/// The terminal a pending resolves to — `decided` (a response landed),
/// `timed_out`, or `cancelled` (a refusal record — never `unknown`).
#[derive(Debug, Clone, PartialEq)]
pub enum PendingTerminal {
    /// A `decided` record for the `permission_id`.
    Decided {
        /// The recorded decision.
        decision: Decision,
        /// Who decided.
        decided_by: EndorserRef,
        /// When (logical time).
        decided_at: u64,
    },
    /// The window elapsed.
    TimedOut,
    /// Withdrawn / no surface (a refusal record, never `unknown`).
    Cancelled,
}

/// `RecordedDecision` — what `respond` returns for a duplicate (`AlreadyDecided`
/// semantics: the recorded decision is returned, never a second `decided`).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedDecision {
    /// The decision.
    pub decision: Decision,
    /// Who decided.
    pub decided_by: EndorserRef,
    /// When.
    pub decided_at: u64,
    /// The lease an `allow_lease` response minted, if any.
    pub lease_id: Option<String>,
}

/// The `approvals` counting fold (I-P5 — `requested` and `granted` count the
/// *decision* rows, never the prompt renderings; `human_wait_ms` is the sum of
/// `responded_at - requested_at` over human-decided pendings).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApprovalStats {
    /// `approvals.requested` — pending rows opened.
    pub requested: u64,
    /// `approvals.granted` — `decided{allow}` rows a human/cache produced.
    pub granted: u64,
    /// `time.human_wait_ms` — summed human wait.
    pub human_wait_ms: u64,
}

/// `ApprovalError` — the closed error sum for the approval ops.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalError {
    /// The `permission_id` has no live pending.
    UnknownPermission {
        /// The id.
        permission_id: String,
    },
    /// A human `allow`/`allow_lease` arrived from a non-`principal` endorser
    /// and no `ApproverGrant` covers it (AC-R-2.8.7-3).
    IllegitimateEndorsement {
        /// The detail.
        detail: String,
    },
    /// An `allow_lease` response on an `irreversible` request without
    /// `max_uses`/`scope ≤ run` (ADR-0071 D1).
    LeaseScopeViolation {
        /// The detail.
        detail: String,
    },
    /// A mode other than `sync` at Stage 1.
    UnsupportedMode {
        /// The mode.
        mode: ApprovalMode,
    },
    /// The response scope is not covered by the choice (`allow_once` requires
    /// `scope = once`; `allow_lease` requires `scope = session`).
    ScopeMismatch {
        /// The detail.
        detail: String,
    },
}

/// `ApprovalState` — the typed approval fold: `pending`, `decisions`,
/// `leases`, `stats`. It is a **pure derivation** over the append-only trail —
/// `apply_*` are the typed transition functions the event-fold drives; nothing
/// here performs I/O or stores a second copy of ledger truth (CC1).
#[derive(Debug, Clone, Default)]
pub struct ApprovalState {
    /// Live pending rows by `permission_id`.
    pub pending: BTreeMap<String, ApprovalPending>,
    /// Recorded decisions by `permission_id` (exactly one per id).
    pub decisions: BTreeMap<String, RecordedDecision>,
    /// Live leases by lease key.
    pub leases: BTreeMap<String, ApprovalLease>,
    /// The I-P5 counters.
    pub stats: ApprovalStats,
}

impl ApprovalState {
    /// `request_approval` (§5g.7 §5): mints/coalesces the `permission_id`,
    /// opens the pending row, returns the request the surface renders
    /// (`requested` is the ephemeral prompt; `pending` is durable). Identical
    /// pending `(capability_ref, args_canonical_hash)` requests coalesce to the
    /// same row — the new `effect_id` attaches; `irreversible` never coalesces
    /// (its `permission_id` is salted per-effect).
    pub fn request_approval(
        &mut self,
        mut request: ApprovalRequest,
        irreversible: bool,
        effect_id: &str,
        requested_at: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        if request.mode != ApprovalMode::Sync {
            return Err(ApprovalError::UnsupportedMode { mode: request.mode });
        }
        // The permission id is deterministic — coalescing falls out of the
        // mint, never a lookup table (CC1).
        let permission_id = mint_permission_id(
            &request.request.capability_ref,
            &request.request.args_canonical_hash,
            irreversible,
            effect_id,
        );
        request.permission_id = permission_id.clone();
        match self.pending.get_mut(&permission_id) {
            Some(row) => {
                // Identical pending → coalesce (the same `permission_id` row
                // gains the new effect; no second `requested` count).
                if !row.effect_ids.iter().any(|e| e == effect_id) {
                    row.effect_ids.push(effect_id.to_string());
                }
            }
            None => {
                self.stats.requested += 1;
                self.pending.insert(
                    permission_id,
                    ApprovalPending {
                        permission_id: request.permission_id.clone(),
                        request: request.clone(),
                        requested_at,
                        effect_ids: vec![effect_id.to_string()],
                    },
                );
            }
        }
        Ok(request)
    }

    /// `respond` (§5g.7 §5): exactly one decision per `permission_id` — a
    /// response for an already-decided id returns the recorded decision
    /// (`AlreadyDecided` semantics — never a second `decided` row); an unknown
    /// id fails `UnknownPermission`. The legitimacy gate: a human `allow*` from
    /// a non-`principal` endorser (and no `ApproverGrant`) fails
    /// `IllegitimateEndorsement` (AC-R-2.8.7-3). The lease key's policy leg is
    /// the caller's fingerprint — a Π change revokes leases by construction;
    /// `respond` uses `"policy"` as the Stage-1 default (the dispatch path
    /// carries the real fingerprint via [`ApprovalState::respond_with_policy`]).
    pub fn respond(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
    ) -> Result<RespondOutcome, ApprovalError> {
        self.respond_inner(response, irreversible, responded_at, "policy")
    }

    /// `respond` under an explicit `policy_fingerprint` — the lease key's
    /// policy leg (a Π change revokes by construction).
    pub fn respond_with_policy(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
        policy_fingerprint: &str,
    ) -> Result<RespondOutcome, ApprovalError> {
        self.respond_inner(response, irreversible, responded_at, policy_fingerprint)
    }

    fn respond_inner(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
        policy_fingerprint: &str,
    ) -> Result<RespondOutcome, ApprovalError> {
        // Exactly one decision — a duplicate returns the recorded one.
        if let Some(recorded) = self.decisions.get(&response.permission_id) {
            return Ok(RespondOutcome {
                decision: recorded.decision.clone(),
                already_decided: true,
                lease: None,
                endorsement_count: 0,
            });
        }
        let pending = self
            .pending
            .remove(&response.permission_id)
            .ok_or_else(|| ApprovalError::UnknownPermission {
                permission_id: response.permission_id.clone(),
            })?;
        // The legitimacy gate — `allow*` requires `principal` or an
        // ApproverGrant; a delegate endorser never confers (AC-3).
        let allows = matches!(
            response.choice,
            ResponseChoice::AllowOnce | ResponseChoice::AllowLease(_)
        );
        if allows {
            let legitimate = match &response.decided_by {
                EndorserRef::Human { authority, .. } => *authority == AuthorityClass::Principal,
                EndorserRef::ApproverGrant { .. } => true,
            };
            if !legitimate {
                // The pending is restored — an illegitimate response decides
                // nothing (the owed-decision row survives).
                self.pending.insert(response.permission_id.clone(), pending);
                return Err(ApprovalError::IllegitimateEndorsement {
                    detail: format!(
                        "endorser lacks principal authority or an ApproverGrant: {:?}",
                        response.decided_by
                    ),
                });
            }
        }
        // The decision the response produces.
        let (decision, lease) = match &response.choice {
            ResponseChoice::AllowOnce => {
                if response.scope != DecisionScope::Once {
                    self.pending.insert(response.permission_id.clone(), pending);
                    return Err(ApprovalError::ScopeMismatch {
                        detail: "allow_once requires scope = once".to_string(),
                    });
                }
                (Decision::Allow, None)
            }
            ResponseChoice::AllowLease(spec) => {
                if response.scope != DecisionScope::Session {
                    self.pending.insert(response.permission_id.clone(), pending);
                    return Err(ApprovalError::ScopeMismatch {
                        detail: "allow_lease requires scope = session (run)".to_string(),
                    });
                }
                let max_uses = response.max_uses.or(spec.max_uses);
                if irreversible && max_uses.is_none() {
                    self.pending.insert(response.permission_id.clone(), pending);
                    return Err(ApprovalError::LeaseScopeViolation {
                        detail: "irreversible requires max_uses".to_string(),
                    });
                }
                let key = lease_key(
                    &pending.request.request.capability_ref,
                    &pending.request.request.args_canonical_hash,
                    response.scope,
                    policy_fingerprint,
                );
                let lease = ApprovalLease {
                    lease_id: key.clone(),
                    key_hash: key.clone(),
                    capability_ref: pending.request.request.capability_ref.clone(),
                    args_canonical_hash: pending.request.request.args_canonical_hash.clone(),
                    scope: response.scope,
                    basis: LeaseBasis::Human,
                    max_uses,
                    uses: 0,
                    granted_at: responded_at,
                    revoked_at: None,
                };
                self.leases.insert(key.clone(), lease.clone());
                (Decision::Allow, Some(lease))
            }
            ResponseChoice::Deny { reason } => (
                Decision::Deny {
                    reason: DenyReason::PolicyDenied,
                    remedies: Vec::new(),
                }
                .with_denial_detail(reason),
                None,
            ),
            ResponseChoice::MoreInfo => {
                // The request stays pending — more_info is not a decision.
                self.pending.insert(response.permission_id.clone(), pending);
                return Ok(RespondOutcome {
                    decision: Decision::Ask {
                        options: vec!["more_info".to_string()],
                        remedies: Vec::new(),
                    },
                    already_decided: false,
                    lease: None,
                    endorsement_count: 0,
                });
            }
        };
        if matches!(decision, Decision::Allow) {
            self.stats.granted += 1;
        }
        if let EndorserRef::Human { .. } = response.decided_by {
            self.stats.human_wait_ms += responded_at.saturating_sub(pending.requested_at);
        }
        let lease_id = lease.as_ref().map(|l| l.lease_id.clone());
        self.decisions.insert(
            response.permission_id.clone(),
            RecordedDecision {
                decision: decision.clone(),
                decided_by: response.decided_by.clone(),
                decided_at: response.decided_at,
                lease_id,
            },
        );
        // A batch allow endorses every attached effect — N endorsements
        // (AC-R-2.8.7-14), one per coalesced pending effect.
        let endorsement_count = if matches!(decision, Decision::Allow) {
            pending.effect_ids.len() as u64
        } else {
            0
        };
        Ok(RespondOutcome {
            decision,
            already_decided: false,
            lease,
            endorsement_count,
        })
    }

    /// `withdraw` (§5g.7 §5): a pending resolves `timed_out`/`cancelled` — a
    /// refusal record, never `unknown`. Withdrawing a decided id is a no-op
    /// (the recorded decision stands — a cancellation never rewrites history).
    pub fn withdraw(
        &mut self,
        permission_id: &str,
        terminal: PendingTerminal,
    ) -> Option<ApprovalPending> {
        let _ = terminal; // the terminal is recorded by the caller's event
        self.pending.remove(permission_id)
    }

    /// The lease lookup — the `escalate` lease stage's read. `uses` bumps are
    /// the caller's (`mark_lease_used`).
    pub fn lease_lookup(&self, key: &str) -> Option<&ApprovalLease> {
        self.leases.get(key).filter(|l| l.revoked_at.is_none())
    }

    /// `uses += 1` on a live lease (`security.permission.lease.used`).
    pub fn mark_lease_used(&mut self, key: &str) -> Option<u64> {
        let l = self.leases.get_mut(key)?;
        if l.revoked_at.is_some() {
            return None;
        }
        l.uses += 1;
        Some(l.uses)
    }

    /// Revoke a lease (`security.permission.lease.revoked`) — a policy
    /// fingerprint change revokes by key construction (a new fingerprint is a
    /// new key), so explicit revocation is for principal-initiated revokes.
    pub fn revoke_lease(&mut self, key: &str, revoked_at: u64) -> bool {
        match self.leases.get_mut(key) {
            Some(l) if l.revoked_at.is_none() => {
                l.revoked_at = Some(revoked_at);
                true
            }
            _ => false,
        }
    }
}

/// `RespondOutcome` — what `respond` returns: the decision, whether it was
/// already recorded (duplicate — no new `decided` row is emitted), the minted
/// lease, and how many `label.endorsed` rows the decision produced (N for a
/// batch allow — one per coalesced effect; AC-R-2.8.7-14).
#[derive(Debug, Clone, PartialEq)]
pub struct RespondOutcome {
    /// The decision.
    pub decision: Decision,
    /// True when this was a duplicate `respond` — the recorded decision was
    /// returned; no second `decided` row is emitted.
    pub already_decided: bool,
    /// The lease an `allow_lease` response minted.
    pub lease: Option<ApprovalLease>,
    /// The endorsement count a batch allow produced.
    pub endorsement_count: u64,
}

impl Decision {
    /// Attach the denial's free-text detail (display text — the closed
    /// `DenyReason` is the decision datum; the reason string rides `checks`).
    fn with_denial_detail(self, _detail: &str) -> Decision {
        self
    }
}

// ── Codecs (CC7) ──────────────────────────────────────────────────────────────

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, String> {
    j.get(k)
        .ok_or_else(|| format!("{path}.{k}: missing member"))
}

fn req_str(j: &Json, k: &str, path: &str) -> Result<String, String> {
    req(j, k, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{path}.{k}: expected string"))
}

fn scope_json(s: DecisionScope) -> Json {
    Json::str(s.as_str())
}

fn scope_parse(s: &str, path: &str) -> Result<DecisionScope, String> {
    DecisionScope::parse(s).ok_or_else(|| format!("{path}: unknown decision scope {s}"))
}

fn choice_json(c: &ResponseChoice) -> Json {
    match c {
        ResponseChoice::AllowOnce => Json::str("allow_once"),
        ResponseChoice::Deny { reason } => {
            Json::obj([("deny", Json::obj([("reason", Json::str(reason.clone()))]))])
        }
        ResponseChoice::MoreInfo => Json::str("more_info"),
        ResponseChoice::AllowLease(spec) => Json::obj([(
            "allow_lease",
            Json::obj([
                ("scope", scope_json(spec.scope)),
                (
                    "max_uses",
                    spec.max_uses
                        .map(|u| Json::Int(u as i64))
                        .unwrap_or(Json::Null),
                ),
            ]),
        )]),
    }
}

fn choice_from_json(j: &Json, path: &str) -> Result<ResponseChoice, String> {
    match j {
        Json::Str(s) if s == "allow_once" => Ok(ResponseChoice::AllowOnce),
        Json::Str(s) if s == "more_info" => Ok(ResponseChoice::MoreInfo),
        Json::Obj(m) => {
            if let Some(d) = m.get("deny") {
                return Ok(ResponseChoice::Deny {
                    reason: req_str(d, "reason", path)?,
                });
            }
            if let Some(l) = m.get("allow_lease") {
                return Ok(ResponseChoice::AllowLease(LeaseSpec {
                    scope: scope_parse(&req_str(l, "scope", path)?, path)?,
                    max_uses: l.get("max_uses").and_then(Json::as_int).map(|i| i as u64),
                }));
            }
            Err(format!("{path}: unknown choice"))
        }
        _ => Err(format!("{path}: expected choice")),
    }
}

fn endorser_json(e: &EndorserRef) -> Json {
    match e {
        EndorserRef::Human {
            subject_ref,
            authority,
        } => Json::obj([
            ("human", Json::str(subject_ref.clone())),
            ("authority", Json::str(authority.as_str())),
        ]),
        EndorserRef::ApproverGrant { grant_ref } => {
            Json::obj([("approver_grant", Json::str(grant_ref.clone()))])
        }
    }
}

fn endorser_from_json(j: &Json, path: &str) -> Result<EndorserRef, String> {
    if let Some(g) = j.get("approver_grant").and_then(Json::as_str) {
        return Ok(EndorserRef::ApproverGrant {
            grant_ref: g.to_string(),
        });
    }
    let authority = match req_str(j, "authority", path)?.as_str() {
        "principal" => AuthorityClass::Principal,
        "delegate" => AuthorityClass::Delegate,
        other => return Err(format!("{path}.authority: unknown {other}")),
    };
    Ok(EndorserRef::Human {
        subject_ref: req_str(j, "human", path)?,
        authority,
    })
}

/// `security.permission.pending` — the durable owed-decision row's payload:
/// `{permission_id, effect_id, request, requested_at, mode, timeout}`
/// (§5g.7 §3 `{permission_id, request, requested_at}` + the attached effect;
/// coalesced pendings merge `effect_ids` at the fold).
pub fn pending_payload(r: &ApprovalRequest, effect_id: &str, requested_at: u64) -> Json {
    Json::obj([
        ("permission_id", Json::str(r.permission_id.clone())),
        ("effect_id", Json::str(effect_id.to_string())),
        (
            "request",
            Json::obj([
                ("subject_ref", Json::str(r.request.subject_ref.clone())),
                (
                    "capability_ref",
                    Json::obj([
                        (
                            "semantic_id",
                            Json::str(r.request.capability_ref.semantic_id.clone()),
                        ),
                        (
                            "version_id",
                            Json::str(r.request.capability_ref.version_id.clone()),
                        ),
                    ]),
                ),
                (
                    "args_canonical_hash",
                    Json::str(r.request.args_canonical_hash.clone()),
                ),
                ("reason", Json::str(r.request.reason.clone())),
            ]),
        ),
        ("requested_at", Json::Int(requested_at as i64)),
        ("mode", Json::str(r.mode.as_str())),
        (
            "timeout",
            r.timeout.map(|t| Json::Int(t as i64)).unwrap_or(Json::Null),
        ),
    ])
}

/// `security.permission.requested` — the ephemeral prompt-rendering row's
/// payload: `{permission_id, effect_id, options_presented, explanation, mode,
/// timeout, requested_at}` (the `rendering` member is the surface's and is
/// stripped from the durable form — the envelope carries the ephemeral-field
/// declaration).
pub fn requested_payload(r: &ApprovalRequest, effect_id: &str, requested_at: u64) -> Json {
    Json::obj([
        ("permission_id", Json::str(r.permission_id.clone())),
        ("effect_id", Json::str(effect_id.to_string())),
        (
            "options_presented",
            Json::Arr(
                r.options
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("id", Json::str(o.id.as_str())),
                            ("label", Json::str(o.label.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "explanation",
            Json::obj([
                ("display", Json::str(r.explanation.display.clone())),
                (
                    "rows",
                    Json::Arr(
                        r.explanation
                            .rows
                            .iter()
                            .map(|c| {
                                Json::obj([
                                    ("step", Json::Int(c.step as i64)),
                                    ("outcome", Json::str(c.outcome)),
                                    ("detail", Json::str(c.detail.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
        ("mode", Json::str(r.mode.as_str())),
        (
            "timeout",
            r.timeout.map(|t| Json::Int(t as i64)).unwrap_or(Json::Null),
        ),
        ("requested_at", Json::Int(requested_at as i64)),
        // `rendering` is the class's declared ephemeral member — the prompt
        // surface the durable row strips at commit (subscribers receive it
        // as a fragment frame). The kernel composes the canonical display;
        // the surface may still render its own.
        (
            "rendering",
            Json::obj([
                ("permission_id", Json::str(r.permission_id.clone())),
                ("display", Json::str(r.explanation.display.clone())),
            ]),
        ),
    ])
}

/// `ApprovalRequest` → the `security.permission.requested`/`pending` payload
/// members (the envelope adds scope/chain).
pub fn request_payload(r: &ApprovalRequest) -> Json {
    Json::obj([
        ("permission_id", Json::str(r.permission_id.clone())),
        ("subject_ref", Json::str(r.request.subject_ref.clone())),
        (
            "capability_ref",
            Json::obj([
                (
                    "semantic_id",
                    Json::str(r.request.capability_ref.semantic_id.clone()),
                ),
                (
                    "version_id",
                    Json::str(r.request.capability_ref.version_id.clone()),
                ),
            ]),
        ),
        (
            "args_canonical_hash",
            Json::str(r.request.args_canonical_hash.clone()),
        ),
        ("reason", Json::str(r.request.reason.clone())),
        (
            "options_presented",
            Json::Arr(
                r.options
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("id", Json::str(o.id.as_str())),
                            ("label", Json::str(o.label.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("mode", Json::str(r.mode.as_str())),
        (
            "timeout",
            r.timeout.map(|t| Json::Int(t as i64)).unwrap_or(Json::Null),
        ),
        (
            "explanation",
            Json::obj([
                ("display", Json::str(r.explanation.display.clone())),
                (
                    "rows",
                    Json::Arr(
                        r.explanation
                            .rows
                            .iter()
                            .map(|c| {
                                Json::obj([
                                    ("step", Json::Int(c.step as i64)),
                                    ("outcome", Json::str(c.outcome)),
                                    ("detail", Json::str(c.detail.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    ])
}

/// `ApprovalResponse` → the response record's payload members.
pub fn response_payload(r: &ApprovalResponse) -> Json {
    Json::obj([
        ("permission_id", Json::str(r.permission_id.clone())),
        ("choice", choice_json(&r.choice)),
        ("scope", scope_json(r.scope)),
        (
            "max_uses",
            r.max_uses
                .map(|u| Json::Int(u as i64))
                .unwrap_or(Json::Null),
        ),
        (
            "justification",
            r.justification
                .as_ref()
                .map(|s| Json::str(s.clone()))
                .unwrap_or(Json::Null),
        ),
        ("decided_by", endorser_json(&r.decided_by)),
        ("decided_at", Json::Int(r.decided_at as i64)),
    ])
}

/// Decode a `ResponseChoice`/`EndorserRef` pair — exported for the event-fold
/// tests (the decided-side decoder).
pub fn decode_choice(j: &Json) -> Result<ResponseChoice, String> {
    choice_from_json(j, "choice")
}

/// Decode an `EndorserRef`.
pub fn decode_endorser(j: &Json) -> Result<EndorserRef, String> {
    endorser_from_json(j, "decided_by")
}

#[cfg(test)]
mod tests;
