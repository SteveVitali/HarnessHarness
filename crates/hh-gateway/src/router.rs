//! The C0 `ModelRouter` slice (§5b.2; ADR-0121 d.1–d.7; ADR-0122 d.2/d.3):
//! `RoutingPolicy` as MUST-data with the closed kind enum, the
//! `ModelRoleTable` role→`RoleBinding` map, `select` under the compatibility
//! guard (the C0 arms are G-1/G-2/G-4), the closed `RoutingRefusal` sum, and
//! the `RoutingDecision` record `model.route.decided` carries.
//!
//! A refused route is a typed refusal — never a warning, never a silent
//! fallback to the primary (AC-R-2.3.2-2). `error_actions` is data on the
//! policy record and rides its `content_hash` (AC-R-2.3.2-6) — the router
//! never pattern-matches provider message text. `select` is pure over its
//! inputs (AC-R-2.3.2-11): identical `(request, views, policy)` yields
//! identical decisions minus the allocated `decision_id`/`reservation_id`.

use std::collections::{BTreeMap, BTreeSet};

use hh_compiler::profile::{
    profile_status, resolve_profile, CapabilityState, DebtStatus, ModelCoordinate,
    ProfileDebtRecord, ProfileResolveError, SelectorView,
};
use hh_wire::json::Json;

use crate::plan::ModelRef;
use crate::vocab::{ModelErrorClass, RerouteReason};

/// `RoutingRequest{role, model_coordinate, required_capabilities[], budget_id,
/// holder, model_call_id?, intent_ref?}` — what `select` consumes (§5b.2
/// `select(RoutingRequest, profile_env, account, health, policy)`).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingRequest {
    /// `role` — the `ModelRole` spelling (`primary`, `utility`, …).
    pub role: String,
    /// `required_capabilities[]` — the capability axes the call needs.
    pub required_capabilities: Vec<String>,
    /// `budget_id` — the budget the G-4 reserve charges.
    pub budget_id: String,
    /// `holder` — the reservation holder.
    pub holder: String,
    /// `model_call_id` — the logical call this decision serves.
    pub model_call_id: Option<String>,
    /// `intent_ref` — a recorded `Design`/operator intent; binds an `expired`
    /// profile (G-1 admits `expired` only with intent; `retired` never).
    pub intent_ref: Option<String>,
    /// `effort?` — the requested effort rung (the `ModelRef.effort` member).
    pub effort: Option<String>,
}

/// `RoutingPolicy.kind` — the closed kind enum (ADR-0121 d.4). C0 executes
/// `static`/`role_table`; every other kind is `PolicyInvalid{reason:
/// "kind … is C1"}` at C0 — a declared tier refusal, never a fall-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingPolicyKind {
    /// `static` — one fixed candidate row.
    Static,
    /// `role_table` — resolve `role` through the `ModelRoleTable`.
    RoleTable,
    /// `fallback_chain` (C1).
    FallbackChain,
    /// `capability_filter` (C1).
    CapabilityFilter,
    /// `cost_cap` (C1).
    CostCap,
    /// `latency_cap` (C1).
    LatencyCap,
    /// `quality_target` (C1).
    QualityTarget,
    /// `health_aware` (C1).
    HealthAware,
    /// `learned` (C4).
    Learned,
}

impl RoutingPolicyKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RoutingPolicyKind::Static => "static",
            RoutingPolicyKind::RoleTable => "role_table",
            RoutingPolicyKind::FallbackChain => "fallback_chain",
            RoutingPolicyKind::CapabilityFilter => "capability_filter",
            RoutingPolicyKind::CostCap => "cost_cap",
            RoutingPolicyKind::LatencyCap => "latency_cap",
            RoutingPolicyKind::QualityTarget => "quality_target",
            RoutingPolicyKind::HealthAware => "health_aware",
            RoutingPolicyKind::Learned => "learned",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RoutingPolicyKind> {
        Some(match s {
            "static" => RoutingPolicyKind::Static,
            "role_table" => RoutingPolicyKind::RoleTable,
            "fallback_chain" => RoutingPolicyKind::FallbackChain,
            "capability_filter" => RoutingPolicyKind::CapabilityFilter,
            "cost_cap" => RoutingPolicyKind::CostCap,
            "latency_cap" => RoutingPolicyKind::LatencyCap,
            "quality_target" => RoutingPolicyKind::QualityTarget,
            "health_aware" => RoutingPolicyKind::HealthAware,
            "learned" => RoutingPolicyKind::Learned,
            _ => return None,
        })
    }

    /// Whether the kind executes at C0 (`static`/`role_table` — ADR-0121 d.7).
    pub fn c0(self) -> bool {
        matches!(
            self,
            RoutingPolicyKind::Static | RoutingPolicyKind::RoleTable
        )
    }
}

/// `error_actions` value — the class → action table (ADR-0122 d.2):
/// `{retry_same{max}, reroute, compact_then_retry, give_up}`. `GiveUp` closes
/// `model.call.failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorAction {
    /// Retry the same target, bounded.
    RetrySame {
        /// `max` — the same-target attempt bound.
        max: u32,
    },
    /// Reroute to another target (`model.rerouted`).
    Reroute,
    /// Compact the transcript, then retry (`context.compaction.*` under the
    /// old profile — ADR-0122 d.3 ordering).
    CompactThenRetry,
    /// Close the call `model.call.failed`.
    GiveUp,
}

impl ErrorAction {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorAction::RetrySame { .. } => "retry_same",
            ErrorAction::Reroute => "reroute",
            ErrorAction::CompactThenRetry => "compact_then_retry",
            ErrorAction::GiveUp => "give_up",
        }
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            ErrorAction::RetrySame { max } => {
                Json::obj([("retry_same", Json::obj([("max", Json::Int(*max as i64))]))])
            }
            other => Json::str(other.as_str()),
        }
    }
}

/// `max_migration_loss{dropped_items ≤ n, no_in_flight_tool_call: true}` — the
/// G-3 bound (C1; carried as data at C0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationLossBound {
    /// `dropped_items ≤ n`.
    pub dropped_items: u64,
    /// `no_in_flight_tool_call` — a migration never severs an in-flight call.
    pub no_in_flight_tool_call: bool,
}

/// One `conditioned_rules[]` entry — a family- or provider-keyed rule carries
/// an `AssumptionDebtRecord` (`expiry_condition ∋ model_version_change`);
/// `link` refuses an incomplete record (T-LCD-05 — AC-R-2.3.2-7).
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyConditionedRule {
    /// The rule id.
    pub rule_id: String,
    /// `conditioned_key` — `Some(_)` when the rule keys on a
    /// family/provider/serving route (debt record then mandatory).
    pub conditioned_key: Option<String>,
    /// The rule's `AssumptionDebtRecord` (the CF-049 shape).
    pub debt: Option<ProfileDebtRecord>,
}

/// `RoutingPolicy{policy_id, version, content_hash, role_scope: set<ModelRole>,
/// kind ∈ {…}, params, error_actions: map<ModelErrorClass, {…}>,
/// max_migration_loss{…}, allow_unknown?: ConditionedRule,
/// conditioned_rules[], ext?}` — the MUST-data routing policy document
/// (ADR-0121 d.4; ADR-0024).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingPolicy {
    /// `policy_id` — the semantic coordinate.
    pub policy_id: String,
    /// `version`.
    pub version: String,
    /// `content_hash` — the `idp/1` address of the record minus `content_hash`
    /// (`error_actions` is inside the hash — AC-R-2.3.2-6).
    pub content_hash: String,
    /// `role_scope` — the roles this policy governs (empty = all roles).
    pub role_scope: BTreeSet<String>,
    /// `kind`.
    pub kind: RoutingPolicyKind,
    /// `params` — per kind (`static{target}` / `role_table{table_ref}`).
    pub params: Json,
    /// `error_actions` — the class → action table, keyed by the
    /// `ModelErrorClass` spelling (data — never message text).
    pub error_actions: BTreeMap<String, ErrorAction>,
    /// `max_migration_loss`.
    pub max_migration_loss: MigrationLossBound,
    /// `allow_unknown` — the debt-recorded rule admitting `unknown`
    /// capabilities (G-2's only escape).
    pub allow_unknown: Option<PolicyConditionedRule>,
    /// `conditioned_rules[]` — every family/provider-keyed rule is a
    /// conditioned rule with a complete debt record.
    pub conditioned_rules: Vec<PolicyConditionedRule>,
    /// `ext` — sealed extension block.
    pub ext: BTreeMap<String, Json>,
}

impl RoutingPolicy {
    /// Canonical JSON (`content_hash` excluded — it is the address of the
    /// rest; `error_actions` rides the hash, AC-R-2.3.2-6).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("policy_id".into(), Json::str(self.policy_id.clone()));
        m.insert("version".into(), Json::str(self.version.clone()));
        m.insert(
            "role_scope".into(),
            Json::Arr(
                self.role_scope
                    .iter()
                    .map(|r| Json::str(r.clone()))
                    .collect(),
            ),
        );
        m.insert("kind".into(), Json::str(self.kind.as_str()));
        m.insert("params".into(), self.params.clone());
        m.insert(
            "error_actions".into(),
            Json::Obj(
                self.error_actions
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        );
        m.insert(
            "max_migration_loss".into(),
            Json::obj([
                (
                    "dropped_items",
                    Json::Int(self.max_migration_loss.dropped_items as i64),
                ),
                (
                    "no_in_flight_tool_call",
                    Json::Bool(self.max_migration_loss.no_in_flight_tool_call),
                ),
            ]),
        );
        if let Some(a) = &self.allow_unknown {
            m.insert("allow_unknown".into(), conditioned_rule_json(a));
        }
        m.insert(
            "conditioned_rules".into(),
            Json::Arr(
                self.conditioned_rules
                    .iter()
                    .map(conditioned_rule_json)
                    .collect(),
            ),
        );
        if !self.ext.is_empty() {
            m.insert("ext".into(), Json::Obj(self.ext.clone()));
        }
        Json::Obj(m)
    }

    /// `content_hash` — the `idp/1` address of `to_json()`.
    pub fn content_id(&self) -> String {
        hh_identity::idp_id(
            "routing_policy.1",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// `link`-time validation (T-LCD-05 — AC-R-2.3.2-7): every family- or
    /// provider-keyed conditioned rule carries a complete debt record
    /// (`expiry_condition ∋ model_version_change`); an incomplete record is
    /// `PolicyInvalid{rule_id, missing_debt_record}` — a typed refusal, never
    /// a warning.
    pub fn link_check(&self) -> Result<(), RoutingRefusal> {
        for rule in self
            .conditioned_rules
            .iter()
            .chain(self.allow_unknown.iter())
        {
            if rule.conditioned_key.is_some() {
                match &rule.debt {
                    Some(d) if d.is_complete() => {
                        if d.expiry_condition.kind
                            != hh_compiler::profile::ExpiryKind::ModelVersionChange
                        {
                            return Err(RoutingRefusal::PolicyInvalid {
                                rule_id: rule.rule_id.clone(),
                                reason: "conditioned rule's debt record lacks \
                                         expiry_condition = model_version_change"
                                    .to_string(),
                            });
                        }
                    }
                    _ => {
                        return Err(RoutingRefusal::PolicyInvalid {
                            rule_id: rule.rule_id.clone(),
                            reason: "missing_debt_record".to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn conditioned_rule_json(r: &PolicyConditionedRule) -> Json {
    let mut m = BTreeMap::new();
    m.insert("rule_id".into(), Json::str(r.rule_id.clone()));
    if let Some(k) = &r.conditioned_key {
        m.insert("conditioned_key".into(), Json::str(k.clone()));
    }
    if let Some(d) = &r.debt {
        m.insert("debt".into(), crate::events::debt_json(d));
    }
    Json::Obj(m)
}

/// `RoutingRefusal` — the closed refusal sum (§5b.2; ADR-0121 d.3):
/// `{NoProfile{model_ref}, ExpiredProfile{profile_ref, status},
/// CapabilityUnmet{model_ref, missing[]}, CapabilityUnknown{model_ref,
/// unknown[]}, MigrationLossExceeded{dropped, bound},
/// InsufficientBudget{dimension}, NoPrice{model_ref},
/// ChainExhausted{attempted[]}, PolicyInvalid{rule_id, reason}}`.
#[derive(Debug, Clone, PartialEq)]
pub enum RoutingRefusal {
    /// `NoProfile{model_ref}` — no selector resolved (G-1).
    NoProfile {
        /// The model coordinate.
        model_ref: String,
    },
    /// `ExpiredProfile{profile_ref, status}` — `expired` without a recorded
    /// `intent_ref`; `retired` never binds (G-1).
    ExpiredProfile {
        /// The profile coordinate.
        profile_ref: String,
        /// The observed status.
        status: String,
    },
    /// `CapabilityUnmet{model_ref, missing[]}` — a required capability is
    /// declared/probed `unsupported` (G-2).
    CapabilityUnmet {
        /// The model coordinate.
        model_ref: String,
        /// The unmet axes.
        missing: Vec<String>,
    },
    /// `CapabilityUnknown{model_ref, unknown[]}` — a required capability is
    /// `unknown` and no debt-recorded `allow_unknown` rule exists (G-2).
    CapabilityUnknown {
        /// The model coordinate.
        model_ref: String,
        /// The unknown axes.
        unknown: Vec<String>,
    },
    /// `MigrationLossExceeded{dropped, bound}` — the projected
    /// `TranscriptMigration` exceeds `max_migration_loss` (G-3; C1).
    MigrationLossExceeded {
        /// Dropped items.
        dropped: u64,
        /// The declared bound.
        bound: u64,
    },
    /// `InsufficientBudget{dimension}` — `reserve` refused (G-4; `amend`
    /// never, ADR-0040 d.6).
    InsufficientBudget {
        /// The exhausted dimension.
        dimension: String,
    },
    /// `NoPrice{model_ref}` — no pricing row covers the target (G-5; C1).
    NoPrice {
        /// The model coordinate.
        model_ref: String,
    },
    /// `ChainExhausted{attempted[]}` — every candidate in the role's binding
    /// refused with a distinct class (the fallback-chain total failure).
    ChainExhausted {
        /// The attempted candidates.
        attempted: Vec<String>,
    },
    /// `PolicyInvalid{rule_id, reason}` — the policy document failed
    /// `link`-time validation (a conditioned rule without a debt record, an
    /// ambiguous selector, a non-C0 kind at C0).
    PolicyInvalid {
        /// The offending rule.
        rule_id: String,
        /// Why.
        reason: String,
    },
}

impl RoutingRefusal {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            RoutingRefusal::NoProfile { .. } => "no_profile",
            RoutingRefusal::ExpiredProfile { .. } => "expired_profile",
            RoutingRefusal::CapabilityUnmet { .. } => "capability_unmet",
            RoutingRefusal::CapabilityUnknown { .. } => "capability_unknown",
            RoutingRefusal::MigrationLossExceeded { .. } => "migration_loss_exceeded",
            RoutingRefusal::InsufficientBudget { .. } => "insufficient_budget",
            RoutingRefusal::NoPrice { .. } => "no_price",
            RoutingRefusal::ChainExhausted { .. } => "chain_exhausted",
            RoutingRefusal::PolicyInvalid { .. } => "policy_invalid",
        }
    }
}

impl std::fmt::Display for RoutingRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingRefusal::NoProfile { model_ref } => {
                write!(f, "NoProfile: {model_ref}")
            }
            RoutingRefusal::ExpiredProfile {
                profile_ref,
                status,
            } => write!(f, "ExpiredProfile: {profile_ref} ({status})"),
            RoutingRefusal::CapabilityUnmet { model_ref, missing } => {
                write!(
                    f,
                    "CapabilityUnmet: {model_ref} missing {}",
                    missing.join(",")
                )
            }
            RoutingRefusal::CapabilityUnknown { model_ref, unknown } => {
                write!(
                    f,
                    "CapabilityUnknown: {model_ref} unknown {}",
                    unknown.join(",")
                )
            }
            RoutingRefusal::MigrationLossExceeded { dropped, bound } => {
                write!(f, "MigrationLossExceeded: {dropped} > {bound}")
            }
            RoutingRefusal::InsufficientBudget { dimension } => {
                write!(f, "InsufficientBudget: {dimension}")
            }
            RoutingRefusal::NoPrice { model_ref } => write!(f, "NoPrice: {model_ref}"),
            RoutingRefusal::ChainExhausted { attempted } => {
                write!(f, "ChainExhausted: {}", attempted.join(", "))
            }
            RoutingRefusal::PolicyInvalid { rule_id, reason } => {
                write!(f, "PolicyInvalid: {rule_id}: {reason}")
            }
        }
    }
}
impl std::error::Error for RoutingRefusal {}

/// `verdict ∈ {selected, rejected{reason}}` — the per-candidate verdict on a
/// `RoutingDecision`; the `rejected` reason set is closed (§5b.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateVerdict {
    /// The candidate was selected.
    Selected,
    /// The candidate was rejected with a closed reason.
    Rejected(CandidateRejectReason),
}

/// `rejected.reason ∈ {no_profile, expired_profile, capability_unmet,
/// capability_unknown, migration_loss, insufficient_budget, no_price,
/// cooldown, policy_excluded, lower_score}` (§5b.2 `RoutingDecision`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateRejectReason {
    /// No profile binds the candidate.
    NoProfile,
    /// The bound profile is `expired`/`retired` inadmissible.
    ExpiredProfile,
    /// A declared capability is unmet.
    CapabilityUnmet,
    /// A capability is `unknown` where the guard requires declared.
    CapabilityUnknown,
    /// The projected migration exceeds the loss bound.
    MigrationLoss,
    /// The reserve refused.
    InsufficientBudget,
    /// No pricing row.
    NoPrice,
    /// The candidate is in cooldown.
    Cooldown,
    /// The policy excludes the candidate.
    PolicyExcluded,
    /// The candidate scored lower than the selected one.
    LowerScore,
}

impl CandidateRejectReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateRejectReason::NoProfile => "no_profile",
            CandidateRejectReason::ExpiredProfile => "expired_profile",
            CandidateRejectReason::CapabilityUnmet => "capability_unmet",
            CandidateRejectReason::CapabilityUnknown => "capability_unknown",
            CandidateRejectReason::MigrationLoss => "migration_loss",
            CandidateRejectReason::InsufficientBudget => "insufficient_budget",
            CandidateRejectReason::NoPrice => "no_price",
            CandidateRejectReason::Cooldown => "cooldown",
            CandidateRejectReason::PolicyExcluded => "policy_excluded",
            CandidateRejectReason::LowerScore => "lower_score",
        }
    }
}

impl CandidateVerdict {
    /// The payload spelling (`rejected{reason}` spells `rejected:<reason>`).
    pub fn spelling(&self) -> String {
        match self {
            CandidateVerdict::Selected => "selected".to_string(),
            CandidateVerdict::Rejected(r) => format!("rejected:{}", r.as_str()),
        }
    }
}

/// One `candidates_considered[]` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `model_ref` — the candidate's coordinate spelling.
    pub model_ref: String,
    /// `verdict`.
    pub verdict: CandidateVerdict,
    /// `score?`.
    pub score: Option<i64>,
}

/// `RoutingDecision{decision_id, model_call_id, role, selected: ModelRef,
/// reservation_id, policy_ref{variant_ref, version_id}, rule_ids_fired[],
/// candidates_considered[], inputs_read[], deviation, relower_required}` —
/// the `model.route.decided` payload (§5b.2; ADR-0121 d.3).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingDecision {
    /// `decision_id` — allocated (ADR-0027).
    pub decision_id: String,
    /// `model_call_id`.
    pub model_call_id: Option<String>,
    /// `role`.
    pub role: String,
    /// `selected` — the `ModelRef`.
    pub selected: ModelRef,
    /// `reservation_id` — the budget reservation the route bound (R-2:
    /// reserve-before-return).
    pub reservation_id: Option<String>,
    /// `policy_ref.variant_ref` — the routing-policy variant coordinate.
    pub policy_ref: String,
    /// `policy_ref.version_id`.
    pub policy_version_id: String,
    /// `rule_ids_fired[]`.
    pub rule_ids_fired: Vec<String>,
    /// `candidates_considered[]` — every candidate in the binding, verdicted
    /// (R-3).
    pub candidates_considered: Vec<Candidate>,
    /// `inputs_read[]` — the views consulted (R-4: profile resolver, budget
    /// account, health view, role table).
    pub inputs_read: Vec<String>,
    /// `deviation` — whether the realized model differs from the configured
    /// table (a reroute sets it).
    pub deviation: bool,
    /// `relower_required` — whether the choice crosses a profile boundary
    /// (a cross-profile reroute must `relower` first — ADR-0122 d.3).
    pub relower_required: bool,
}

/// A route-table candidate — the `ModelRef` plus its `ModelCoordinate` (the
/// selector-match view `resolve_profile` reads).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteCandidate {
    /// The `ModelRef`.
    pub model_ref: ModelRef,
    /// The selector coordinate.
    pub coordinate: ModelCoordinate,
}

/// `RoleBinding{model_ref (primary), alternates[], policy_ref, profile_ref}` —
/// one `ModelRoleTable` row (ADR-0121 d.2 + amendment).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleBinding {
    /// `model_ref` — the primary candidate.
    pub primary: RouteCandidate,
    /// `alternates[]`.
    pub alternates: Vec<RouteCandidate>,
    /// `policy_ref` — the row's routing-policy coordinate.
    pub policy_ref: String,
    /// `profile_ref` — the bound `ModelProfile` coordinate (the
    /// `profile_binding` projection).
    pub profile_ref: String,
}

/// `ModelRoleTable{roles: map<ModelRole, RoleBinding>}` — the sealed model set
/// (data in the sealed Harness Definition; its `semantic_id` is
/// `configuration_id.model_ref`, CF-313).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelRoleTable {
    /// `roles` — role spelling → binding.
    pub roles: BTreeMap<String, RoleBinding>,
}

/// The budget port — G-4's `reserve(budget_id, max_output(profile), holder)`
/// (ADR-0040). `Err` carries the refused dimension's spelling
/// (`InsufficientBudget{dimension}`).
pub trait BudgetPort {
    /// `reserve(budget_id, max_output_tokens, holder) → reservation_id`.
    fn reserve(
        &mut self,
        budget_id: &str,
        max_output_tokens: u64,
        holder: &str,
    ) -> Result<String, String>;
}

/// `AccountBudget` — the `hh_budget::Account` adapter (`AccountingHandle`):
/// reserves `tokens.output.visible = max_output(profile)` under the caller's
/// lease; a conservation refusal maps to its `dimension`.
pub struct AccountBudget<'a> {
    /// The accounting handle.
    pub account: &'a mut hh_budget::Account<'a>,
    /// The writer lease.
    pub lease: &'a hh_ledger::store::Lease,
    /// The reservation TTL (ms).
    pub ttl_ms: u64,
}

impl BudgetPort for AccountBudget<'_> {
    fn reserve(
        &mut self,
        budget_id: &str,
        max_output_tokens: u64,
        holder: &str,
    ) -> Result<String, String> {
        let quantity = hh_budget::ResourceVector::one(
            hh_budget::DimensionId::TokensOutputVisible,
            max_output_tokens as i64,
        );
        self.account
            .reserve(self.lease, budget_id, &quantity, holder, self.ttl_ms)
            .map_err(|e| match e {
                hh_budget::errors::BudgetError::InsufficientBudget { dimension, .. } => {
                    dimension.as_str().to_string()
                }
                other => format!("{other}"),
            })
    }
}

/// `HealthView` — the ledger-derived cooldown projection (`project(ledger,
/// view_kind = health, window)` — ADR-0122 d.4); a materialized view, never a
/// store of truth.
pub trait HealthView {
    /// Whether the candidate is in cooldown (`cooldown_until` ahead).
    fn cooldown(&self, candidate: &ModelRef) -> bool;
}

/// The empty view — no candidate cools down (the C0 default).
#[derive(Debug, Default)]
pub struct NoHealth;

impl HealthView for NoHealth {
    fn cooldown(&self, _candidate: &ModelRef) -> bool {
        false
    }
}

fn model_ref_spelling(m: &ModelRef) -> String {
    format!(
        "{}/{}{}",
        m.profile_ref,
        m.provider_model_id,
        m.serving_route
            .as_deref()
            .map(|r| format!("@{r}"))
            .unwrap_or_default()
    )
}

/// `select(request, profile_env, account, health, policy, table, decision_id)
/// → RoutingDecision | RoutingRefusal` — the C0 guard composition
/// (ADR-0121 d.1/d.5):
///
/// - **G-1** `resolve_profile` yields exactly one chain leaf with
///   `expiry.status ∈ {active, expiring}` — `expired` only with a recorded
///   `intent_ref`; `retired` never.
/// - **G-2** `required_capabilities ⊆ {c : capabilities[c] ∈ {declared,
///   probed}}` — `unknown` (or an unrecognised axis) ⇒ `CapabilityUnknown`
///   unless a debt-recorded `allow_unknown` rule exists.
/// - **G-4** `reserve(budget_id, max_output(profile), holder)` succeeds — the
///   reservation exists before the decision returns; `amend` never.
/// - Health: a candidate in cooldown is verdicted `rejected:cooldown` and
///   skipped.
///
/// Candidates run in binding order (primary, then alternates); every
/// candidate appears in `candidates_considered[]` with a verdict (R-3).
/// Total failure surfaces the shared refusal class when every candidate
/// failed identically, else `ChainExhausted{attempted[]}`.
pub fn select(
    request: &RoutingRequest,
    profile_env: &dyn SelectorView,
    account: &mut dyn BudgetPort,
    health: &dyn HealthView,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
    decision_id: &str,
) -> Result<RoutingDecision, RoutingRefusal> {
    // Policy validity first (T-LCD-05; a non-C0 kind at C0 is a declared
    // tier refusal — CC6, never a fall-through).
    policy.link_check()?;
    if !policy.kind.c0() {
        return Err(RoutingRefusal::PolicyInvalid {
            rule_id: policy.policy_id.clone(),
            reason: format!("kind {} is not a C0 kind", policy.kind.as_str()),
        });
    }

    // The candidate list — `static` reads `params.target`; `role_table` reads
    // the role's binding (G-1's "no row" is `NoProfile` on the role —
    // the binding names the model the profile must resolve for).
    let candidates: Vec<RouteCandidate> = match policy.kind {
        RoutingPolicyKind::Static => {
            let t = policy
                .params
                .get("target")
                .ok_or_else(|| RoutingRefusal::PolicyInvalid {
                    rule_id: policy.policy_id.clone(),
                    reason: "static policy without params.target".to_string(),
                })?;
            vec![
                candidate_from_json(t).ok_or_else(|| RoutingRefusal::PolicyInvalid {
                    rule_id: policy.policy_id.clone(),
                    reason: "static params.target malformed".to_string(),
                })?,
            ]
        }
        RoutingPolicyKind::RoleTable => match table.roles.get(&request.role) {
            Some(b) => std::iter::once(b.primary.clone())
                .chain(b.alternates.iter().cloned())
                .collect(),
            None => {
                return Err(RoutingRefusal::NoProfile {
                    model_ref: format!("role:{}", request.role),
                })
            }
        },
        _ => unreachable!("non-C0 kinds refused above"),
    };

    let mut verdicts: Vec<Candidate> = Vec::new();
    let mut refusals: Vec<RoutingRefusal> = Vec::new();
    let mut selected: Option<(RouteCandidate, String, String)> = None; // (candidate, leaf profile_ref, reservation_id)

    for cand in &candidates {
        let spelling = model_ref_spelling(&cand.model_ref);
        // Cooldown — the health view's verdict (never for a role with one
        // candidate per the view's own rule, but the view is the authority).
        if health.cooldown(&cand.model_ref) {
            verdicts.push(Candidate {
                model_ref: spelling.clone(),
                verdict: CandidateVerdict::Rejected(CandidateRejectReason::Cooldown),
                score: None,
            });
            refusals.push(RoutingRefusal::ChainExhausted { attempted: vec![] });
            continue;
        }
        // G-1 — exactly one profile resolves, `active | expiring` admissible.
        let chain = match resolve_profile(&cand.coordinate, profile_env) {
            Ok(c) => c,
            Err(ProfileResolveError::NoProfile { .. }) => {
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::NoProfile),
                    score: None,
                });
                refusals.push(RoutingRefusal::NoProfile {
                    model_ref: spelling.clone(),
                });
                continue;
            }
            Err(ProfileResolveError::AmbiguousSelector { candidates }) => {
                return Err(RoutingRefusal::PolicyInvalid {
                    rule_id: "selector".to_string(),
                    reason: format!("ambiguous_selector: {}", candidates.join(", ")),
                });
            }
            Err(ProfileResolveError::RetiredProfile { profile_ref, .. }) => {
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::ExpiredProfile),
                    score: None,
                });
                refusals.push(RoutingRefusal::ExpiredProfile {
                    profile_ref,
                    status: "retired".to_string(),
                });
                continue;
            }
        };
        let leaf = chain.profiles.last().expect("resolve_profile yields ≥1");
        let status = profile_status(leaf);
        let leaf_ref = hh_compiler::profile::profile_coordinate(leaf);
        match status {
            DebtStatus::Active | DebtStatus::Expiring => {}
            DebtStatus::Expired => {
                if request.intent_ref.is_none() {
                    verdicts.push(Candidate {
                        model_ref: spelling.clone(),
                        verdict: CandidateVerdict::Rejected(CandidateRejectReason::ExpiredProfile),
                        score: None,
                    });
                    refusals.push(RoutingRefusal::ExpiredProfile {
                        profile_ref: leaf_ref,
                        status: "expired".to_string(),
                    });
                    continue;
                }
            }
            DebtStatus::Retired => {
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::ExpiredProfile),
                    score: None,
                });
                refusals.push(RoutingRefusal::ExpiredProfile {
                    profile_ref: leaf_ref,
                    status: "retired".to_string(),
                });
                continue;
            }
        }
        // G-2 — `required_capabilities ⊆ declared | probed`. The tri-state has
        // no declared-negative at C0, so an unsatisfied axis reads `unknown`
        // (`CapabilityUnmet` remains in the closed sum for the C1 arms — a
        // measured bound below a required floor is *unmet*, not unknown).
        let mut unknown: Vec<String> = Vec::new();
        for cap in &request.required_capabilities {
            match leaf.capabilities.capability_state(cap) {
                Some(CapabilityState::Declared | CapabilityState::Probed) => {}
                // `context_window`/`max_output` are measured bounds — a
                // declared bound satisfies the axis.
                None if cap == "context_window" || cap == "max_output" => {
                    let bound = if cap == "context_window" {
                        leaf.capabilities.context_window
                    } else {
                        leaf.capabilities.max_output
                    };
                    if bound.is_none() {
                        unknown.push(cap.clone());
                    }
                }
                Some(CapabilityState::Unknown) | None => unknown.push(cap.clone()),
            }
        }
        if !unknown.is_empty() && policy.allow_unknown.is_none() {
            verdicts.push(Candidate {
                model_ref: spelling.clone(),
                verdict: CandidateVerdict::Rejected(CandidateRejectReason::CapabilityUnknown),
                score: None,
            });
            refusals.push(RoutingRefusal::CapabilityUnknown {
                model_ref: spelling.clone(),
                unknown,
            });
            continue;
        }
        // G-4 — `reserve(budget_id, max_output(profile), holder)` succeeds
        // before the decision returns. An undeclared `max_output` is a
        // `capability_unknown` — the reservation can't be sized honestly.
        let max_output = match leaf.capabilities.max_output {
            Some(m) => m,
            None => {
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::CapabilityUnknown),
                    score: None,
                });
                refusals.push(RoutingRefusal::CapabilityUnknown {
                    model_ref: spelling.clone(),
                    unknown: vec!["max_output".to_string()],
                });
                continue;
            }
        };
        match account.reserve(&request.budget_id, max_output, &request.holder) {
            Ok(reservation_id) => {
                selected = Some((cand.clone(), leaf_ref, reservation_id));
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Selected,
                    score: None,
                });
                break;
            }
            Err(dimension) => {
                verdicts.push(Candidate {
                    model_ref: spelling.clone(),
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::InsufficientBudget),
                    score: None,
                });
                refusals.push(RoutingRefusal::InsufficientBudget { dimension });
            }
        }
    }

    let (cand, leaf_ref, reservation_id) = match selected {
        Some(t) => t,
        None => {
            // Every candidate failed — the shared class surfaces when the
            // failures agree; distinct classes are `ChainExhausted`.
            let first = refusals
                .first()
                .cloned()
                .unwrap_or(RoutingRefusal::ChainExhausted { attempted: vec![] });
            if refusals.iter().all(|r| r.as_str() == first.as_str())
                && !matches!(first, RoutingRefusal::ChainExhausted { .. })
            {
                return Err(first);
            }
            return Err(RoutingRefusal::ChainExhausted {
                attempted: candidates
                    .iter()
                    .map(|c| model_ref_spelling(&c.model_ref))
                    .collect(),
            });
        }
    };

    let mut selected_ref = cand.model_ref.clone();
    selected_ref.profile_ref = leaf_ref;
    if let Some(e) = &request.effort {
        selected_ref.effort = Some(e.clone());
    }
    Ok(RoutingDecision {
        decision_id: decision_id.to_string(),
        model_call_id: request.model_call_id.clone(),
        role: request.role.clone(),
        selected: selected_ref,
        reservation_id: Some(reservation_id),
        policy_ref: format!("{}@{}", policy.policy_id, policy.version),
        policy_version_id: policy.content_hash.clone(),
        rule_ids_fired: policy
            .conditioned_rules
            .iter()
            .map(|r| r.rule_id.clone())
            .collect(),
        candidates_considered: verdicts,
        inputs_read: vec![
            "profile_resolver".to_string(),
            "budget_account".to_string(),
            "health_view".to_string(),
        ],
        deviation: false,
        relower_required: false,
    })
}

/// Decode a `RouteCandidate` from `params.target` (`{model_ref{…},
/// coordinate{provider_api_family, model_family, model_version}}`).
fn candidate_from_json(j: &Json) -> Option<RouteCandidate> {
    let m = j.get("model_ref")?;
    let c = j.get("coordinate")?;
    Some(RouteCandidate {
        model_ref: ModelRef {
            profile_ref: m.get("profile_ref").and_then(Json::as_str)?.to_string(),
            provider_model_id: m
                .get("provider_model_id")
                .and_then(Json::as_str)?
                .to_string(),
            snapshot_id: m
                .get("snapshot_id")
                .and_then(Json::as_str)
                .map(str::to_string),
            serving_route: m
                .get("serving_route")
                .and_then(Json::as_str)
                .map(str::to_string),
            effort: m.get("effort").and_then(Json::as_str).map(str::to_string),
        },
        coordinate: ModelCoordinate {
            provider_api_family: c
                .get("provider_api_family")
                .and_then(Json::as_str)?
                .to_string(),
            model_family: c.get("model_family").and_then(Json::as_str)?.to_string(),
            model_version: c.get("model_version").and_then(Json::as_str)?.to_string(),
        },
    })
}

/// `reroute(decision, to_binding, reason, attempt_no, relowered,
/// relower_event_ref)` — the ADR-0122 d.3 record. The caller owns the
/// ordering: `attempt.failed` → (optional `context.compaction.*` under the
/// old profile) → `model.surface.relowered` → `model.rerouted{…}` →
/// `attempt.started` under the new `profile_ref`; the old reservation is
/// released before the new one is held (ADR-0040). `relowered = true` iff the
/// reroute crossed a profile boundary.
pub fn reroute(
    decision: &RoutingDecision,
    to: &RouteCandidate,
    model_call_id: &str,
    reason: &RerouteReason,
    attempt_no: u32,
    relowered: bool,
    relower_event_ref: Option<&str>,
) -> Json {
    crate::events::rerouted(
        model_call_id,
        &decision.selected.provider_model_id,
        &to.model_ref.provider_model_id,
        reason,
        attempt_no,
        relowered,
        relower_event_ref,
        &decision.decision_id,
    )
}

/// The `error_actions` lookup — the class → action table is data on the
/// policy (AC-R-2.3.2-6); `GiveUp` (an absent row's closed default) closes
/// `model.call.failed`.
pub fn error_action(policy: &RoutingPolicy, class: &ModelErrorClass) -> ErrorAction {
    policy
        .error_actions
        .get(class.as_str())
        .cloned()
        .unwrap_or(ErrorAction::GiveUp)
}
