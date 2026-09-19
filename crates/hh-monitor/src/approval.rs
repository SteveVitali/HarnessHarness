//! # Approvals & escalation — the §5g.7 C1/Stage-2 slice (R-2.8.7)
//!
//! The approval request/response records (`PermissionRequest`, `ApprovalRequest`,
//! `ApprovalResponse`, `ApprovalOption`, `Explanation`), the durable `pending`
//! owed-decision row, the lease machinery (`ActionPattern`-projected keys at
//! `scope ∈ {turn, run, session}` — ADR-0071 D1), the narrowing-only reviewer
//! chain (`policy_rule → lease → hook → auto_reviewer → human` — all five
//! stages live at Stage 2, non-human stages raise-only), the never-auto set
//! (I-P1), the counting fold (I-P5), batching/coalescing, the repeated-denial
//! fallback, `ApproverGrant` legitimacy, and the `IllegitimateEndorsement`
//! legitimacy check (AC-R-2.8.7-3).
//!
//! Stage-2 additions (S2.6): `async`/`defer` request admission (the suspend/
//! wakeup append side is the dispatcher's — hh-env), `ActionPattern` leases
//! with `policy_fingerprint`/`risk_ceiling`/`origin_permission_id`, the hook
//! and `auto_reviewer` chain stages, `batch_id` grouping, the denial-ceiling
//! fallback, and the `ApproverGrant` record's legitimacy gate.
//!
//! What this module does NOT own (spec §5g.7 §9 + the decomposed manifest):
//! - the judge-variant `security.permission.reviewed` pipeline and
//!   `auto_review_fn_rate` calibration — C1/Stage 4;
//! - `grant_approver` organizational routing (§05i) — C1/Stage 4;
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

/// `ModelJustification` — the one free-text member an `Explanation` may carry
/// (AC-R-2.8.7-11): the proposer's model-authored reason, always rendered at
/// `authority = delegate` — surfaces render it with distinct provenance from
/// the kernel-derived members (T-LCD-02). The record shape exists so the
/// authority is *carried*, never implied.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelJustification {
    /// The text (`Text{authority = delegate}` — display only, never an input).
    pub text: String,
}

/// `Explanation` (§5g.7 §3) — the monitor's own explanation of why the ask
/// fired: `{display, rows[], model_justification?}` where `rows` reuse the
/// `CheckRecord` shape (the Π check trail — CC1, one row shape) and `display`
/// is kernel-rendered text derived from the closed tags (record-derived —
/// every rendered component except `model_justification` is kernel
/// provenance; AC-R-2.8.7-11).
#[derive(Debug, Clone, PartialEq)]
pub struct Explanation {
    /// The rendered display text (kernel-rendered from the closed check tags —
    /// distinct provenance from `model_justification`).
    pub display: String,
    /// The check-trail rows (`CheckRecord` — the same shape `KernelDecision`
    /// carries).
    pub rows: Vec<CheckRecord>,
    /// The model-authored justification, when the proposer attached one — the
    /// only free-text member (`Text{authority = delegate}`).
    pub model_justification: Option<ModelJustification>,
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
    /// The batch the dispatcher grouped this request under (§5g.7 §5 batching —
    /// record-derived: one batch prompt decides N pendings, each still
    /// producing its own `decided` row and endorsement count; `irreversible`
    /// requests never carry one).
    pub batch_id: Option<String>,
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
/// `{pattern?, scope, max_uses?}` (§5g.7 §3 `allow_lease{pattern, scope,
/// max_uses?}`; `max_uses` and a declared `pattern` are mandatory for
/// `irreversible` classes).
#[derive(Debug, Clone, PartialEq)]
pub struct LeaseSpec {
    /// The declared `ActionPattern` projection the lease keys on (`None` = the
    /// exact-hash lease — the whole canonical-args tuple).
    pub pattern: Option<ActionPattern>,
    /// The lease scope (`turn | run | session`).
    pub scope: LeaseScope,
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

/// `LeaseScope ∈ {turn, run, session}` — the lease's lifetime (§5g.7 §3).
/// `PersistenceScope` is the one lattice (CC1); `Definition`/`User`/`Project`
/// are refused at mint (`LeaseScopeViolation` — a persisted lease is a
/// `lifecycle.definition.changed` widening, never a lease).
pub type LeaseScope = hh_provenance::PersistenceScope;

/// `lease_scope_rank` — the looseness order over the admissible members
/// (`turn < run < session`); `PersistenceScope`'s derived `Ord` is the
/// declaration order (definition first), never the looseness — the rank is
/// the "scope ≤ run" test's basis.
pub fn lease_scope_rank(s: LeaseScope) -> u8 {
    match s {
        hh_provenance::PersistenceScope::Turn => 0,
        hh_provenance::PersistenceScope::Run => 1,
        hh_provenance::PersistenceScope::Session => 2,
        _ => 3,
    }
}

/// `ActionPattern` — the declared canonical-parameter projection a lease keys
/// on (§5g.7 §3 `pattern: ActionPattern`; §05d owns the projection rule —
/// ADR-0087/0100): the set of canonical `param_path`s the lease binds. Two
/// calls share the pattern lease iff their projections over `fields` agree —
/// every parameter *outside* the projection varies freely (that is the
/// declared domain the human approved; the soundness rule reads the
/// projection over canonical params only, never surface fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPattern {
    /// The canonical `param_path`s the projection binds (sorted — canonical).
    pub fields: std::collections::BTreeSet<String>,
}

impl ActionPattern {
    /// `pattern_id` — `H("action_pattern" ∥ sorted fields)`.
    pub fn pattern_id(&self) -> String {
        let mut fs: Vec<&str> = self.fields.iter().map(String::as_str).collect();
        fs.sort();
        idp::idp_id("action_pattern", fs.join("\x1f").as_bytes())
    }

    /// The projected-args hash over canonical `(param_path, canonical_value)`
    /// pairs — `H("pattern_args" ∥ pattern_id ∥ projected pairs)`. Params
    /// outside `fields` never enter the hash.
    pub fn projection_hash(
        &self,
        canonical_args: &std::collections::BTreeMap<String, String>,
    ) -> String {
        let mut buf = self.pattern_id();
        for f in &self.fields {
            if let Some(v) = canonical_args.get(f) {
                buf.push('\x1f');
                buf.push_str(f);
                buf.push('=');
                buf.push_str(v);
            }
        }
        idp::idp_id("pattern_args", buf.as_bytes())
    }

    /// The lease-key material for a projected lease —
    /// `pattern:{pattern_id}:{pattern_args_hash}` (never `effect_id`).
    pub fn key_material(
        &self,
        canonical_args: &std::collections::BTreeMap<String, String>,
    ) -> String {
        format!(
            "pattern:{}:{}",
            self.pattern_id(),
            self.projection_hash(canonical_args)
        )
    }

    /// The canonical payload form — `{pattern_id, fields[]}`.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("pattern_id", Json::str(self.pattern_id())),
            (
                "fields",
                Json::Arr(self.fields.iter().map(|f| Json::str(f.clone())).collect()),
            ),
        ])
    }

    /// Parse the canonical payload form — `None` on a bad member.
    pub fn from_json(j: &Json) -> Option<ActionPattern> {
        let fields = match j.get("fields")? {
            Json::Arr(fs) => fs
                .iter()
                .map(|f| f.as_str().map(String::from))
                .collect::<Option<std::collections::BTreeSet<_>>>()?,
            _ => return None,
        };
        Some(ActionPattern { fields })
    }
}

/// `policy_fingerprint` — the lease key's policy leg (ADR-0071 D1;
/// AC-R-2.8.7-4): `H(sealed_definition_version ∥ approval_mode ∥
/// narrowing_leaf_ids)`. Any change to the sealed definition version, the
/// declared approval mode, or any Π narrowing leaf is a new fingerprint —
/// every lease minted under the old one misses by construction and the
/// `lease.revoked{reason: fingerprint}` row records the revocation.
pub fn policy_fingerprint(
    sealed_definition_version: &str,
    approval_mode: &str,
    narrowing_leaf_ids: &[String],
) -> String {
    let mut leaves = narrowing_leaf_ids.to_vec();
    leaves.sort();
    idp::idp_id(
        "policy_fingerprint",
        format!(
            "{}\x1f{}\x1f{}",
            sealed_definition_version,
            approval_mode,
            leaves.join("\x1f")
        )
        .as_bytes(),
    )
}

/// `ApprovalLease` (§5g.7 §3): `{lease_id, key, capability_ref, pattern?,
/// args_canonical_hash | pattern_args_hash, scope ∈ {turn,run,session},
/// holder, granted_by(basis), origin_permission_id, max_uses?, uses,
/// policy_fingerprint, risk_ceiling, grant_authority, granted_at,
/// revoked_at?}` (ADR-0071 D1). The key hashes `capability_ref ∥
/// key_material ∥ scope ∥ policy_fingerprint` where `key_material` is the
/// exact canonical-args hash for a `pattern = None` lease or
/// `pattern:{pattern_id}:{pattern_args_hash}` for a projected lease; any
/// policy change revokes by construction. `uses` counts served hits;
/// `max_uses` bounds them (`irreversible` leases must carry a declared
/// pattern, a human basis, `max_uses` and `scope ≤ run`).
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalLease {
    /// The lease id (the content key — `lease_key`).
    pub lease_id: String,
    /// The key hash the lease covers.
    pub key_hash: String,
    /// The capability the lease covers.
    pub capability_ref: PinnedRef,
    /// The args hash an exact lease covers (`pattern = None`).
    pub args_canonical_hash: String,
    /// The declared `ActionPattern` projection, when the lease keys on one.
    pub pattern: Option<ActionPattern>,
    /// The projected-args hash a pattern lease covers.
    pub pattern_args_hash: Option<String>,
    /// The lease scope (`turn | run | session`).
    pub scope: LeaseScope,
    /// The concrete scope coordinate the lease lives in (the run id /
    /// session ref / turn id it was granted under).
    pub scope_ref: String,
    /// The holder (the proposer's semantic id — sub-agents receive only
    /// attenuated copies, never the principal's lease).
    pub holder: String,
    /// Who/what granted the lease.
    pub basis: LeaseBasis,
    /// The `permission_id` the lease descends from (the `origin_permission_id`
    /// a `decider = cache` decision carries).
    pub origin_permission_id: String,
    /// The fingerprint the lease was minted under — a Π/mode change revokes
    /// by key construction; this member records it for the audit row.
    pub policy_fingerprint: String,
    /// The `effective_risk_class` ceiling the lease serves — a hit requires
    /// `risk.leq_danger(risk_ceiling)` (AC-R-2.8.7-4).
    pub risk_ceiling: hh_ontology::risk::RiskClass,
    /// The `context_label.authority` at grant — `scope = external` leases are
    /// re-validated: a per-call authority below this misses the lease.
    pub grant_authority: AuthorityClass,
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

/// The lease key (ADR-0071 D1 — `H(capability_ref ∥ key_material ∥
/// scope ∥ policy_fingerprint)`): a policy change revokes by construction.
/// `key_material` is the exact canonical-args hash for an exact lease or
/// `pattern:{pattern_id}:{pattern_args_hash}` for an `ActionPattern` lease —
/// never `effect_id` (§5g.1 step 8). `scope` enters the hash so a `turn`
/// lease never serves a `run` lookup.
pub fn lease_key(
    capability_ref: &PinnedRef,
    key_material: &str,
    scope: LeaseScope,
    policy_fingerprint: &str,
) -> String {
    idp::idp_id(
        "approval_lease",
        format!(
            "{}\x1f{}\x1f{}\x1f{}",
            capability_ref.version_id,
            key_material,
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
/// consumes (§5g.7 §4 / §3's record: every member a ledger or sealed-definition
/// record, never free text). `run_chain` is pure over records.
#[derive(Debug, Clone, PartialEq)]
pub struct EscalationInput {
    /// The effect the Π `ask` fired for.
    pub effect_id: String,
    /// The run's scope coordinate the lease lookup binds (the run id for
    /// `scope = run` leases — the record-derived coordinate, never guessed).
    pub scope_ref: String,
    /// The capability the proposal names.
    pub capability_ref: PinnedRef,
    /// The args hash (the exact-hash lease leg).
    pub args_canonical_hash: String,
    /// The declared `ActionPattern` projections' key materials the caller
    /// computed — `pattern:{pattern_id}:{pattern_args_hash}` strings; the
    /// lease stage's pattern legs.
    pub pattern_keys: Vec<String>,
    /// The effect domain (never-auto: Π-8 reads it).
    pub domain: hh_hir::kinds::EffectDomain,
    /// The effective risk class (lease `risk_ceiling`; never-auto Π-7).
    pub risk: hh_ontology::risk::RiskClass,
    /// `min(authority(proposer), context_label.authority)` — Π-7's never-auto
    /// leg and the `scope = external` lease re-validation read it.
    pub eff: AuthorityClass,
    /// The proposer's semantic id (the lease `holder` match).
    pub holder: String,
    /// Whether the class is `irreversible` (never batched; lease requires a
    /// declared pattern, a human basis, `max_uses` and `scope ≤ run`).
    pub irreversible: bool,
    /// The run's attendance (`attended | unattended` — Π-12's mode leg).
    pub mode: Mode,
    /// Π-12's `UnattendedPolicy` — `deny | defer | auto_review`.
    pub unattended_policy: crate::policy::UnattendedPolicy,
    /// The run's declared approval mode (fingerprint leg; `async` defers the
    /// human stage).
    pub approval_mode: ApprovalMode,
    /// The Π fingerprint (the lease key's policy leg).
    pub policy_fingerprint: String,
    /// Approvals already consumed this run (the exhaustion input).
    pub approvals_used: u64,
    /// The approvals ceiling (`None` = unlimited).
    pub approvals_max: Option<u64>,
    /// Never-auto legs beyond the class: the sealed definition carried an
    /// explicit `ask` leaf for this effect's pattern (I-P1).
    pub explicit_ask: bool,
    /// The effect is itself a `message_human` consent step (I-P1).
    pub consent_step: bool,
    /// The covering `Permission` is revoked or stale (I-P1).
    pub revoked_or_stale: bool,
    /// The effect is a `scope = user` persistence endorsement (I-P1).
    pub user_scope_persistence: bool,
    /// The batching admit — the run's batch policy groups this class
    /// (`irreversible` is never batched regardless).
    pub batchable: bool,
    /// The per-call `context_label.authority` — the `scope = external` lease
    /// re-validation compares it against the lease's `grant_authority`
    /// (ADR-0071 D1).
    pub context_authority: AuthorityClass,
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
    /// Π-12 `defer` or an `async` approval mode — the run suspends on
    /// `awaiting_approval` until the `permission_decided` wakeup lands.
    Defer,
    /// A sealed `auto_review` rule's verdict resolved the ask (`policy_rule`
    /// endorsement — I-P2).
    AutoReviewed,
    /// An attested hook resolved the ask (deny — a hook `allow` is never
    /// itself an endorsement; it feeds the `auto_reviewer` stage, I-P2).
    HookResolved,
    /// The pending timed out or was cancelled — a refusal, never `unknown`.
    Refused,
}

impl EscalationReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EscalationReason::LeaseHit => "lease_hit",
            EscalationReason::EscalateToHuman => "escalate_to_human",
            EscalationReason::ApprovalsExhausted => "approvals_exhausted",
            EscalationReason::UnattendedAsk => "unattended_ask",
            EscalationReason::Defer => "defer",
            EscalationReason::AutoReviewed => "auto_reviewed",
            EscalationReason::HookResolved => "hook_resolved",
            EscalationReason::Refused => "refused",
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

// ── The reviewer chain (§5g.7 §4; all five stages live at Stage 2) ───────────

/// The never-auto predicate over the full I-P1 set (§5g.7 I-P1): returns the
/// closed member tag when the effect is constitutionally non-automatic —
/// Π-8 (`permission_request` domain), `irreversible ∨ external-scope` under
/// Π-7, revoked/stale permission, explicit sealed `ask` leaves, consent-step
/// `message_human`, `scope = user` persistence endorsements. A `Some` result
/// skips every non-human stage (AC-R-2.8.7-2).
pub fn never_auto_member(input: &EscalationInput) -> Option<&'static str> {
    use hh_hir::kinds::EffectDomain;
    use hh_ontology::risk::RiskScope;
    if input.domain == EffectDomain::PermissionRequest {
        return Some("pi_8");
    }
    if input.eff <= AuthorityClass::External
        && (input.irreversible || input.risk.scope == RiskScope::External)
    {
        return Some("pi_7_irreversible_external");
    }
    if input.revoked_or_stale {
        return Some("permission_revoked_or_stale");
    }
    if input.explicit_ask {
        return Some("explicit_ask");
    }
    if input.consent_step {
        return Some("consent_step");
    }
    if input.user_scope_persistence {
        return Some("user_scope_persistence");
    }
    None
}

/// `ReviewStage` — the fixed chain order `policy_rule → lease → hook →
/// auto_reviewer → human` (ADR-0069 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewStage {
    /// The `definition`-issued pre-authorization (ADR-0053 D5) — ran at
    /// `authorize` (a covering `policy_rule`-basis handle); reaching the chain
    /// means it passed.
    PolicyRule,
    /// The `ApprovalLease` lookup.
    Lease,
    /// The attested deterministic hooks (R-2.8.5 — `EscalationInput` in,
    /// `{allow, deny, ask, defer}` out).
    Hook,
    /// The sealed `HarnessRule{kind: auto_review}` execution (I-P2 — the only
    /// path that turns a hook/judge `allow` into an endorsement).
    AutoReviewer,
    /// The human principal (or a reviewer under an `ApproverGrant`).
    Human,
}

impl ReviewStage {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewStage::PolicyRule => "policy_rule",
            ReviewStage::Lease => "lease",
            ReviewStage::Hook => "hook",
            ReviewStage::AutoReviewer => "auto_reviewer",
            ReviewStage::Human => "human",
        }
    }
}

/// `StageOutcome` — one chain stage's result: `resolve(allow|deny) | pass`
/// (ADR-0069 D4). Non-human stages may only *narrow* — an `allow` they
/// produce is never a widening (the ask was already gated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOutcome {
    /// The stage resolved the ask to `allow`.
    ResolveAllow,
    /// The stage resolved the ask to `deny`.
    ResolveDeny,
    /// The stage passed the ask onward.
    Pass,
}

impl StageOutcome {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            StageOutcome::ResolveAllow => "allow",
            StageOutcome::ResolveDeny => "deny",
            StageOutcome::Pass => "pass",
        }
    }
}

/// `StageOutput` — one stage's recorded output (the trace predicate
/// AC-R-2.8.7-2 reads: `allow` never follows `deny` in the sequence, and no
/// non-human stage resolves `allow` for a never-auto member).
#[derive(Debug, Clone, PartialEq)]
pub struct StageOutput {
    /// The stage.
    pub stage: ReviewStage,
    /// The outcome.
    pub outcome: StageOutcome,
    /// The stage ref (hook/validator/rule ref, or the never-auto member tag).
    pub stage_ref: String,
}

/// `HookVerdict` — the attested hook's closed return `{allow | deny | ask |
/// defer | error}` (the R-2.8.5 hook projection). `allow` is an *input to a
/// `policy_rule`* (I-P2) — it never endorses by itself; `error`/timeout is
/// `pass` for `read_only`/`reversible`, `deny` otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookVerdict {
    /// The hook allows (feeds the `auto_review` rule stage only — I-P2).
    Allow,
    /// The hook denies.
    Deny,
    /// The hook escalates (`ask` — pass onward).
    Ask,
    /// The hook defers (the run suspends — the `defer` mode marker).
    Defer,
    /// The hook errored or timed out (stage error rule).
    Error,
}

/// `HookReport` — the attested hook's *recorded* verdict: `{hook_ref,
/// attestation_ref, verdict}`. The hook itself runs outside the TCB; the
/// report is what the chain consumes (R-2.8.5 attestation — the
/// `attestation_ref` is the admission record).
#[derive(Debug, Clone, PartialEq)]
pub struct HookReport {
    /// The hook's identity coordinate.
    pub hook_ref: String,
    /// The R-2.8.5 attestation record ref.
    pub attestation_ref: String,
    /// The recorded verdict.
    pub verdict: HookVerdict,
}

/// `AutoReviewRule` — the sealed `HarnessRule{kind: auto_review}` the kernel
/// executes (I-P2): `{rule_ref, domains?, max_risk, eff_at_most?, verdict}`.
/// A matching rule resolves the ask with basis `policy_rule`; a hook `allow`
/// without a matching rule is a `pass` — the hook never endorses alone.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoReviewRule {
    /// The sealed rule's coordinate (`basis_ref`'s rule leg).
    pub rule_ref: String,
    /// The domains the rule covers (empty = every domain below the ceilings).
    pub domains: Vec<hh_hir::kinds::EffectDomain>,
    /// The risk ceiling the rule serves (`risk.leq_danger(max_risk)`).
    pub max_risk: hh_ontology::risk::RiskClass,
    /// The authority bound the rule serves (`eff ≤` it when declared).
    pub eff_at_most: Option<AuthorityClass>,
    /// The conditioned verdict the rule carries.
    pub verdict: ReviewVerdictKind,
}

/// `ChainDecision` — the chain's resolution: what the dispatcher records.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainDecision {
    /// Resolved `allow` — `decider` (`cache` for a lease hit, `auto_reviewer`
    /// for a sealed rule) + the endorsement basis.
    Allow {
        /// The producing stage's decider tag.
        decider: crate::decision::Decider,
        /// The lease key served (when `decider = cache`).
        cache_key: Option<String>,
        /// The endorsement basis the `label.endorsed`/`lease` rows name.
        basis: LeaseBasis,
        /// The lease the hit consumed (for `lease.used` emission).
        lease_id: Option<String>,
        /// The `origin_permission_id` the lease descends from.
        origin_permission_id: Option<String>,
    },
    /// Resolved `deny`.
    Deny {
        /// The closed reason.
        reason: DenyReason,
        /// The stage that denied (`hook` deny / unattended Π-12 / exhaustion).
        stage: ReviewStage,
    },
    /// The human stage — `sync`: render the request.
    AskHuman {
        /// Who the ask escalates to.
        target: ReviewerRef,
    },
    /// Defer — `lifecycle.run.suspended{awaiting_approval}` + the
    /// `permission_decided` wakeup (the dispatcher appends; the chain decides).
    Defer {
        /// Who the deferred ask escalates to.
        target: ReviewerRef,
    },
}

/// `ChainOutcome` — `run_chain`'s result: the stage-output sequence (the
/// AC-R-2.8.7-2 trace predicate reads it) + the escalations to record +
/// the decision.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainOutcome {
    /// The per-stage outputs in chain order (one per stage that ran — a
    /// never-auto member's skipped non-human stages are recorded `pass` with
    /// the member tag as `stage_ref`).
    pub stages: Vec<StageOutput>,
    /// The `security.permission.escalated{from_stage, to_stage, reason}` rows
    /// the dispatcher emits (one per stage hop that passed the ask onward).
    pub escalations: Vec<(ReviewStage, ReviewStage, String)>,
    /// The decision.
    pub decision: ChainDecision,
}

/// Whether a lease serves this input — the full hit predicate (ADR-0071 D1;
/// AC-R-2.8.7-4): capability + scope-coordinate + holder match; live and
/// under `max_uses`; `risk ≤ risk_ceiling`; a `scope = external` risk
/// re-validates `context_label.authority` against the grant-time value;
/// `irreversible` requires a declared pattern, a human basis, `max_uses` and
/// `scope ≤ run`.
pub fn lease_serves(lease: &ApprovalLease, input: &EscalationInput) -> bool {
    if lease.capability_ref.version_id != input.capability_ref.version_id {
        return false;
    }
    if lease.revoked_at.is_some()
        || lease.max_uses.map(|m| lease.uses >= m).unwrap_or(false)
        || lease.holder != input.holder
        || !input.risk.leq_danger(&lease.risk_ceiling)
    {
        return false;
    }
    if lease.scope_ref != input.scope_ref {
        return false;
    }
    // The scope = external re-validation (ADR-0071 D1): a per-call context
    // authority below the grant-time value misses the lease.
    if input.risk.scope == hh_ontology::risk::RiskScope::External
        && input.context_authority < lease.grant_authority
    {
        return false;
    }
    // `irreversible` — human basis, declared pattern, bounded, `scope ≤ run`.
    if input.irreversible {
        let human = matches!(lease.basis, LeaseBasis::Human);
        let bounded = lease.max_uses.is_some();
        let patterned = lease.pattern.is_some();
        let scoped = lease_scope_rank(lease.scope) <= lease_scope_rank(LeaseScope::Run);
        if !(human && bounded && patterned && scoped) {
            return false;
        }
    }
    true
}

/// The lease key materials this input looks up: the exact-args leg plus each
/// declared pattern leg.
pub fn lease_candidates(input: &EscalationInput) -> Vec<String> {
    let mut out = vec![input.args_canonical_hash.clone()];
    out.extend(input.pattern_keys.iter().cloned());
    out
}

/// `run_chain(input, leases, hooks, rules, lease_scope)` — the §5g.7 §4
/// reviewer chain, all five stages, narrowing-only. Pure over records: hooks
/// arrive as *recorded* `HookReport`s (the hook runner is outside the TCB);
/// the `auto_review` rules are sealed records the caller read out of the
/// definition; leases are the `ApprovalState` projection.
///
/// Order and rules (ADR-0069 D4, I-P1/I-P2):
/// 1. `policy_rule` — the pre-authorization already ran at `authorize`
///    (reaching the chain means it passed); recorded `pass`.
/// 2. Never-auto (I-P1) — a member skips every non-human stage.
/// 3. Π-12 — `unattended`: `deny` | `defer` | `auto_review` (the auto_review
///    policy routes to the `auto_reviewer` stage only; a never-auto member
///    under `auto_review` denies — fail-closed).
/// 4. `lease` — a serving lease resolves `allow` (`decider = cache`).
/// 5. `hook` — verdicts in order: `deny` resolves; `defer` resolves defer;
///    `allow` marks a hook-allow and passes to `auto_reviewer`; `ask` passes;
///    `error` passes for `read_only`/`reversible`, denies otherwise.
/// 6. `auto_reviewer` — a matching sealed rule resolves `allow`
///    (`decider = auto_reviewer`, basis `policy_rule`); Π-12's `auto_review`
///    unattended policy without a matching rule denies (fail-closed).
/// 7. `human` — `async` mode defers; `sync` produces the `ask`.
pub fn run_chain(
    input: &EscalationInput,
    leases: &BTreeMap<String, ApprovalLease>,
    hooks: &[HookReport],
    rules: &[AutoReviewRule],
    lease_scope: LeaseScope,
) -> ChainOutcome {
    use hh_ontology::risk::RiskReversibility;
    let mut stages: Vec<StageOutput> = Vec::new();
    let mut escalations: Vec<(ReviewStage, ReviewStage, String)> = Vec::new();

    // Stage 1 — `policy_rule`: the pre-authorization check ran at `authorize`
    // (a covering `policy_rule`-basis handle resolved `ask → allow` upstream);
    // reaching the chain means it passed.
    stages.push(StageOutput {
        stage: ReviewStage::PolicyRule,
        outcome: StageOutcome::Pass,
        stage_ref: "pre_authorization".to_string(),
    });

    // Never-auto (I-P1) — the constitutional set skips every non-human stage.
    let na = never_auto_member(input);

    // Π-12 — unattended: `deny | defer | auto_review` before the chain runs
    // (AC-R-2.8.7-5: one decision step; a `defer` under unattended suspends —
    // the never-auto set excludes only *automatic resolution*, not the wait).
    if input.mode == Mode::Unattended {
        match input.unattended_policy {
            crate::policy::UnattendedPolicy::Deny => {
                stages.push(StageOutput {
                    stage: ReviewStage::Human,
                    outcome: StageOutcome::ResolveDeny,
                    stage_ref: "pi_12".to_string(),
                });
                return ChainOutcome {
                    stages,
                    escalations,
                    decision: ChainDecision::Deny {
                        reason: DenyReason::UnattendedAsk,
                        stage: ReviewStage::Human,
                    },
                };
            }
            crate::policy::UnattendedPolicy::Defer => {
                stages.push(StageOutput {
                    stage: ReviewStage::Human,
                    outcome: StageOutcome::Pass,
                    stage_ref: "pi_12:defer".to_string(),
                });
                return ChainOutcome {
                    stages,
                    escalations,
                    decision: ChainDecision::Defer {
                        target: ReviewerRef::Human {
                            principal_ref: "principal".to_string(),
                        },
                    },
                };
            }
            crate::policy::UnattendedPolicy::AutoReview => {
                // Falls through to the auto_reviewer stage below (never-auto
                // members deny instead — Π-12 excludes them).
                if let Some(m) = na {
                    stages.push(StageOutput {
                        stage: ReviewStage::AutoReviewer,
                        outcome: StageOutcome::ResolveDeny,
                        stage_ref: m.to_string(),
                    });
                    return ChainOutcome {
                        stages,
                        escalations,
                        decision: ChainDecision::Deny {
                            reason: DenyReason::UnattendedAsk,
                            stage: ReviewStage::AutoReviewer,
                        },
                    };
                }
            }
        }
    }

    // The exhaustion conversion (record-level — `hh-budget::resolve_ask` owns
    // the reservation; an exhausted budget never reaches a reviewer).
    if let Some(max) = input.approvals_max {
        if input.approvals_used >= max {
            return ChainOutcome {
                stages,
                escalations,
                decision: ChainDecision::Deny {
                    reason: DenyReason::ApprovalsExhausted,
                    stage: ReviewStage::Human,
                },
            };
        }
    }

    // Stage 2 — `lease`: the exact leg plus each declared pattern leg.
    let mut lease_hit: Option<(String, ApprovalLease)> = None;
    if na.is_none() {
        for material in lease_candidates(input) {
            let key = lease_key(
                &input.capability_ref,
                &material,
                lease_scope,
                &input.policy_fingerprint,
            );
            if let Some(l) = leases.get(&key) {
                if lease_serves(l, input) {
                    lease_hit = Some((key, l.clone()));
                    break;
                }
            }
        }
    } else if let Some(m) = na {
        stages.push(StageOutput {
            stage: ReviewStage::Lease,
            outcome: StageOutcome::Pass,
            stage_ref: m.to_string(),
        });
    }
    if let Some((key, lease)) = lease_hit {
        stages.push(StageOutput {
            stage: ReviewStage::Lease,
            outcome: StageOutcome::ResolveAllow,
            stage_ref: key.clone(),
        });
        return ChainOutcome {
            stages,
            escalations,
            decision: ChainDecision::Allow {
                decider: crate::decision::Decider::Cache,
                cache_key: Some(key),
                basis: lease.basis.clone(),
                lease_id: Some(lease.lease_id.clone()),
                origin_permission_id: Some(lease.origin_permission_id.clone()),
            },
        };
    }
    if na.is_none() {
        stages.push(StageOutput {
            stage: ReviewStage::Lease,
            outcome: StageOutcome::Pass,
            stage_ref: "miss".to_string(),
        });
        escalations.push((
            ReviewStage::Lease,
            ReviewStage::Hook,
            "no_lease".to_string(),
        ));
    }

    // Stage 3 — `hook`: verdicts in order (the attested set). `allow` marks
    // the hook-allow the `auto_review` stage consumes (I-P2 — the hook alone
    // never endorses).
    let mut hook_allow: Option<String> = None;
    let mut hook_defer = false;
    if let Some(m) = na {
        stages.push(StageOutput {
            stage: ReviewStage::Hook,
            outcome: StageOutcome::Pass,
            stage_ref: m.to_string(),
        });
    } else {
        for h in hooks {
            match h.verdict {
                HookVerdict::Deny => {
                    stages.push(StageOutput {
                        stage: ReviewStage::Hook,
                        outcome: StageOutcome::ResolveDeny,
                        stage_ref: h.hook_ref.clone(),
                    });
                    return ChainOutcome {
                        stages,
                        escalations,
                        decision: ChainDecision::Deny {
                            reason: DenyReason::PolicyDenied,
                            stage: ReviewStage::Hook,
                        },
                    };
                }
                HookVerdict::Defer => hook_defer = true,
                HookVerdict::Allow => {
                    hook_allow.get_or_insert_with(|| h.hook_ref.clone());
                }
                HookVerdict::Ask => {}
                HookVerdict::Error => {
                    let soft = input.risk.reversibility == RiskReversibility::ReadOnly
                        || input.risk.reversibility == RiskReversibility::Reversible;
                    if !soft {
                        stages.push(StageOutput {
                            stage: ReviewStage::Hook,
                            outcome: StageOutcome::ResolveDeny,
                            stage_ref: format!("{}:error", h.hook_ref),
                        });
                        return ChainOutcome {
                            stages,
                            escalations,
                            decision: ChainDecision::Deny {
                                reason: DenyReason::PolicyDenied,
                                stage: ReviewStage::Hook,
                            },
                        };
                    }
                }
            }
        }
        stages.push(StageOutput {
            stage: ReviewStage::Hook,
            outcome: StageOutcome::Pass,
            stage_ref: hook_allow
                .clone()
                .map(|r| format!("allow:{r}"))
                .unwrap_or_else(|| "none".to_string()),
        });
    }
    if hook_defer {
        return ChainOutcome {
            stages,
            escalations,
            decision: ChainDecision::Defer {
                target: ReviewerRef::Human {
                    principal_ref: "principal".to_string(),
                },
            },
        };
    }
    escalations.push((
        ReviewStage::Hook,
        ReviewStage::AutoReviewer,
        "hook_pass".to_string(),
    ));

    // Stage 4 — `auto_reviewer`: a matching sealed `HarnessRule{auto_review}`
    // resolves with basis `policy_rule` (I-P2). A hook `allow` without a
    // matching rule is a pass — the hook never endorses alone. Π-12's
    // `auto_review` unattended policy without a matching rule denies
    // (fail-closed).
    let rule_match = |r: &AutoReviewRule| {
        (r.domains.is_empty() || r.domains.contains(&input.domain))
            && input.risk.leq_danger(&r.max_risk)
            && r.eff_at_most.map(|b| input.eff <= b).unwrap_or(true)
    };
    let wants_auto = hook_allow.is_some()
        || (input.mode == Mode::Unattended
            && input.unattended_policy == crate::policy::UnattendedPolicy::AutoReview);
    let matched: Option<&AutoReviewRule> = if na.is_none() && wants_auto {
        rules.iter().find(|r| rule_match(r))
    } else {
        None
    };
    match matched {
        Some(rule) => {
            let outcome = match rule.verdict {
                ReviewVerdictKind::Allow => StageOutcome::ResolveAllow,
                ReviewVerdictKind::Deny | ReviewVerdictKind::Amend => StageOutcome::ResolveDeny,
            };
            stages.push(StageOutput {
                stage: ReviewStage::AutoReviewer,
                outcome,
                stage_ref: rule.rule_ref.clone(),
            });
            return ChainOutcome {
                stages,
                escalations,
                decision: match rule.verdict {
                    ReviewVerdictKind::Allow => ChainDecision::Allow {
                        decider: crate::decision::Decider::AutoReviewer,
                        cache_key: None,
                        basis: LeaseBasis::PolicyRule {
                            rule_ref: rule.rule_ref.clone(),
                            verdict: rule.verdict,
                        },
                        lease_id: None,
                        origin_permission_id: None,
                    },
                    _ => ChainDecision::Deny {
                        reason: DenyReason::PolicyDenied,
                        stage: ReviewStage::AutoReviewer,
                    },
                },
            };
        }
        None => {
            stages.push(StageOutput {
                stage: ReviewStage::AutoReviewer,
                outcome: StageOutcome::Pass,
                stage_ref: na
                    .map(str::to_string)
                    .unwrap_or_else(|| "no_rule".to_string()),
            });
            if input.mode == Mode::Unattended
                && input.unattended_policy == crate::policy::UnattendedPolicy::AutoReview
            {
                return ChainOutcome {
                    stages,
                    escalations,
                    decision: ChainDecision::Deny {
                        reason: DenyReason::UnattendedAsk,
                        stage: ReviewStage::AutoReviewer,
                    },
                };
            }
            escalations.push((
                ReviewStage::AutoReviewer,
                ReviewStage::Human,
                "no_rule".to_string(),
            ));
        }
    }

    // Stage 5 — `human`: `async` defers (the run suspends on the pending);
    // `sync` renders the ask.
    let target = ReviewerRef::Human {
        principal_ref: "principal".to_string(),
    };
    let decision = match input.approval_mode {
        ApprovalMode::Async => ChainDecision::Defer {
            target: target.clone(),
        },
        _ => ChainDecision::AskHuman {
            target: target.clone(),
        },
    };
    stages.push(StageOutput {
        stage: ReviewStage::Human,
        outcome: StageOutcome::Pass,
        stage_ref: match &decision {
            ChainDecision::Defer { .. } => "defer".to_string(),
            _ => "ask".to_string(),
        },
    });
    ChainOutcome {
        stages,
        escalations,
        decision,
    }
}

/// The chain's legacy adapter — `escalate(input, leases)` runs [`run_chain`]
/// with no hooks, no auto-review rules and the lease stage at `scope = run`
/// (the Stage-1 call shape, kept for the pre-existing call sites).
pub fn escalate(
    input: &EscalationInput,
    leases: &BTreeMap<String, ApprovalLease>,
) -> EscalationDecision {
    let out = run_chain(input, leases, &[], &[], LeaseScope::Run);
    match out.decision {
        ChainDecision::Allow { cache_key, .. } => EscalationDecision {
            decision: Decision::Allow,
            target: None,
            reason: EscalationReason::LeaseHit,
            cache_key,
            lease_hit: true,
        },
        ChainDecision::Deny { reason, .. } => EscalationDecision {
            decision: Decision::Deny {
                reason,
                remedies: Vec::new(),
            },
            target: None,
            reason: match reason {
                DenyReason::ApprovalsExhausted => EscalationReason::ApprovalsExhausted,
                DenyReason::UnattendedAsk => EscalationReason::UnattendedAsk,
                _ => EscalationReason::HookResolved,
            },
            cache_key: None,
            lease_hit: false,
        },
        ChainDecision::Defer { target } => EscalationDecision {
            decision: Decision::Ask {
                options: vec!["allow_once".to_string(), "deny".to_string()],
                remedies: Vec::new(),
            },
            target: Some(target),
            reason: EscalationReason::Defer,
            cache_key: None,
            lease_hit: false,
        },
        ChainDecision::AskHuman { target } => EscalationDecision {
            decision: Decision::Ask {
                options: vec!["allow_once".to_string(), "deny".to_string()],
                remedies: Vec::new(),
            },
            target: Some(target),
            reason: EscalationReason::EscalateToHuman,
            cache_key: Some(lease_key(
                &input.capability_ref,
                &input.args_canonical_hash,
                LeaseScope::Run,
                &input.policy_fingerprint,
            )),
            lease_hit: false,
        },
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
    /// The effects the resolved pending covered — the re-dispatch linkage:
    /// a resumed effect re-deriving `ask` is served by this row
    /// (`decision_for_effect`), never re-asked.
    pub effect_ids: Vec<String>,
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

/// `ApproverGrant` — the §5g.7 §4 delegated-approval record (ADR-0070; OQ-177):
/// `{grant_ref, grantee, capability_prefixes[], max_risk, expires_at?,
/// revoked_at?}`. A response `decided_by = approver_grant(grant_ref)` is
/// legitimate only when a live grant covers the pending's capability and the
/// decided risk (AC-R-2.8.7-3 — the grant *record* confers, never a name).
#[derive(Debug, Clone, PartialEq)]
pub struct ApproverGrant {
    /// The grant's identity coordinate.
    pub grant_ref: String,
    /// The principal who delegated (the grant's issuer).
    pub grantor: String,
    /// The reviewer the grant empowers.
    pub grantee: String,
    /// The capability `semantic_id` prefixes the grant covers (empty = all).
    pub capability_prefixes: Vec<String>,
    /// The risk ceiling the grant covers (`risk.leq_danger(max_risk)`).
    pub max_risk: hh_ontology::risk::RiskClass,
    /// When the grant expires (logical time; `None` = the run).
    pub expires_at: Option<u64>,
    /// When the grant was revoked, if it was.
    pub revoked_at: Option<u64>,
}

impl ApproverGrant {
    /// Whether this live grant covers `(capability_semantic_id, risk)` at
    /// `now` — the respond-path legitimacy test (AC-R-2.8.7-3).
    pub fn covers(
        &self,
        capability_semantic_id: &str,
        risk: &hh_ontology::risk::RiskClass,
        now: u64,
    ) -> bool {
        if self.revoked_at.is_some() || self.expires_at.map(|e| now > e).unwrap_or(false) {
            return false;
        }
        if !risk.leq_danger(&self.max_risk) {
            return false;
        }
        self.capability_prefixes.is_empty()
            || self
                .capability_prefixes
                .iter()
                .any(|p| capability_semantic_id.starts_with(p.as_str()))
    }
}

/// `DenialFallback` — the closed repeated-denial fallback set (§5g.7 §5's
/// typed fallback — never a widening, never an unbounded silent retry):
/// `escalate` re-renders the ask to the human stage once more, `stop_run`
/// suspends the run as `refused`, `refuse_class` denies every later pending
/// for the same `(capability_ref, args_canonical_hash)` key without a surface
/// trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialFallback {
    /// Re-escalate once (the denial was likely mis-keyed).
    Escalate,
    /// Stop the run (`refused{denials_exhausted}`).
    StopRun,
    /// Refuse the class for the run (record-derived — no new surface trip).
    RefuseClass,
}

impl DenialFallback {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DenialFallback::Escalate => "escalate",
            DenialFallback::StopRun => "stop_run",
            DenialFallback::RefuseClass => "refuse_class",
        }
    }
}

/// `DenialPolicy` — the sealed definition's repeated-denial policy:
/// `{max_denials, fallback}` (§5g.7 §5; `max_denials` counts `decided{deny}`
/// rows per `(capability_ref, args_canonical_hash)` key — coalesced pendings
/// count once per decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenialPolicy {
    /// Denials tolerated before the fallback fires (`0` = fire on the first).
    pub max_denials: u64,
    /// What happens when the count exceeds the ceiling.
    pub fallback: DenialFallback,
}

/// `RespondCtx` — the record-derived response context the dispatcher passes
/// (the lease-mint legs a response cannot carry itself): the Π fingerprint,
/// the run's scope coordinate, the decided effect's risk ceiling, the
/// grant-time `context_label.authority`, the live `ApproverGrant` set, and
/// the sealed denial policy.
#[derive(Debug, Clone, PartialEq)]
pub struct RespondCtx {
    /// The Π fingerprint (the lease key's policy leg).
    pub policy_fingerprint: String,
    /// The run's scope coordinate (the lease `scope_ref`).
    pub scope_ref: String,
    /// The decided effect's `effective_risk_class` (the lease `risk_ceiling`).
    pub risk_ceiling: hh_ontology::risk::RiskClass,
    /// `context_label.authority` at grant (the `scope = external`
    /// re-validation baseline).
    pub grant_authority: AuthorityClass,
    /// The live `ApproverGrant` set (legitimacy reads it).
    pub grants: Vec<ApproverGrant>,
    /// The sealed denial policy (`None` = no ceiling — the Stage-1 default).
    pub denial_policy: Option<DenialPolicy>,
}

impl RespondCtx {
    /// The Stage-1 compatibility context — `fingerprint = "policy"`,
    /// `scope_ref = "run"`, `risk_ceiling = UNKNOWN`, `grant_authority =
    /// principal`, no grants, no denial ceiling. Callers with real records
    /// must build a full `RespondCtx`; the default exists so the pre-S2.6
    /// call sites (and the legacy `respond`) keep their shape.
    pub fn stage1() -> RespondCtx {
        RespondCtx {
            policy_fingerprint: "policy".to_string(),
            scope_ref: "run".to_string(),
            risk_ceiling: hh_ontology::risk::RiskClass::UNKNOWN,
            grant_authority: AuthorityClass::Principal,
            grants: Vec::new(),
            denial_policy: None,
        }
    }
}

/// `ApprovalState` — the typed approval fold: `pending`, `decisions`,
/// `leases`, `stats`, `denial_counts`. It is a **pure derivation** over the
/// append-only trail —
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
    /// `decided{deny}` counts per `(capability_ref, args_canonical_hash)` key —
    /// the repeated-denial fallback's record (the count is over *decisions*,
    /// never prompt renderings; coalesced pendings count once).
    pub denial_counts: BTreeMap<String, u64>,
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
        match request.mode {
            // `sync` — the surface decides before the dispatch continues.
            ApprovalMode::Sync => {}
            // `async` (`defer`) — the pending outlives the dispatch; the run
            // suspends on `awaiting_approval` and a `permission_decided`
            // wakeup resumes it (the dispatcher appends the suspension).
            ApprovalMode::Async => {}
            // `immediate` never reaches `request_approval` — it is the
            // chain-internal "resolved before the human stage" marker.
            ApprovalMode::Immediate => {
                return Err(ApprovalError::UnsupportedMode { mode: request.mode });
            }
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
    /// a non-`principal` endorser fails `IllegitimateEndorsement` unless a
    /// live `ApproverGrant` in `ctx.grants` covers the pending's capability and
    /// the decided risk (AC-R-2.8.7-3). `respond` runs under
    /// [`RespondCtx::stage1`] — the dispatch path carries the real records via
    /// [`ApprovalState::respond_with_ctx`].
    pub fn respond(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
    ) -> Result<RespondOutcome, ApprovalError> {
        self.respond_inner(response, irreversible, responded_at, &RespondCtx::stage1())
    }

    /// `respond` under an explicit `policy_fingerprint` — the lease key's
    /// policy leg (a Π change revokes by construction). Equivalent to
    /// [`ApprovalState::respond_with_ctx`] with the Stage-1 defaults for the
    /// remaining legs.
    pub fn respond_with_policy(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
        policy_fingerprint: &str,
    ) -> Result<RespondOutcome, ApprovalError> {
        let ctx = RespondCtx {
            policy_fingerprint: policy_fingerprint.to_string(),
            ..RespondCtx::stage1()
        };
        self.respond_inner(response, irreversible, responded_at, &ctx)
    }

    /// `respond` under the full record-derived [`RespondCtx`] — the dispatch
    /// path's form (the fingerprint, scope coordinate, risk ceiling,
    /// grant-time authority, grant set and denial policy are all records).
    pub fn respond_with_ctx(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
        ctx: &RespondCtx,
    ) -> Result<RespondOutcome, ApprovalError> {
        self.respond_inner(response, irreversible, responded_at, ctx)
    }

    fn respond_inner(
        &mut self,
        response: &ApprovalResponse,
        irreversible: bool,
        responded_at: u64,
        ctx: &RespondCtx,
    ) -> Result<RespondOutcome, ApprovalError> {
        let policy_fingerprint = ctx.policy_fingerprint.as_str();
        // Exactly one decision — a duplicate returns the recorded one.
        if let Some(recorded) = self.decisions.get(&response.permission_id) {
            return Ok(RespondOutcome {
                decision: recorded.decision.clone(),
                already_decided: true,
                lease: None,
                endorsement_count: 0,
                denial_fallback: None,
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
                // An `ApproverGrant` endorsement is legitimate only when a
                // *live* grant in `ctx.grants` covers the pending's capability
                // and the decided risk (AC-R-2.8.7-3 — the record confers,
                // never the name).
                EndorserRef::ApproverGrant { grant_ref } => ctx
                    .grants
                    .iter()
                    .find(|g| &g.grant_ref == grant_ref)
                    .map(|g| {
                        g.covers(
                            &pending.request.request.capability_ref.semantic_id,
                            &ctx.risk_ceiling,
                            responded_at,
                        )
                    })
                    .unwrap_or(false),
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
                // `irreversible` leases require a declared pattern, bounded
                // uses and `scope ≤ run` (ADR-0071 D1).
                if irreversible
                    && (max_uses.is_none()
                        || spec.pattern.is_none()
                        || lease_scope_rank(spec.scope) > lease_scope_rank(LeaseScope::Run))
                {
                    self.pending.insert(response.permission_id.clone(), pending);
                    return Err(ApprovalError::LeaseScopeViolation {
                        detail:
                            "irreversible requires a declared pattern, max_uses and scope <= run"
                                .to_string(),
                    });
                }
                // The key material: the exact canonical-args hash for an
                // exact lease, or `pattern:{pattern_id}:{pattern_args_hash}`
                // for a declared `ActionPattern` lease (ADR-0071 D1 — never
                // `effect_id`).
                let key_material = match &spec.pattern {
                    Some(p) => format!(
                        "pattern:{}:{}",
                        p.pattern_id(),
                        pending.request.request.args_canonical_hash
                    ),
                    None => pending.request.request.args_canonical_hash.clone(),
                };
                let key = lease_key(
                    &pending.request.request.capability_ref,
                    &key_material,
                    spec.scope,
                    policy_fingerprint,
                );
                let lease = ApprovalLease {
                    lease_id: key.clone(),
                    key_hash: key.clone(),
                    capability_ref: pending.request.request.capability_ref.clone(),
                    args_canonical_hash: pending.request.request.args_canonical_hash.clone(),
                    pattern: spec.pattern.clone(),
                    pattern_args_hash: spec
                        .pattern
                        .as_ref()
                        .map(|_| pending.request.request.args_canonical_hash.clone()),
                    scope: spec.scope,
                    scope_ref: ctx.scope_ref.clone(),
                    holder: pending.request.request.subject_ref.clone(),
                    basis: LeaseBasis::Human,
                    origin_permission_id: pending.permission_id.clone(),
                    policy_fingerprint: policy_fingerprint.to_string(),
                    risk_ceiling: ctx.risk_ceiling,
                    grant_authority: ctx.grant_authority,
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
                    denial_fallback: None,
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
        // The repeated-denial fallback (§5g.7 §5): a `deny` decision bumps the
        // `(capability_ref, args_canonical_hash)` count; crossing the sealed
        // ceiling fires the typed fallback (never a widening, never a silent
        // unbounded retry).
        let mut denial_fallback = None;
        if matches!(decision, Decision::Deny { .. }) {
            let dkey = format!(
                "{}\x1f{}",
                pending.request.request.capability_ref.version_id,
                pending.request.request.args_canonical_hash
            );
            let n = self.denial_counts.entry(dkey).or_insert(0);
            *n += 1;
            if let Some(pol) = ctx.denial_policy {
                if *n > pol.max_denials {
                    denial_fallback = Some(pol.fallback);
                }
            }
        }
        self.decisions.insert(
            response.permission_id.clone(),
            RecordedDecision {
                decision: decision.clone(),
                decided_by: response.decided_by.clone(),
                decided_at: response.decided_at,
                lease_id,
                effect_ids: pending.effect_ids.clone(),
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
            denial_fallback,
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

    /// The recorded decision covering `effect_id` — the resume half of the
    /// defer slice: a re-dispatched effect re-deriving `ask` is served by the
    /// recorded row (`authorize` consults it before the lease stage — the
    /// decided record is the truth, never a re-ask loop).
    pub fn decision_for_effect(&self, effect_id: &str) -> Option<(&String, &RecordedDecision)> {
        self.decisions
            .iter()
            .find(|(_, d)| d.effect_ids.iter().any(|e| e == effect_id))
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

    /// `project(events, until_seq)` — rebuild the approval fold from the
    /// durable prefix (`HandleTable::project`'s sibling — the ADR-0026
    /// derived view; CC1: the trail is the one truth, the fold a pure
    /// derivation). A malformed row is skipped — the durable form was
    /// validated at append; `project` never guesses.
    pub fn project(events: &[hh_ledger::event::EventEnvelope], until_seq: u64) -> ApprovalState {
        let mut st = ApprovalState::default();
        for env in events.iter().filter(|e| e.seq <= until_seq) {
            st.fold(env);
        }
        st
    }

    /// Fold one durable envelope. `pending` opens the owed-decision row
    /// (a repeat `permission_id` coalesces — the deterministic mint *is*
    /// the coalescing rule); `decided` resolves it into the decisions
    /// table (the exactly-one gate's record — a second `decided` for the
    /// id never overwrites); `lease.granted`/`used`/`revoked`/`expired`
    /// drive the lease lifecycle. The I-P5 counters fold in spec order:
    /// `requested` counts pending rows, `granted` counts `decided{allow}`,
    /// `human_wait_ms` sums the human-decided `wait_ms`.
    pub fn fold(&mut self, env: &hh_ledger::event::EventEnvelope) {
        let p = &env.payload;
        match env.class.as_str() {
            "security.permission.pending" => {
                let Some(pid) = p.get("permission_id").and_then(Json::as_str) else {
                    return;
                };
                let rq = p.get("request").cloned().unwrap_or(Json::Null);
                let cap = rq.get("capability_ref").cloned().unwrap_or(Json::Null);
                let request = PermissionRequest {
                    subject_ref: rq
                        .get("subject_ref")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    capability_ref: PinnedRef {
                        semantic_id: cap
                            .get("semantic_id")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        version_id: cap
                            .get("version_id")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    },
                    args_canonical_hash: rq
                        .get("args_canonical_hash")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    reason: rq
                        .get("reason")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                };
                let mode = match p.get("mode").and_then(Json::as_str) {
                    Some(m) => match ApprovalMode::parse(m) {
                        Some(m) => m,
                        None => return,
                    },
                    None => ApprovalMode::Sync,
                };
                let mut effect_ids: Vec<String> = match p.get("effect_ids") {
                    Some(Json::Arr(rows)) => rows
                        .iter()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect(),
                    _ => Vec::new(),
                };
                if let Some(e) = p.get("effect_id").and_then(Json::as_str) {
                    if !e.is_empty() && !effect_ids.iter().any(|x| x == e) {
                        effect_ids.push(e.to_string());
                    }
                }
                match self.pending.get_mut(pid) {
                    // Coalesced — the new effects attach; `requested` never
                    // double-counts.
                    Some(row) => {
                        for e in effect_ids {
                            if !row.effect_ids.iter().any(|x| x == &e) {
                                row.effect_ids.push(e);
                            }
                        }
                    }
                    None => {
                        self.stats.requested += 1;
                        self.pending.insert(
                            pid.to_string(),
                            ApprovalPending {
                                permission_id: pid.to_string(),
                                request: ApprovalRequest {
                                    permission_id: pid.to_string(),
                                    request,
                                    options: Vec::new(),
                                    mode,
                                    timeout: p
                                        .get("timeout")
                                        .and_then(Json::as_int)
                                        .map(|t| t.max(0) as u64),
                                    explanation: Explanation {
                                        display: String::new(),
                                        rows: Vec::new(),
                                        model_justification: None,
                                    },
                                    batch_id: p
                                        .get("batch_id")
                                        .and_then(Json::as_str)
                                        .map(String::from),
                                },
                                requested_at: p
                                    .get("requested_at")
                                    .and_then(Json::as_int)
                                    .map(|t| t.max(0) as u64)
                                    .unwrap_or(0),
                                effect_ids,
                            },
                        );
                    }
                }
            }
            "security.permission.decided" => {
                let Some(pid) = p.get("permission_id").and_then(Json::as_str) else {
                    return;
                };
                // The exactly-one gate — the first durable `decided` for an
                // id is the record; a second never rewrites it.
                if self.decisions.contains_key(pid) {
                    return;
                }
                let decision = match p.get("decision").and_then(Json::as_str) {
                    Some("allow") | Some("allow_once") | Some("allow_lease") => Decision::Allow,
                    // `decided{ask}` is *non-final* — the attempt cycle's ask
                    // verdict records the escalation, never resolves the
                    // pending (the final `decided{allow|deny}` for the same
                    // `permission_id` lands at respond/chain-terminal).
                    Some("ask") => return,
                    Some(d) if d.starts_with("deny") || d == "cancelled" || d == "abort_run" => {
                        Decision::Deny {
                            reason: deny_reason_fold(p.get("reason").and_then(Json::as_str)),
                            remedies: Vec::new(),
                        }
                    }
                    _ => return,
                };
                let decided_by = match p.get("decider").and_then(Json::as_str) {
                    Some(d @ ("human" | "principal")) => EndorserRef::Human {
                        subject_ref: p
                            .get("decider_ref")
                            .and_then(Json::as_str)
                            .unwrap_or(d)
                            .to_string(),
                        authority: AuthorityClass::Principal,
                    },
                    Some(other) => EndorserRef::Human {
                        subject_ref: p
                            .get("decider_ref")
                            .and_then(Json::as_str)
                            .unwrap_or(other)
                            .to_string(),
                        authority: AuthorityClass::Delegate,
                    },
                    None => return,
                };
                let wait_ms = p
                    .get("wait_ms")
                    .and_then(Json::as_int)
                    .map(|t| t.max(0) as u64)
                    .unwrap_or(0);
                // Capture the pending *before* removal — the denial key and
                // the effect linkage ride its record.
                let pending = self.pending.remove(pid);
                let mut effect_ids = pending
                    .as_ref()
                    .map(|r| r.effect_ids.clone())
                    .unwrap_or_default();
                if let Some(e) = p.get("effect_id").and_then(Json::as_str) {
                    if !e.is_empty() && !effect_ids.iter().any(|x| x == e) {
                        effect_ids.push(e.to_string());
                    }
                }
                // A scope-scoped `decided` may carry the attached effects
                // directly (`effect_ids` member).
                if let Some(Json::Arr(rows)) = p.get("effect_ids") {
                    for e in rows.iter().filter_map(|x| x.as_str()) {
                        if !effect_ids.iter().any(|x| x == e) {
                            effect_ids.push(e.to_string());
                        }
                    }
                }
                let requested_at = p
                    .get("requested_at")
                    .and_then(Json::as_int)
                    .map(|t| t.max(0) as u64)
                    .or_else(|| pending.as_ref().map(|r| r.requested_at))
                    .unwrap_or(0);
                if matches!(decision, Decision::Allow) {
                    self.stats.granted += 1;
                }
                if matches!(decided_by, EndorserRef::Human { authority, .. } if authority == AuthorityClass::Principal)
                {
                    self.stats.human_wait_ms += wait_ms;
                }
                if matches!(decision, Decision::Deny { .. }) {
                    if let Some(r) = &pending {
                        let dkey = format!(
                            "{}{}",
                            r.request.request.capability_ref.version_id,
                            r.request.request.args_canonical_hash
                        );
                        *self.denial_counts.entry(dkey).or_insert(0) += 1;
                    }
                }
                // The lease this decision minted, when the `granted` row
                // folded first (either order resolves — the granted arm
                // back-fills the symmetric direction).
                let lease_id = self
                    .leases
                    .values()
                    .find(|l| l.origin_permission_id == pid)
                    .map(|l| l.lease_id.clone());
                self.decisions.insert(
                    pid.to_string(),
                    RecordedDecision {
                        decision,
                        decided_by,
                        decided_at: requested_at.saturating_add(wait_ms),
                        lease_id,
                        effect_ids,
                    },
                );
            }
            "security.permission.lease.granted" => {
                let Some(lease) = lease_from_granted(p) else {
                    return;
                };
                // Back-fill the decision's lease link when it folded first.
                if let Some(rec) = self.decisions.get_mut(&lease.origin_permission_id) {
                    if rec.lease_id.is_none() {
                        rec.lease_id = Some(lease.lease_id.clone());
                    }
                }
                self.leases.insert(lease.key_hash.clone(), lease);
            }
            "security.permission.lease.used" => {
                let key = p
                    .get("key_hash")
                    .or_else(|| p.get("lease_id"))
                    .and_then(Json::as_str);
                if let Some(k) = key {
                    if let Some(l) = self.leases.get_mut(k) {
                        if let Some(u) = p.get("uses").and_then(Json::as_int) {
                            l.uses = u.max(0) as u64;
                        } else {
                            l.uses += 1;
                        }
                    }
                }
            }
            "security.permission.lease.revoked" | "security.permission.lease.expired" => {
                let key = p
                    .get("key_hash")
                    .or_else(|| p.get("lease_id"))
                    .and_then(Json::as_str);
                if let Some(k) = key {
                    let at = p
                        .get("revoked_at")
                        .and_then(Json::as_int)
                        .map(|t| t.max(0) as u64)
                        .unwrap_or(0);
                    self.revoke_lease(k, at);
                }
            }
            _ => {}
        }
    }
}

/// `deny_reason_fold` — the decided row's `reason` member back into the
/// closed sum (display detail never re-enters the decision; the tag is the
/// datum).
fn deny_reason_fold(s: Option<&str>) -> DenyReason {
    match s {
        Some("ApprovalsExhausted") => DenyReason::ApprovalsExhausted,
        Some("UnattendedAsk") => DenyReason::UnattendedAsk,
        Some("ApprovalTimedOut") => DenyReason::ApprovalTimedOut,
        Some("HandleRevoked") => DenyReason::HandleRevoked,
        Some("NoCoveringGrant") => DenyReason::NoCoveringGrant,
        Some("ScopeCeilingExceeded") => DenyReason::ScopeCeilingExceeded,
        Some("GrantConstraintExhausted") => DenyReason::GrantConstraintExhausted,
        Some("AuthorityWidening") => DenyReason::AuthorityWidening,
        _ => DenyReason::PolicyDenied,
    }
}

/// `lease_from_granted` — decode a `security.permission.lease.granted`
/// payload back into the lease record (the fold's half — a malformed row
/// returns `None`; the durable form was validated at append).
fn lease_from_granted(p: &Json) -> Option<ApprovalLease> {
    let lease_id = p.get("lease_id").and_then(Json::as_str)?.to_string();
    let key_hash = p
        .get("key_hash")
        .and_then(Json::as_str)
        .map(String::from)
        .unwrap_or_else(|| lease_id.clone());
    let cap = p.get("capability_ref")?;
    let scope = lease_scope_parse(p.get("scope")?.as_str()?, "lease.scope").ok()?;
    let basis = match p.get("basis").and_then(Json::as_str) {
        Some("policy_rule") => LeaseBasis::PolicyRule {
            rule_ref: p
                .get("rule_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            verdict: match p.get("verdict").and_then(Json::as_str) {
                Some("deny") => ReviewVerdictKind::Deny,
                Some("amend") => ReviewVerdictKind::Amend,
                _ => ReviewVerdictKind::Allow,
            },
        },
        _ => LeaseBasis::Human,
    };
    Some(ApprovalLease {
        lease_id,
        key_hash,
        capability_ref: PinnedRef {
            semantic_id: cap.get("semantic_id")?.as_str()?.to_string(),
            version_id: cap.get("version_id")?.as_str()?.to_string(),
        },
        args_canonical_hash: p
            .get("args_canonical_hash")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        pattern: p.get("pattern").and_then(ActionPattern::from_json),
        pattern_args_hash: p
            .get("pattern_args_hash")
            .and_then(Json::as_str)
            .map(String::from),
        scope,
        scope_ref: p
            .get("scope_ref")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        holder: p
            .get("holder")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        basis,
        origin_permission_id: p
            .get("permission_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        policy_fingerprint: p
            .get("policy_fingerprint")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        risk_ceiling: p
            .get("risk_ceiling")
            .and_then(hh_ontology::risk::RiskClass::from_json)
            .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN),
        grant_authority: p
            .get("grant_authority")
            .and_then(Json::as_str)
            .and_then(AuthorityClass::parse)
            .unwrap_or(AuthorityClass::Delegate),
        max_uses: p
            .get("max_uses")
            .and_then(Json::as_int)
            .map(|u| u.max(0) as u64),
        uses: p
            .get("uses")
            .and_then(Json::as_int)
            .map(|u| u.max(0) as u64)
            .unwrap_or(0),
        granted_at: p
            .get("granted_at")
            .and_then(Json::as_int)
            .map(|t| t.max(0) as u64)
            .unwrap_or(0),
        revoked_at: p
            .get("revoked_at")
            .and_then(Json::as_int)
            .map(|t| t.max(0) as u64),
    })
}

/// `lease_granted_payload` — the `security.permission.lease.granted` row's
/// payload (§5g.7 §3 lease record — the fold-readable form; every member the
/// fold decodes is a declared audit field).
pub fn lease_granted_payload(l: &ApprovalLease) -> Json {
    let basis = match &l.basis {
        LeaseBasis::Human => Json::str("human"),
        LeaseBasis::PolicyRule { .. } => Json::str("policy_rule"),
    };
    let mut m = vec![
        ("lease_id", Json::str(l.lease_id.clone())),
        ("key_hash", Json::str(l.key_hash.clone())),
        (
            "capability_ref",
            Json::obj([
                (
                    "semantic_id",
                    Json::str(l.capability_ref.semantic_id.clone()),
                ),
                ("version_id", Json::str(l.capability_ref.version_id.clone())),
            ]),
        ),
        (
            "args_canonical_hash",
            Json::str(l.args_canonical_hash.clone()),
        ),
        ("scope", Json::str(l.scope.as_str())),
        ("basis", basis),
        ("scope_ref", Json::str(l.scope_ref.clone())),
        ("holder", Json::str(l.holder.clone())),
        ("permission_id", Json::str(l.origin_permission_id.clone())),
        (
            "policy_fingerprint",
            Json::str(l.policy_fingerprint.clone()),
        ),
        ("risk_ceiling", l.risk_ceiling.to_json()),
        ("grant_authority", Json::str(l.grant_authority.as_str())),
        ("granted_at", Json::Int(l.granted_at as i64)),
        ("uses", Json::Int(l.uses as i64)),
    ];
    if let Some(pat) = &l.pattern {
        m.push(("pattern", pat.to_json()));
    }
    if let Some(h) = &l.pattern_args_hash {
        m.push(("pattern_args_hash", Json::str(h.clone())));
    }
    if let Some(u) = l.max_uses {
        m.push(("max_uses", Json::Int(u as i64)));
    }
    if let Some(t) = l.revoked_at {
        m.push(("revoked_at", Json::Int(t as i64)));
    }
    if let LeaseBasis::PolicyRule { rule_ref, verdict } = &l.basis {
        m.push(("rule_ref", Json::str(rule_ref.clone())));
        m.push(("verdict", Json::str(verdict.as_str())));
    }
    Json::obj(m)
}

/// `RespondOutcome` — what `respond` returns: the decision, whether it was
/// already recorded (duplicate — no new `decided` row is emitted), the minted
/// lease, how many `label.endorsed` rows the decision produced (N for a batch
/// allow — one per coalesced effect; AC-R-2.8.7-14), and the repeated-denial
/// fallback when the sealed ceiling crossed.
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
    /// The typed repeated-denial fallback, when the sealed `DenialPolicy`
    /// ceiling crossed (`None` = under the ceiling or no policy declared).
    pub denial_fallback: Option<DenialFallback>,
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

fn lease_scope_parse(s: &str, path: &str) -> Result<LeaseScope, String> {
    match s {
        "turn" => Ok(LeaseScope::Turn),
        "run" => Ok(LeaseScope::Run),
        "session" => Ok(LeaseScope::Session),
        other => Err(format!("{path}: unknown lease scope {other}")),
    }
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
                (
                    "pattern",
                    spec.pattern
                        .as_ref()
                        .map(ActionPattern::to_json)
                        .unwrap_or(Json::Null),
                ),
                ("scope", Json::str(spec.scope.as_str())),
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
                    pattern: l.get("pattern").and_then(ActionPattern::from_json),
                    scope: lease_scope_parse(&req_str(l, "scope", path)?, path)?,
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
        (
            "batch_id",
            r.batch_id
                .as_ref()
                .map(|b| Json::str(b.clone()))
                .unwrap_or(Json::Null),
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
                (
                    "model_justification",
                    r.explanation
                        .model_justification
                        .as_ref()
                        .map(|m| Json::str(m.text.clone()))
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        ("mode", Json::str(r.mode.as_str())),
        (
            "timeout",
            r.timeout.map(|t| Json::Int(t as i64)).unwrap_or(Json::Null),
        ),
        (
            "batch_id",
            r.batch_id
                .as_ref()
                .map(|b| Json::str(b.clone()))
                .unwrap_or(Json::Null),
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

/// `auto_review_rules(sealed)` — extract the sealed `HarnessRule{auto_review}`
/// set the reviewer chain's `auto_reviewer` stage runs (I-P2 — the rule, not
/// the hook, endorses; raise-only admission means a sealed rule may narrow or
/// deny, never widen past the pending's ceiling). A malformed member skips
/// the rule — the sealed definition validated it, the extractor never
/// guesses.
pub fn auto_review_rules(sealed: &hh_hir::SealedDefinition) -> Vec<AutoReviewRule> {
    let mut out = Vec::new();
    for node in &sealed.document.nodes {
        let hh_hir::records::KindRecord::HarnessRule(rule) = &node.semantic else {
            continue;
        };
        let hh_hir::records::RuleAction::AutoReview(spec) = &rule.action else {
            continue;
        };
        let Some(verdict) = spec
            .get("verdict")
            .and_then(Json::as_str)
            .and_then(|v| match v {
                "allow" => Some(ReviewVerdictKind::Allow),
                "deny" => Some(ReviewVerdictKind::Deny),
                "amend" => Some(ReviewVerdictKind::Amend),
                _ => None,
            })
        else {
            continue;
        };
        let Some(max_risk) = spec
            .get("max_risk")
            .and_then(hh_ontology::risk::RiskClass::from_json)
        else {
            continue;
        };
        let domains = match spec.get("domains") {
            Some(Json::Arr(rows)) => rows
                .iter()
                .filter_map(|d| {
                    d.as_str()
                        .and_then(|s| hh_hir::kinds::EffectDomain::parse(s).ok())
                })
                .collect(),
            _ => Vec::new(),
        };
        out.push(AutoReviewRule {
            rule_ref: rule.rule_id.clone(),
            domains,
            max_risk,
            eff_at_most: spec
                .get("eff_at_most")
                .and_then(Json::as_str)
                .and_then(AuthorityClass::parse),
            verdict,
        });
    }
    out
}
