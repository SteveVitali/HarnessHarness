//! The `ModelRouter` (§5b.2; ADR-0121 d.1–d.7; ADR-0122 d.1–d.4):
//! `RoutingPolicy` as MUST-data with the closed kind enum, the
//! `ModelRoleTable` role→`RoleBinding` map, `select`/`select_with` under the
//! compatibility guard, `on_attempt_failed` (the retry-vs-reroute split —
//! ADR-0122 d.1/d.2), the `HealthView` ledger projection (ADR-0122 d.4),
//! `explain`, the closed `RoutingRefusal` sum, and the `RoutingDecision`
//! record `model.route.decided` carries.
//!
//! C0 lands `static`/`role_table` under G-1/G-2/G-4; **C1 (this slice)** lands
//! `fallback_chain`, `capability_filter`, `cost_cap`, `latency_cap`,
//! `quality_target` and `health_aware`, guards G-3 (projected
//! `TranscriptMigration` within `max_migration_loss`), G-5 (`NoPrice` when the
//! policy reads cost) and G-6 (lifted-preference attenuation), the
//! `HealthView` fold over `model.call.attempt.*`, and `on_attempt_failed`'s
//! `Continue | Reroute | GiveUp` with the old reservation released before the
//! new one is held (R-7). `learned` stays a declared C4 refusal (CC6).
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

/// `RoutingRequest{role, required_capabilities[], budget_id, holder,
/// model_call_id?, intent_ref?, effort?, latency_target?, quality_prior_ref?,
/// preferences?, source_profile_ref?, in_flight_effect}` — what `select`
/// consumes (§5b.2 `select(RoutingRequest, profile_env, account, health,
/// policy)`; the C1 members are additive — CC8).
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
    /// `latency_target?{p95_ms}` — the call's declared latency target the
    /// `latency_cap` policy reads against the health view's quantiles
    /// (§5b.2 `RoutingRequest.latency_target`; C1).
    pub latency_target_ms: Option<u64>,
    /// `quality_prior_ref?` — the `QualityPrior` record a `quality_target`
    /// policy reads (per-request override; `params.prior_ref` otherwise).
    pub quality_prior_ref: Option<String>,
    /// `preferences?` — lifted routing preferences (`authority ≤ external`;
    /// G-6 attenuation only — they may exclude, never add, ADR-0034).
    pub preferences: Option<LiftedPreferences>,
    /// `source_profile_ref?` — the profile the logical call was previously
    /// bound under; set on a reroute so G-3 can project the
    /// `TranscriptMigration` (C1).
    pub source_profile_ref: Option<String>,
    /// `in_flight_effect` — a proposed effect is unresolved (ADR-0030);
    /// G-3 refuses a cross-profile migration while it holds.
    pub in_flight_effect: bool,
    /// `task_class?` — the `bandit` policy's cell axis (ADR-0312 d.1:
    /// reward cells key `(task_class, model_ref)`; absent reads the
    /// `untagged` cell — never pooled with a tagged one).
    pub task_class: Option<String>,
}

/// `preferences{authority, cost_priority?, speed_priority?,
/// intelligence_priority?, exclude[], hints[]}` — lifted routing preferences
/// (MCP `ModelPreferences`/ACP model options ride `lift`; §5b.2).
/// `authority ≤ external` is the minting ceiling — a preference claiming
/// higher authority is `PolicyInvalid`, never silently trusted (CC2).
/// `exclude[]` names `model_ref`/`provider_model_id` spellings the preference
/// vetoes; G-6 admits exclusion only — a preference never adds a candidate
/// outside the role's binding.
#[derive(Debug, Clone, PartialEq)]
pub struct LiftedPreferences {
    /// The minted authority — `unverified` for imported content,
    /// `external` at most (ADR-0033 d.2).
    pub authority: hh_provenance::authority::AuthorityClass,
    /// `cost_priority` (ppm — higher prefers cheaper).
    pub cost_priority: Option<i64>,
    /// `speed_priority` (ppm — higher prefers lower latency).
    pub speed_priority: Option<i64>,
    /// `intelligence_priority` (ppm — higher prefers higher prior estimates).
    pub intelligence_priority: Option<i64>,
    /// `exclude[]` — `provider_model_id` or `model_ref` spellings the
    /// preference vetoes (G-6 attenuation).
    pub exclude: Vec<String>,
    /// `hints[]` — advisory spellings; recorded on the request, never used to
    /// admit a candidate outside the binding (G-6).
    pub hints: Vec<String>,
}

impl LiftedPreferences {
    /// G-6's legality check — the lifted authority must be `≤ external`
    /// (anything above is a minting violation; the router refuses the record
    /// rather than trusting it — CC2).
    pub fn legal(&self) -> bool {
        self.authority <= hh_provenance::authority::AuthorityClass::External
    }

    /// Whether the preference vetoes `spelling` (a `model_ref` spelling or a
    /// bare `provider_model_id`).
    pub fn excludes(&self, spelling: &str, provider_model_id: &str) -> bool {
        self.exclude
            .iter()
            .any(|e| e == spelling || e == provider_model_id)
    }
}

/// `RoutingPolicy.kind` — the closed kind enum (ADR-0121 d.4; extended by
/// ADR-0312 d.1 with `bandit`). C1 executes `static`, `role_table`,
/// `fallback_chain`, `capability_filter`, `cost_cap`, `latency_cap`,
/// `quality_target`, `health_aware`; the `bandit` family (C1/Stage-5)
/// ranks candidates by ledger-projected reward cells; `learned` is C4 — a
/// declared tier refusal (`PolicyInvalid`), never a fall-through.
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
    /// `bandit` (C1/Stage-5; ADR-0312 d.1) — rank by
    /// `reward = success_ppm − λ_ppm·cost_millis` over the projected
    /// `(task_class, model_ref)` cell view.
    Bandit,
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
            RoutingPolicyKind::Bandit => "bandit",
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
            "bandit" => RoutingPolicyKind::Bandit,
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

    /// Whether the kind executes at C1 (the full policy family minus
    /// `learned` — C4 stays a declared tier refusal; ADR-0121 d.4/d.7).
    pub fn c1(self) -> bool {
        !matches!(self, RoutingPolicyKind::Learned)
    }
}

/// `error_actions` value — the class → action table (ADR-0122 d.2):
/// `{retry_same{max, then?}, reroute, compact_then_retry{then?}, give_up}`.
/// `then` is the C1 sequencing member — the spec default table chains
/// `retry_same{max}` → `reroute` and `compact_then_retry` → `reroute`
/// (a retry policy is a bounded *sequence*, never a single verdict).
/// `GiveUp` closes `model.call.failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorAction {
    /// Retry the same target, bounded; `then` applies once `max` is spent.
    RetrySame {
        /// `max` — the same-target attempt bound (total attempts, incl. the
        /// first).
        max: u32,
        /// `then` — the follow-on action once `max` attempts are spent;
        /// `None` reads `give_up`.
        then: Option<Box<ErrorAction>>,
    },
    /// Reroute to another target (`model.rerouted`).
    Reroute,
    /// Compact the transcript, then retry (`context.compaction.*` under the
    /// old profile — ADR-0122 d.3 ordering); `then` applies when a
    /// compaction already ran for this call.
    CompactThenRetry {
        /// `then` — the follow-on after one compaction (default `reroute`
        /// under the spec table when chained; `None` reads `give_up`).
        then: Option<Box<ErrorAction>>,
    },
    /// Close the call `model.call.failed`.
    GiveUp,
}

impl ErrorAction {
    /// Decode a `to_json()` spelling — the bare strings
    /// (`"reroute"`/`"give_up"`/`"compact_then_retry"`) or the object forms
    /// (`{retry_same:{max, then?}}`, `{compact_then_retry:{then}}`).
    /// `None` on any malformed member — never a coerced default.
    pub fn from_json(j: &Json) -> Option<ErrorAction> {
        match j {
            Json::Str(s) => match s.as_str() {
                "reroute" => Some(ErrorAction::Reroute),
                "give_up" => Some(ErrorAction::GiveUp),
                "compact_then_retry" => Some(ErrorAction::CompactThenRetry { then: None }),
                _ => None,
            },
            Json::Obj(_) => {
                if let Some(inner) = j.get("retry_same") {
                    let max = inner.get("max")?.as_int()?;
                    if max < 0 {
                        return None;
                    }
                    let then = match inner.get("then") {
                        Some(t) => Some(Box::new(ErrorAction::from_json(t)?)),
                        None => None,
                    };
                    Some(ErrorAction::RetrySame {
                        max: max as u32,
                        then,
                    })
                } else if let Some(inner) = j.get("compact_then_retry") {
                    let then = match inner.get("then") {
                        Some(t) => Some(Box::new(ErrorAction::from_json(t)?)),
                        None => None,
                    };
                    Some(ErrorAction::CompactThenRetry { then })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorAction::RetrySame { .. } => "retry_same",
            ErrorAction::Reroute => "reroute",
            ErrorAction::CompactThenRetry { .. } => "compact_then_retry",
            ErrorAction::GiveUp => "give_up",
        }
    }

    /// Canonical JSON (`then` chains as a nested `then: {…}` member; absent
    /// `then` keeps the C0 spelling — CC8 additive).
    pub fn to_json(&self) -> Json {
        match self {
            ErrorAction::RetrySame { max, then } => {
                let mut inner = vec![("max", Json::Int(*max as i64))];
                if let Some(t) = then {
                    inner.push(("then", t.to_json()));
                }
                Json::obj([(
                    "retry_same",
                    Json::Obj(inner.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
                )])
            }
            ErrorAction::CompactThenRetry { then: Some(t) } => {
                Json::obj([("compact_then_retry", Json::obj([("then", t.to_json())]))])
            }
            ErrorAction::CompactThenRetry { then: None } => Json::str("compact_then_retry"),
            other => Json::str(other.as_str()),
        }
    }

    /// `then` — the chained follow-on (`None` reads `GiveUp` — the closed
    /// default for an absent action row).
    pub fn then(&self) -> &ErrorAction {
        match self {
            ErrorAction::RetrySame { then: Some(t), .. }
            | ErrorAction::CompactThenRetry { then: Some(t) } => t,
            _ => &ErrorAction::GiveUp,
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

    /// The spec `error_actions` default table (§5b.2 `error_actions`
    /// defaults; ADR-0122 d.2): transient classes retry the same target then
    /// reroute; `context_length_exceeded` compacts then retries then
    /// reroutes; `auth`/`not_found`/`unsupported_feature`/
    /// `transcript_incompatible` reroute and never retry the same target;
    /// `invalid_request`, the budget classes, `refusal` and `unknown` give
    /// up. The table is *data* — a policy document overrides per class.
    pub fn default_error_actions() -> BTreeMap<String, ErrorAction> {
        let mut m: BTreeMap<String, ErrorAction> = BTreeMap::new();
        let transient_then_reroute = |max: u32| ErrorAction::RetrySame {
            max,
            then: Some(Box::new(ErrorAction::Reroute)),
        };
        for c in [
            ModelErrorClass::Network,
            ModelErrorClass::Timeout(crate::vocab::TimeoutKind::Attempt),
            ModelErrorClass::Timeout(crate::vocab::TimeoutKind::Connect),
            ModelErrorClass::Timeout(crate::vocab::TimeoutKind::StreamIdle),
            ModelErrorClass::StreamDecode,
            ModelErrorClass::ServerError,
            ModelErrorClass::Overloaded,
            ModelErrorClass::RateLimited,
            ModelErrorClass::EmptyResponse,
        ] {
            m.insert(c.as_str().to_string(), transient_then_reroute(2));
        }
        m.insert(
            ModelErrorClass::ContextLengthExceeded.as_str().to_string(),
            ErrorAction::CompactThenRetry {
                then: Some(Box::new(ErrorAction::Reroute)),
            },
        );
        for c in [
            ModelErrorClass::Auth,
            ModelErrorClass::NotFound(crate::vocab::NotFoundKind::Model),
            ModelErrorClass::NotFound(crate::vocab::NotFoundKind::Endpoint),
            ModelErrorClass::UnsupportedFeature,
            ModelErrorClass::TranscriptIncompatible,
        ] {
            m.insert(c.as_str().to_string(), ErrorAction::Reroute);
        }
        for c in [
            ModelErrorClass::InvalidRequest,
            ModelErrorClass::PermanentRequest,
            ModelErrorClass::QuotaExhausted,
            ModelErrorClass::CreditsExhausted,
            ModelErrorClass::UsageNotIncluded,
            ModelErrorClass::VerificationRequired,
            ModelErrorClass::Forbidden,
        ] {
            m.insert(c.as_str().to_string(), ErrorAction::GiveUp);
        }
        // `content_policy`/`safety_blocked`/`refusal`/`served_model_mismatch`/
        // control classes are deliberate policy choices — no default row; an
        // absent row reads `GiveUp` (`error_action`'s closed default).
        m
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
        // ADR-0312 d.1 — a `bandit` policy's estimator is itself a
        // conditioned rule (ADR-0189: every non-`static` estimator carries
        // the debt record): at least one `conditioned_rules[]` entry must
        // hold a complete record. `link` refuses an incomplete one.
        if self.kind == RoutingPolicyKind::Bandit
            && !self
                .conditioned_rules
                .iter()
                .any(|r| r.debt.as_ref().is_some_and(|d| d.is_complete()))
        {
            return Err(RoutingRefusal::PolicyInvalid {
                rule_id: self.policy_id.clone(),
                reason: "bandit estimator requires a conditioned rule with \
                         a complete AssumptionDebtRecord (missing_debt_record)"
                    .to_string(),
            });
        }
        Ok(())
    }

    /// `to_json()`'s inverse — the sealed `router` slot's policy document
    /// decodes through this codec (R-2.7; CC1 — one spelling for the
    /// record). `Err` on any missing/malformed member: a sealed policy
    /// never coerces. An absent `content_hash` member computes
    /// `content_id()` (the declared address of the rest) — a present one
    /// is carried verbatim for the caller's seal check.
    pub fn from_json(j: &Json) -> Result<RoutingPolicy, String> {
        let str_at = |k: &str| {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("routing_policy.{k}: missing/not-string"))
        };
        let role_scope = match j.get("role_scope") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| i.as_str().map(str::to_string))
                .collect::<Option<BTreeSet<String>>>()
                .ok_or_else(|| "routing_policy.role_scope: not a string array".to_string())?,
            Some(_) => return Err("routing_policy.role_scope: not an array".to_string()),
            None => BTreeSet::new(),
        };
        let kind = RoutingPolicyKind::parse(&str_at("kind")?)
            .ok_or_else(|| "routing_policy.kind: unknown kind".to_string())?;
        let params = j.get("params").cloned().unwrap_or(Json::Null);
        let mut error_actions = BTreeMap::new();
        match j.get("error_actions") {
            Some(Json::Obj(m)) => {
                for (class, v) in m {
                    error_actions.insert(
                        class.clone(),
                        ErrorAction::from_json(v).ok_or_else(|| {
                            format!("routing_policy.error_actions.{class}: malformed")
                        })?,
                    );
                }
            }
            Some(_) => return Err("routing_policy.error_actions: not an object".to_string()),
            None => {}
        }
        let mml = j
            .get("max_migration_loss")
            .ok_or_else(|| "routing_policy.max_migration_loss: missing".to_string())?;
        let max_migration_loss = MigrationLossBound {
            dropped_items: mml
                .get("dropped_items")
                .and_then(Json::as_int)
                .filter(|d| *d >= 0)
                .ok_or_else(|| "routing_policy.max_migration_loss.dropped_items".to_string())?
                as u64,
            no_in_flight_tool_call: match mml.get("no_in_flight_tool_call") {
                Some(Json::Bool(b)) => *b,
                _ => {
                    return Err(
                        "routing_policy.max_migration_loss.no_in_flight_tool_call".to_string()
                    )
                }
            },
        };
        let allow_unknown = match j.get("allow_unknown") {
            None | Some(Json::Null) => None,
            Some(v) => Some(conditioned_rule_from_json(
                v,
                "routing_policy.allow_unknown",
            )?),
        };
        let conditioned_rules = match j.get("conditioned_rules") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    conditioned_rule_from_json(v, &format!("routing_policy.conditioned_rules[{i}]"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err("routing_policy.conditioned_rules: not an array".to_string()),
            None => Vec::new(),
        };
        let ext = match j.get("ext") {
            Some(Json::Obj(m)) => m.clone(),
            Some(_) => return Err("routing_policy.ext: not an object".to_string()),
            None => BTreeMap::new(),
        };
        let mut p = RoutingPolicy {
            policy_id: str_at("policy_id")?,
            // `version` is the canonical member; the sealed `router` slot's
            // params grammar spells it `policy_version` — a bare `version`
            // member inside a node's semantic projection trips T-10
            // (`IdentityIncludesSurface`). One decoder, two admitted
            // surface spellings; emission is always `version`.
            version: match str_at("version") {
                Ok(v) => v,
                Err(e) if j.get("policy_version").is_some() => {
                    let _ = e;
                    str_at("policy_version")?
                }
                Err(e) => return Err(e),
            },
            content_hash: String::new(),
            role_scope,
            kind,
            params,
            error_actions,
            max_migration_loss,
            allow_unknown,
            conditioned_rules,
            ext,
        };
        p.content_hash = match j.get("content_hash").and_then(Json::as_str) {
            Some(h) => h.to_string(),
            None => p.content_id(),
        };
        Ok(p)
    }
}

fn conditioned_rule_json(r: &PolicyConditionedRule) -> Json {
    let mut m = BTreeMap::new();
    m.insert("rule_id".into(), Json::str(r.rule_id.clone()));
    if let Some(k) = &r.conditioned_key {
        m.insert("conditioned_key".into(), Json::str(k.clone()));
    }
    if let Some(d) = &r.debt {
        // The record spelling (schema's `debt_to_json`), not the event
        // projection — `RoutingPolicy` is a sealed document and the
        // codec must round-trip `removal_test` & the additive members.
        m.insert("debt".into(), hh_compiler::schema::debt_to_json(d));
    }
    Json::Obj(m)
}

/// `conditioned_rule_json`'s inverse — `Err` on any malformed member (the
/// sealed document's debt record decodes through the compiler's codec —
/// CC1, never a second spelling).
fn conditioned_rule_from_json(j: &Json, path: &str) -> Result<PolicyConditionedRule, String> {
    let rule_id = j
        .get("rule_id")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.rule_id: missing"))?
        .to_string();
    let conditioned_key = j
        .get("conditioned_key")
        .and_then(Json::as_str)
        .map(str::to_string);
    let debt = match j.get("debt") {
        None | Some(Json::Null) => None,
        Some(d) => Some(
            hh_compiler::schema::debt_from_json(d, &format!("{path}.debt"))
                .map_err(|e| format!("{path}.debt: {e:?}"))?,
        ),
    };
    Ok(PolicyConditionedRule {
        rule_id,
        conditioned_key,
        debt,
    })
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
    /// Parse a canonical spelling — `None` on an unknown reason (the closed
    /// sum never coerces).
    pub fn parse(s: &str) -> Option<CandidateRejectReason> {
        Some(match s {
            "no_profile" => CandidateRejectReason::NoProfile,
            "expired_profile" => CandidateRejectReason::ExpiredProfile,
            "capability_unmet" => CandidateRejectReason::CapabilityUnmet,
            "capability_unknown" => CandidateRejectReason::CapabilityUnknown,
            "migration_loss" => CandidateRejectReason::MigrationLoss,
            "insufficient_budget" => CandidateRejectReason::InsufficientBudget,
            "no_price" => CandidateRejectReason::NoPrice,
            "cooldown" => CandidateRejectReason::Cooldown,
            "policy_excluded" => CandidateRejectReason::PolicyExcluded,
            "lower_score" => CandidateRejectReason::LowerScore,
            _ => return None,
        })
    }

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

impl RoutingDecision {
    /// Decode a `model.route.decided` payload (the `events::route_decided`
    /// spelling) — the durable-fold read path the driver's reroute
    /// execution replays a pending decision from (R-2.7; CC1 — one codec
    /// pair per record). `None` on any malformed member — a ledger row
    /// that can't be read is never silently defaulted.
    pub fn from_json(j: &Json) -> Option<RoutingDecision> {
        let bool_at = |k: &str| match j.get(k) {
            Some(Json::Bool(b)) => Some(*b),
            _ => None,
        };
        let str_arr = |k: &str| -> Option<Vec<String>> {
            match j.get(k) {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|i| i.as_str().map(str::to_string))
                    .collect(),
                _ => None,
            }
        };
        let mut candidates_considered = Vec::new();
        match j.get("candidates_considered") {
            Some(Json::Arr(items)) => {
                for c in items {
                    let model_ref = c.get("model_ref")?.as_str()?.to_string();
                    let verdict = match c.get("verdict")?.as_str()? {
                        "selected" => CandidateVerdict::Selected,
                        v => CandidateVerdict::Rejected(CandidateRejectReason::parse(
                            v.strip_prefix("rejected:")?,
                        )?),
                    };
                    let score = c.get("score").and_then(Json::as_int);
                    candidates_considered.push(Candidate {
                        model_ref,
                        verdict,
                        score,
                    });
                }
            }
            _ => return None,
        }
        let policy_ref = j.get("policy_ref")?;
        Some(RoutingDecision {
            decision_id: j.get("decision_id")?.as_str()?.to_string(),
            model_call_id: match j.get("model_call_id") {
                Some(Json::Null) | None => None,
                Some(v) => Some(v.as_str()?.to_string()),
            },
            role: j.get("role")?.as_str()?.to_string(),
            selected: model_ref_from_json(j.get("selected")?)?,
            reservation_id: match j.get("reservation_id") {
                Some(Json::Null) | None => None,
                Some(v) => Some(v.as_str()?.to_string()),
            },
            policy_ref: policy_ref.get("variant_ref")?.as_str()?.to_string(),
            policy_version_id: policy_ref.get("version_id")?.as_str()?.to_string(),
            rule_ids_fired: str_arr("rule_ids_fired")?,
            candidates_considered,
            inputs_read: str_arr("inputs_read")?,
            deviation: bool_at("deviation")?,
            relower_required: bool_at("relower_required")?,
        })
    }
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

/// The canonical `{provider_api_family, model_family, model_version}` member —
/// one codec shared by `RouteCandidate`/`RoleBinding`/`ModelRoleTable` (CC1).
fn coordinate_json(c: &ModelCoordinate) -> Json {
    Json::obj([
        (
            "provider_api_family",
            Json::str(c.provider_api_family.clone()),
        ),
        ("model_family", Json::str(c.model_family.clone())),
        ("model_version", Json::str(c.model_version.clone())),
    ])
}

/// Decode a `ModelCoordinate` — `None` on any missing/malformed member (the
/// closed grammar refuses coerced defaults).
fn coordinate_from_json(j: &Json) -> Option<ModelCoordinate> {
    Some(ModelCoordinate {
        provider_api_family: j.get("provider_api_family")?.as_str()?.to_string(),
        model_family: j.get("model_family")?.as_str()?.to_string(),
        model_version: j.get("model_version")?.as_str()?.to_string(),
    })
}

/// Decode a `ModelRef` — `None` on a missing required member
/// (`profile_ref`/`provider_model_id`); optionals decode when present.
fn model_ref_from_json(j: &Json) -> Option<ModelRef> {
    Some(ModelRef {
        profile_ref: j.get("profile_ref")?.as_str()?.to_string(),
        provider_model_id: j.get("provider_model_id")?.as_str()?.to_string(),
        snapshot_id: j
            .get("snapshot_id")
            .and_then(Json::as_str)
            .map(str::to_string),
        serving_route: j
            .get("serving_route")
            .and_then(Json::as_str)
            .map(str::to_string),
        effort: j.get("effort").and_then(Json::as_str).map(str::to_string),
    })
}

impl RouteCandidate {
    /// Canonical JSON — `{model_ref, coordinate}`.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("model_ref", self.model_ref.to_json()),
            ("coordinate", coordinate_json(&self.coordinate)),
        ])
    }
}

impl RoleBinding {
    /// Canonical JSON — `{model_ref, alternates[], policy_ref, profile_ref}`
    /// (the ADR-0121 d.2-amended row shape: `model_ref` is the primary
    /// candidate, `profile_ref` the bound `ModelProfile` coordinate).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("model_ref", self.primary.model_ref.to_json()),
            ("coordinate", coordinate_json(&self.primary.coordinate)),
            (
                "alternates",
                Json::Arr(self.alternates.iter().map(|a| a.to_json()).collect()),
            ),
            ("policy_ref", Json::str(self.policy_ref.clone())),
            ("profile_ref", Json::str(self.profile_ref.clone())),
        ])
    }

    /// Decode — `None` on any malformed member (never a partial binding).
    pub fn from_json(j: &Json) -> Option<RoleBinding> {
        Some(RoleBinding {
            primary: RouteCandidate {
                model_ref: model_ref_from_json(j.get("model_ref")?)?,
                coordinate: coordinate_from_json(j.get("coordinate")?)?,
            },
            alternates: match j.get("alternates") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(candidate_from_json)
                    .collect::<Option<Vec<_>>>()?,
                Some(_) => return None,
                None => Vec::new(),
            },
            policy_ref: j.get("policy_ref")?.as_str()?.to_string(),
            profile_ref: j.get("profile_ref")?.as_str()?.to_string(),
        })
    }
}

impl ModelRoleTable {
    /// Canonical JSON — `{roles: {role → RoleBinding}}` (BTreeMap ordering is
    /// the canonical order — the id is stable across processes, CC3).
    pub fn to_json(&self) -> Json {
        Json::obj([(
            "roles",
            Json::Obj(
                self.roles
                    .iter()
                    .map(|(r, b)| (r.clone(), b.to_json()))
                    .collect(),
            ),
        )])
    }

    /// Decode — role keys are the closed `ModelRole` set (CF-468: the older
    /// `summarizer`/`shim_interpreter`/`subagent(class)` spellings never
    /// decode — `compaction`/`utility`/`subagent` are the ratified names).
    pub fn from_json(j: &Json) -> Option<ModelRoleTable> {
        let mut roles = BTreeMap::new();
        match j.get("roles") {
            Some(Json::Obj(m)) => {
                for (role, b) in m {
                    // The role key must name a ratified `ModelRole`.
                    hh_compiler::profile::ModelRole::parse(role)?;
                    roles.insert(role.clone(), RoleBinding::from_json(b)?);
                }
            }
            Some(_) | None => return None,
        }
        Some(ModelRoleTable { roles })
    }

    /// `semantic_id` — the idp/1 address of the canonical record (CF-313:
    /// `configuration_id.model_ref` names *this* id, never a surface alias —
    /// the model set is a factor coordinate, `bundle_id` never is).
    pub fn semantic_id(&self) -> String {
        hh_identity::idp_id(
            "model_role_table.1",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// The `profile_binding` projection — `map<ModelRole, ProfileRef>`
    /// (`{role → {profile_ref, pinned: true}}`; CF-313: the sealed table's
    /// profile view; the manifest carries this member, `native.profile` is
    /// the `primary` row).
    pub fn profile_binding(&self) -> Json {
        Json::Obj(
            self.roles
                .iter()
                .map(|(r, b)| {
                    (
                        r.clone(),
                        Json::obj([
                            ("profile_ref", Json::str(b.profile_ref.clone())),
                            ("pinned", Json::Bool(true)),
                        ]),
                    )
                })
                .collect(),
        )
    }
}

/// The budget port — G-4's `reserve(budget_id, max_output(profile), holder)`
/// (ADR-0040) and R-7's `release` (the old reservation is released before the
/// reroute's new one is held). `Err` carries the refused dimension's spelling
/// (`InsufficientBudget{dimension}`).
pub trait BudgetPort {
    /// `reserve(budget_id, max_output_tokens, holder) → reservation_id`.
    fn reserve(
        &mut self,
        budget_id: &str,
        max_output_tokens: u64,
        holder: &str,
    ) -> Result<String, String>;
    /// `release(reservation_id)` — the reservation's unspent remainder returns
    /// to its budget (ADR-0040's affine rule — never aliased, never dropped).
    /// A failed release is a typed error the caller surfaces; the router never
    /// proceeds to hold the new reservation while the old one is live.
    fn release(&mut self, reservation_id: &str) -> Result<(), String>;
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

    fn release(&mut self, reservation_id: &str) -> Result<(), String> {
        self.account
            .release(self.lease, reservation_id)
            .map(|_| ())
            .map_err(|e| format!("{e}"))
    }
}

/// `TargetHealth{attempts, failures, p95_ms?, cooldown_until_ms?}` — one
/// `(provider_model_id, serving_route)` row of the `HealthView` projection
/// (ADR-0122 d.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TargetHealth {
    /// Attempts observed in the window.
    pub attempts: u64,
    /// Failed attempts observed in the window.
    pub failures: u64,
    /// The p95 completed-attempt latency in the window (ms).
    pub p95_ms: Option<u64>,
    /// `cooldown_until` (ms since epoch) when set — the candidate cools down
    /// while `now < cooldown_until_ms`.
    pub cooldown_until_ms: Option<u64>,
}

impl TargetHealth {
    /// The observed failure rate (ppm; `None` with no attempts).
    pub fn failure_rate_ppm(&self) -> Option<i64> {
        if self.attempts == 0 {
            None
        } else {
            Some(self.failures as i64 * hh_budget::quantity::PPM_SCALE / self.attempts as i64)
        }
    }
}

/// `HealthView` — the ledger-derived health projection (`project(ledger,
/// view_kind = health, window)` — ADR-0122 d.4); a materialized view, never a
/// store of truth. `stats` carries the C1 quantiles/failure counts
/// `latency_cap`/`health_aware` read; `cooldown` is the C0 predicate — it
/// takes `now_ms` (a caller-supplied clock; `select` stays pure — AC-11).
pub trait HealthView {
    /// Whether the candidate is in cooldown at `now_ms` (`cooldown_until`
    /// ahead of now — an expired cooldown is no cooldown).
    fn cooldown(&self, candidate: &ModelRef, now_ms: u64) -> bool {
        self.stats(candidate)
            .and_then(|s| s.cooldown_until_ms)
            .is_some_and(|until| now_ms < until)
    }
    /// The candidate's health row (`None` = unobserved — the candidate is
    /// fresh, not refused; a projection that never observed an attempt has
    /// nothing to say).
    fn stats(&self, _candidate: &ModelRef) -> Option<TargetHealth> {
        None
    }
}

/// The empty view — no candidate cools down (the C0 default).
#[derive(Debug, Default)]
pub struct NoHealth;

impl HealthView for NoHealth {
    fn cooldown(&self, _candidate: &ModelRef, _now_ms: u64) -> bool {
        false
    }
}

/// `HealthConfig` — the projection's declared thresholds (OQ-295's MUST-data
/// placeholder values are configuration, never hard-coded — the view is
/// honest about which window/threshold it applies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthConfig {
    /// `429`/`rate_limited` cooldown length (ms).
    pub rate_limited_cooldown_ms: u64,
    /// Any-failure cooldown length (ms) once the rate threshold trips.
    pub failure_cooldown_ms: u64,
    /// Failure-rate threshold (ppm of attempts) that trips a cooldown.
    pub failure_rate_threshold_ppm: i64,
    /// Minimum attempts before the rate check applies.
    pub min_attempts: u64,
    /// The trailing window in seq terms (attempts counted).
    pub window_attempts: usize,
}

impl Default for HealthConfig {
    /// The OQ-295 placeholder defaults (MUST-data until the open question
    /// ratifies values — recorded in the run doc).
    fn default() -> HealthConfig {
        HealthConfig {
            rate_limited_cooldown_ms: 30_000,
            failure_cooldown_ms: 60_000,
            failure_rate_threshold_ppm: 500_000,
            min_attempts: 4,
            window_attempts: 64,
        }
    }
}

/// `TableHealth` — the materialized `HealthView` over
/// `model.call.attempt.{started,completed,failed}` joined to each call's
/// `model.route.decided`/`model.call.requested` target. A projection — it
/// carries `derived_from` watermarks (the folded `seq` ceiling), never a
/// store of truth (ADR-0042).
#[derive(Debug, Clone, Default)]
pub struct TableHealth {
    /// `(provider_model_id, serving_route)` → health row.
    pub rows: BTreeMap<(String, String), TargetHealth>,
    /// The projection's watermark (the last folded seq).
    pub watermark_seq: u64,
}

impl HealthView for TableHealth {
    fn stats(&self, candidate: &ModelRef) -> Option<TargetHealth> {
        self.rows
            .get(&(
                candidate.provider_model_id.clone(),
                candidate.serving_route.clone().unwrap_or_default(),
            ))
            .copied()
    }
}

/// `project_health(rows, cfg)` — the ADR-0122 d.4 fold. `rows` are the run's
/// `(seq, ts_ms, class, payload)` projection inputs over
/// `model.call.requested`/`model.route.decided` (the call→target join) and
/// `model.call.attempt.{started,completed,failed}` (the observations):
///
/// - attempts/failures are counted per `(provider_model_id, serving_route)`
///   (failures keep their `error.class` spelling);
/// - `p95_ms` is the 95th-percentile `duration_ms` over completed attempts;
/// - `429`/`rate_limited` ⇒ `cooldown_until = failed_at +
///   cfg.rate_limited_cooldown_ms` (the spec default);
/// - failure rate > `failure_rate_threshold_ppm` over ≥ `min_attempts` ⇒
///   `cooldown_until = last_failed_at + cfg.failure_cooldown_ms`.
///
/// `single_candidate` carries the `(provider_model_id, serving_route)` keys
/// of roles whose binding holds one candidate — the view never cools those
/// down (the spec's "never for a role with one candidate").
pub fn project_health(
    rows: &[(u64, u64, &str, &Json)],
    cfg: &HealthConfig,
    single_candidate: &BTreeSet<(String, String)>,
) -> TableHealth {
    // model_call_id → target (the call→route join).
    let mut target_of: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (_, _, class, p) in rows {
        match *class {
            "model.route.decided" => {
                let id = p.get("model_call_id").and_then(Json::as_str);
                let model = p
                    .get("selected")
                    .and_then(|s| s.get("provider_model_id"))
                    .and_then(Json::as_str);
                let route = p
                    .get("selected")
                    .and_then(|s| s.get("serving_route"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if let (Some(id), Some(m)) = (id, model) {
                    target_of.insert(id.to_string(), (m.to_string(), route.to_string()));
                }
            }
            "model.call.requested" => {
                let id = p.get("model_call_id").and_then(Json::as_str);
                let model = p
                    .get("model_ref")
                    .and_then(|s| s.get("provider_model_id"))
                    .and_then(Json::as_str);
                let route = p
                    .get("model_ref")
                    .and_then(|s| s.get("serving_route"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if let (Some(id), Some(m)) = (id, model) {
                    target_of
                        .entry(id.to_string())
                        .or_insert_with(|| (m.to_string(), route.to_string()));
                }
            }
            _ => {}
        }
    }
    struct Agg {
        /// `(seq, failed, duration_ms?, ts_ms)` per observed attempt outcome.
        attempts: Vec<(u64, bool, Option<u64>, u64)>,
        /// The largest `rate_limited`-stamped cooldown (ms).
        rl_cooldown_until_ms: Option<u64>,
        last_failed_at: u64,
    }
    let mut per_target: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut watermark = 0u64;
    for (seq, ts_ms, class, p) in rows {
        watermark = watermark.max(*seq);
        let Some(id) = p.get("model_call_id").and_then(Json::as_str) else {
            continue;
        };
        let Some(target) = target_of.get(id).cloned() else {
            continue;
        };
        let agg = per_target.entry(target).or_insert_with(|| Agg {
            attempts: vec![],
            rl_cooldown_until_ms: None,
            last_failed_at: 0,
        });
        match *class {
            "model.call.attempt.started" => {
                agg.attempts.push((*seq, false, None, *ts_ms));
            }
            "model.call.attempt.completed" => {
                let d = p
                    .get("duration_ms")
                    .and_then(Json::as_int)
                    .filter(|d| *d >= 0)
                    .map(|d| d as u64);
                // A completion supersedes the opened attempt's placeholder.
                let attempt_no = p.get("attempt_no").and_then(Json::as_int);
                if let Some(open) = agg
                    .attempts
                    .iter_mut()
                    .rev()
                    .find(|(_, failed, dur, _)| !*failed && dur.is_none())
                {
                    *open = (*seq, false, d, *ts_ms);
                } else {
                    agg.attempts.push((*seq, false, d, *ts_ms));
                }
                let _ = attempt_no;
            }
            "model.call.attempt.failed" => {
                agg.attempts.push((*seq, true, None, *ts_ms));
                agg.last_failed_at = *ts_ms;
                let class_spelling = p
                    .get("error")
                    .and_then(|e| e.get("class"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if class_spelling == ModelErrorClass::RateLimited.as_str() {
                    agg.rl_cooldown_until_ms = agg
                        .rl_cooldown_until_ms
                        .max(Some(*ts_ms + cfg.rate_limited_cooldown_ms));
                }
            }
            _ => {}
        }
    }
    let mut table = TableHealth {
        rows: BTreeMap::new(),
        watermark_seq: watermark,
    };
    for (key, agg) in per_target {
        let solo = single_candidate.contains(&key);
        // The trailing window — the last `window_attempts` observations.
        let window: Vec<(u64, bool, Option<u64>, u64)> = agg
            .attempts
            .iter()
            .rev()
            .take(cfg.window_attempts.max(1))
            .cloned()
            .collect();
        let attempts = window.len() as u64;
        let failures = window.iter().filter(|(_, f, _, _)| *f).count() as u64;
        let mut latencies: Vec<u64> = window
            .iter()
            .filter_map(|(_, f, d, _)| if *f { None } else { *d })
            .collect();
        latencies.sort_unstable();
        let p95 = if latencies.is_empty() {
            None
        } else {
            let ix = (latencies.len() as u64 * 95)
                .div_ceil(100)
                .saturating_sub(1) as usize;
            Some(latencies[ix.min(latencies.len() - 1)])
        };
        let mut cooldown = agg.rl_cooldown_until_ms;
        if !solo
            && attempts >= cfg.min_attempts
            && failures as i64 * hh_budget::quantity::PPM_SCALE / attempts.max(1) as i64
                > cfg.failure_rate_threshold_ppm
        {
            cooldown = cooldown.max(Some(agg.last_failed_at + cfg.failure_cooldown_ms));
        }
        // The "never for a role with one candidate" rule.
        if solo {
            cooldown = None;
        }
        table.rows.insert(
            key,
            TargetHealth {
                attempts,
                failures,
                p95_ms: p95,
                cooldown_until_ms: cooldown,
            },
        );
    }
    table
}

/// `PricingView` — the pricing-table lookup `cost_cap`/G-5 reads
/// (ADR-0039's `record_kind:pricing_table` projected per `ModelRef`).
/// `None` is `NoPrice` — never a zero-cost fallback (ADR-0121 d.5).
pub trait PricingView {
    /// The table's per-call cost estimate for `candidate` under
    /// `pricing_table_ref` at `max_output_tokens` reserve size (the table's
    /// smallest currency unit).
    fn estimate_call_cost(
        &self,
        candidate: &RouteCandidate,
        pricing_table_ref: &str,
        max_output_tokens: u64,
    ) -> Option<i64>;
}

/// The empty pricing view — every cost read is `NoPrice` (fail-closed).
#[derive(Debug, Default)]
pub struct NoPricing;

impl PricingView for NoPricing {
    fn estimate_call_cost(
        &self,
        _candidate: &RouteCandidate,
        _pricing_table_ref: &str,
        _max_output_tokens: u64,
    ) -> Option<i64> {
        None
    }
}

/// `QualityPriorView` — the `QualityPrior`/`FittedSurfaceReport` lookup a
/// `quality_target` policy reads (ADR-0160; read-only, never a score the
/// router invents). Estimates are ppm-scale against the policy's `metric`.
pub trait QualityPriorView {
    /// The prior's estimate (ppm) for `candidate` under `prior_ref`/`metric`;
    /// `None` when no covering prior exists (`unknown` levels refused —
    /// ADR-0012).
    fn estimate_ppm(
        &self,
        candidate: &RouteCandidate,
        prior_ref: &str,
        metric: &str,
    ) -> Option<i64>;
}

/// The empty prior view — no estimate exists (fail-closed).
#[derive(Debug, Default)]
pub struct NoQualityPrior;

impl QualityPriorView for NoQualityPrior {
    fn estimate_ppm(
        &self,
        _candidate: &RouteCandidate,
        _prior_ref: &str,
        _metric: &str,
    ) -> Option<i64> {
        None
    }
}

/// `MigrationView` — the projected `TranscriptMigration` a G-3 check reads
/// (ADR-0019 `relower`). `Some(d)` is the projected dropped-item count;
/// `None` is *unprojectable* — fail-closed, the candidate verdicts
/// `migration_loss` (a migration the router cannot bound is never admitted).
pub trait MigrationView {
    /// Project the dropped-item count of a relower from `from_profile_ref` to
    /// the candidate's leaf profile.
    fn projected_dropped(&self, from_profile_ref: &str, to_profile_ref: &str) -> Option<u64>;
}

/// The empty migration view — `Some(0)` for a same-profile move (no
/// migration), `None` across profiles (unprojectable ⇒ fail-closed).
#[derive(Debug, Default)]
pub struct NoMigration;

impl MigrationView for NoMigration {
    fn projected_dropped(&self, from_profile_ref: &str, to_profile_ref: &str) -> Option<u64> {
        if from_profile_ref == to_profile_ref {
            Some(0)
        } else {
            None
        }
    }
}

/// `BanditCell` — one projected reward cell keyed `(task_class,
/// model_ref)` (ADR-0312 d.1). The view is a kernel-derived projection of
/// the ledger's `model.call.completed` rows — never hidden mutable state
/// the router owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BanditCell {
    /// `n` — observations in the cell. Below the policy's `min_n` the cell
    /// reads `unknown` (ADR-0189 D6 applied at the router family).
    pub n: u64,
    /// Mean task-success rate in ppm (the deterministic-oracle /
    /// validator-verdict fold the view computes).
    pub success_ppm: i64,
    /// Mean realized call cost in milli-units of the pinned pricing
    /// table's currency (the λ·cost operand — `estimated_from_pricing`
    /// rows never feed a reward).
    pub cost_millis: i64,
}

/// `BanditView` — the projected reward store a `bandit` policy reads
/// (ADR-0312 d.1). Cells are keyed `(task_class, model_ref_spelling)`;
/// there is no cross-cell pooling — an unobserved key is `None` and reads
/// as `unknown` under `min_n`.
pub trait BanditView {
    /// The cell for `(task_class, model_ref)`, `None` when unobserved.
    fn cell(&self, task_class: &str, model_ref: &str) -> Option<BanditCell>;
    /// The total observation count across a task_class's cells — the
    /// exploration bonus's `N` term.
    fn total_n(&self, task_class: &str) -> u64;
}

/// The empty bandit view — every cell `unknown` (the `cold_start` prior
/// ranks; nothing is invented).
#[derive(Debug, Default)]
pub struct NoBandit;

impl BanditView for NoBandit {
    fn cell(&self, _task_class: &str, _model_ref: &str) -> Option<BanditCell> {
        None
    }
    fn total_n(&self, _task_class: &str) -> u64 {
        0
    }
}

/// `RoutingViews` — the C1 view bundle `select_with` reads (the spec's
/// `profile_env`/`account`/`health` are already parameters; these are the
/// G-3/G-5/quality-prior/reward projections the C1 kinds consult).
#[derive(Default)]
pub struct RoutingViews<'a> {
    /// The pricing table view (G-5).
    pub pricing: Option<&'a dyn PricingView>,
    /// The quality-prior view (`quality_target`).
    pub quality: Option<&'a dyn QualityPriorView>,
    /// The migration projection (G-3).
    pub migration: Option<&'a dyn MigrationView>,
    /// The reward-cell projection (`bandit`; ADR-0312 d.1).
    pub bandit: Option<&'a dyn BanditView>,
}

impl RoutingViews<'_> {
    /// No views — `NoPricing`/`NoQualityPrior`/`NoMigration`/`NoBandit`
    /// semantics (fail-closed on the guard that reads them).
    pub fn none() -> RoutingViews<'static> {
        RoutingViews {
            pricing: None,
            quality: None,
            migration: None,
            bandit: None,
        }
    }
}

/// `⌊√(total·10⁶/n)⌋` — the `bandit` exploration bonus's √ratio spelled
/// in milli-units so `c_ppm·isqrt_ppm(N,n)/1000` reads `c_ppm·√(N/n)`.
/// Pure integer Newton — deterministic, no floats (ADR-0312 d.1).
fn isqrt_ppm(total: u64, n: u64) -> i64 {
    let n = n.max(1) as u128;
    let v = (total as u128).saturating_mul(1_000_000) / n;
    if v == 0 {
        return 0;
    }
    let mut x = v;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    (x.min(i64::MAX as u128)) as i64
}

/// The canonical `model_ref` spelling a route decision's `attempted` set
/// keys on — `{profile_ref}/{provider_model_id}[@{serving_route}]` (CC1:
/// the driver rebuilds the durable `AttemptState.attempted` through this
/// one spelling — never a second derivation).
pub fn model_ref_spelling(m: &ModelRef) -> String {
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

/// A candidate the guard admitted, before the budget reserve — the per-kind
/// filters may still attach a `score` (the `quality_target`/`health_aware`
/// ranking inputs; `policy_excluded`/`no_price`/`cooldown` verdicts carry the
/// typed refuse already).
struct Admitted {
    /// The resolved leaf profile coordinate (post-G-1).
    leaf_ref: String,
    /// `max_output` the G-4 reserve sizes against.
    max_output: u64,
    /// The kind's score (`quality_target`'s prior estimate,
    /// `health_aware`'s inverted failure rate — higher is better).
    score: Option<i64>,
}

/// The admission pipeline's tri-state — a rejected candidate carries its
/// closed reason + the refusal class for aggregation; `Fatal` is a guard
/// failure that refuses the whole `select` (an ambiguous selector is a
/// link-time invalidity — `PolicyInvalid` — never absorbed as a candidate
/// verdict).
enum Admit {
    /// The candidate passed G-1…G-5 and the kind's filters.
    Admitted(Admitted),
    /// The candidate was rejected with a closed reason.
    Rejected(CandidateRejectReason, RoutingRefusal),
    /// The whole selection refuses (a policy/link invalidity).
    Fatal(RoutingRefusal),
}

/// The per-candidate admission pipeline — G-1 → G-2 → G-3 → G-5 → kind
/// filters — before the G-4 reserve (the reserve binds only the selected
/// candidate; a rejected candidate never holds a reservation).
#[allow(clippy::too_many_arguments)] // the arity is the guard's
fn admit_candidate(
    cand: &RouteCandidate,
    required_capabilities: &[String],
    request: &RoutingRequest,
    profile_env: &dyn SelectorView,
    health: &dyn HealthView,
    now_ms: u64,
    views: &RoutingViews,
    policy: &RoutingPolicy,
    extra_capabilities: &[String],
) -> Admit {
    let spelling = model_ref_spelling(&cand.model_ref);
    let reject =
        |reason: CandidateRejectReason, refusal: RoutingRefusal| Admit::Rejected(reason, refusal);
    // Health — cooldown is a view verdict (ADR-0122 d.4).
    if health.cooldown(&cand.model_ref, now_ms) {
        return reject(
            CandidateRejectReason::Cooldown,
            RoutingRefusal::ChainExhausted { attempted: vec![] },
        );
    }
    // G-1 — exactly one profile resolves, `active | expiring` admissible.
    let chain = match resolve_profile(&cand.coordinate, profile_env) {
        Ok(c) => c,
        Err(ProfileResolveError::NoProfile { .. }) => {
            return reject(
                CandidateRejectReason::NoProfile,
                RoutingRefusal::NoProfile {
                    model_ref: spelling.clone(),
                },
            );
        }
        Err(ProfileResolveError::AmbiguousSelector { candidates }) => {
            // A link-time invalidity — the whole `select` refuses, not just
            // this candidate (T-LCD-05 class).
            return Admit::Fatal(RoutingRefusal::PolicyInvalid {
                rule_id: "selector".to_string(),
                reason: format!("ambiguous_selector: {}", candidates.join(", ")),
            });
        }
        Err(ProfileResolveError::RetiredProfile { profile_ref, .. }) => {
            return reject(
                CandidateRejectReason::ExpiredProfile,
                RoutingRefusal::ExpiredProfile {
                    profile_ref,
                    status: "retired".to_string(),
                },
            );
        }
    };
    let leaf = chain.profiles.last().expect("resolve_profile yields ≥1");
    let status = profile_status(leaf);
    let leaf_ref = hh_compiler::profile::profile_coordinate(leaf);
    match status {
        DebtStatus::Active | DebtStatus::Expiring => {}
        DebtStatus::Expired => {
            if request.intent_ref.is_none() {
                return reject(
                    CandidateRejectReason::ExpiredProfile,
                    RoutingRefusal::ExpiredProfile {
                        profile_ref: leaf_ref,
                        status: "expired".to_string(),
                    },
                );
            }
        }
        DebtStatus::Retired => {
            return reject(
                CandidateRejectReason::ExpiredProfile,
                RoutingRefusal::ExpiredProfile {
                    profile_ref: leaf_ref,
                    status: "retired".to_string(),
                },
            );
        }
    }
    // G-2 — `required_capabilities ∪ kind-declared ⊆ declared | probed`. The
    // tri-state has no declared-negative at C1, so an unsatisfied axis reads
    // `unknown` (`CapabilityUnmet` remains for measured-bound floors).
    let mut unknown: Vec<String> = Vec::new();
    for cap in required_capabilities
        .iter()
        .chain(extra_capabilities.iter())
    {
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
        return reject(
            CandidateRejectReason::CapabilityUnknown,
            RoutingRefusal::CapabilityUnknown {
                model_ref: spelling.clone(),
                unknown,
            },
        );
    }
    // An undeclared `max_output` is a `capability_unknown` — the reservation
    // can't be sized honestly.
    let max_output = match leaf.capabilities.max_output {
        Some(m) => m,
        None => {
            return reject(
                CandidateRejectReason::CapabilityUnknown,
                RoutingRefusal::CapabilityUnknown {
                    model_ref: spelling.clone(),
                    unknown: vec!["max_output".to_string()],
                },
            );
        }
    };
    // G-3 — a cross-profile move is legal only under a projected migration
    // within `max_migration_loss`, and never while a proposed effect is
    // unresolved (ADR-0030).
    if let Some(src) = &request.source_profile_ref {
        if *src != leaf_ref {
            if request.in_flight_effect && policy.max_migration_loss.no_in_flight_tool_call {
                return reject(
                    CandidateRejectReason::MigrationLoss,
                    RoutingRefusal::MigrationLossExceeded {
                        dropped: 0,
                        bound: policy.max_migration_loss.dropped_items,
                    },
                );
            }
            let migration: &dyn MigrationView = views.migration.unwrap_or(&NoMigration);
            match migration.projected_dropped(src, &leaf_ref) {
                Some(d) if d <= policy.max_migration_loss.dropped_items => {}
                Some(d) => {
                    return reject(
                        CandidateRejectReason::MigrationLoss,
                        RoutingRefusal::MigrationLossExceeded {
                            dropped: d,
                            bound: policy.max_migration_loss.dropped_items,
                        },
                    );
                }
                // Unprojectable ⇒ the bound can't be shown — fail-closed.
                None => {
                    return reject(
                        CandidateRejectReason::MigrationLoss,
                        RoutingRefusal::MigrationLossExceeded {
                            dropped: u64::MAX,
                            bound: policy.max_migration_loss.dropped_items,
                        },
                    );
                }
            }
        }
    }
    // G-5 + kind filters — the policy data decides which views are read.
    let mut score: Option<i64> = None;
    match policy.kind {
        RoutingPolicyKind::CostCap => {
            let table_ref = policy
                .params
                .get("pricing_table_ref")
                .and_then(Json::as_str)
                .unwrap_or("");
            let cap = policy
                .params
                .get("max_spend_per_call")
                .and_then(Json::as_int);
            let pricing: &dyn PricingView = views.pricing.unwrap_or(&NoPricing);
            match pricing.estimate_call_cost(cand, table_ref, max_output) {
                None => {
                    return reject(
                        CandidateRejectReason::NoPrice,
                        RoutingRefusal::NoPrice {
                            model_ref: spelling.clone(),
                        },
                    );
                }
                Some(cost) => {
                    if let Some(cap) = cap {
                        if cost > cap {
                            return reject(
                                CandidateRejectReason::PolicyExcluded,
                                RoutingRefusal::ChainExhausted { attempted: vec![] },
                            );
                        }
                    }
                    score = Some(-cost); // cheaper ranks higher
                }
            }
        }
        RoutingPolicyKind::LatencyCap => {
            let cap_ms = policy
                .params
                .get("p95_ms")
                .and_then(Json::as_int)
                .or(request.latency_target_ms.map(|t| t as i64));
            if let Some(cap) = cap_ms {
                if let Some(stats) = health.stats(&cand.model_ref) {
                    if let Some(p95) = stats.p95_ms {
                        if p95 as i64 > cap {
                            return reject(
                                CandidateRejectReason::PolicyExcluded,
                                RoutingRefusal::ChainExhausted { attempted: vec![] },
                            );
                        }
                    }
                }
            }
        }
        RoutingPolicyKind::QualityTarget => {
            let metric = policy
                .params
                .get("metric")
                .and_then(Json::as_str)
                .unwrap_or("quality");
            let prior_ref = request
                .quality_prior_ref
                .as_deref()
                .or_else(|| policy.params.get("prior_ref").and_then(Json::as_str))
                .unwrap_or("");
            let min_estimate = policy.params.get("min_estimate").and_then(Json::as_int);
            let priors: &dyn QualityPriorView = views.quality.unwrap_or(&NoQualityPrior);
            match priors.estimate_ppm(cand, prior_ref, metric) {
                None => {
                    // No covering prior — `unknown` levels refused (ADR-0012);
                    // the floor can't be demonstrated. The verdict is
                    // `policy_excluded`; the closed refusal sum names no
                    // dedicated spelling, so the row carries the filtered
                    // `chain_exhausted` marker (an all-excluded run surfaces
                    // `ChainExhausted{attempted}` below).
                    return reject(
                        CandidateRejectReason::PolicyExcluded,
                        RoutingRefusal::ChainExhausted { attempted: vec![] },
                    );
                }
                Some(est) => {
                    if let Some(min) = min_estimate {
                        if est < min {
                            return reject(
                                CandidateRejectReason::PolicyExcluded,
                                RoutingRefusal::ChainExhausted { attempted: vec![] },
                            );
                        }
                    }
                    score = Some(est);
                }
            }
        }
        RoutingPolicyKind::HealthAware => {
            // Rank by observed failure rate (lower is better); unobserved
            // candidates rank first (fresh is not failed — the view never
            // penalises what it hasn't seen).
            score = health
                .stats(&cand.model_ref)
                .and_then(|s| s.failure_rate_ppm())
                .map(|r| -r);
        }
        RoutingPolicyKind::Bandit => {
            // ADR-0312 d.1 — `score = reward_ppm + bonus`:
            //   reward_ppm = cell.success_ppm − λ_ppm·cell.cost_millis/1000
            //   bonus      = c_ppm·√(N/n)         (declared `explore_ppm`)
            // A cell below `min_n` is `unknown` and scores the declared
            // `cold_start_ppm` prior (default 0) — no pooling, no
            // interpolation (ADR-0012). The whole fold is integer ppm;
            // equal inputs select identically every time.
            let bandit: &dyn BanditView = views.bandit.unwrap_or(&NoBandit);
            let task_class = request.task_class.as_deref().unwrap_or("untagged");
            let min_n = policy
                .params
                .get("min_n")
                .and_then(Json::as_int)
                .unwrap_or(0)
                .max(0) as u64;
            let lambda_ppm = policy
                .params
                .get("lambda_ppm")
                .and_then(Json::as_int)
                .unwrap_or(0);
            let cold_start = policy
                .params
                .get("cold_start_ppm")
                .and_then(Json::as_int)
                .unwrap_or(0);
            let cell = bandit.cell(task_class, &spelling);
            let base = match cell {
                Some(c) if c.n >= min_n => c.success_ppm - lambda_ppm * c.cost_millis / 1000,
                _ => cold_start,
            };
            let bonus = policy
                .params
                .get("explore_ppm")
                .and_then(Json::as_int)
                .map(|c_ppm| {
                    let n = cell.map(|c| c.n).unwrap_or(0).max(1);
                    let total = bandit.total_n(task_class).max(n);
                    // c_ppm·√(N/n) — isqrt(N·10⁶/n)/1000 = √ratio.
                    c_ppm * isqrt_ppm(total, n) / 1000
                })
                .unwrap_or(0);
            score = Some(base + bonus);
        }
        _ => {}
    }
    Admit::Admitted(Admitted {
        leaf_ref,
        max_output,
        score,
    })
}

/// The per-kind candidate list — every kind binds its candidates from the
/// role's `RoleBinding` (the sealed model set — G-6 never admits a candidate
/// outside it) except `static` (`params.target`) and `fallback_chain`
/// (`params.targets[]`, itself a binding of the role).
fn candidate_list(
    request: &RoutingRequest,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
) -> Result<Vec<RouteCandidate>, RoutingRefusal> {
    let malformed = |reason: &str| RoutingRefusal::PolicyInvalid {
        rule_id: policy.policy_id.clone(),
        reason: reason.to_string(),
    };
    let binding = || match table.roles.get(&request.role) {
        Some(b) => Ok(std::iter::once(b.primary.clone())
            .chain(b.alternates.iter().cloned())
            .collect()),
        None => Err(RoutingRefusal::NoProfile {
            model_ref: format!("role:{}", request.role),
        }),
    };
    match policy.kind {
        RoutingPolicyKind::Static => {
            let t = policy
                .params
                .get("target")
                .ok_or_else(|| malformed("static policy without params.target"))?;
            Ok(vec![candidate_from_json(t).ok_or_else(|| {
                malformed("static params.target malformed")
            })?])
        }
        RoutingPolicyKind::FallbackChain => {
            // `fallback_chain{targets[], max_reroutes, allowed_classes[]}` —
            // the chain is the binding (§5b.2 params).
            let arr = match policy.params.get("targets") {
                Some(Json::Arr(ts)) => ts.clone(),
                Some(_) => return Err(malformed("fallback_chain params.targets must be an array")),
                // An empty/absent chain binds the role's own row (fail_fast's
                // degenerate case — the primary alone).
                None => return binding(),
            };
            let mut out = Vec::new();
            for t in &arr {
                out.push(
                    candidate_from_json(t)
                        .ok_or_else(|| malformed("fallback_chain params.targets[] malformed"))?,
                );
            }
            Ok(out)
        }
        RoutingPolicyKind::RoleTable
        | RoutingPolicyKind::CapabilityFilter
        | RoutingPolicyKind::CostCap
        | RoutingPolicyKind::LatencyCap
        | RoutingPolicyKind::QualityTarget
        | RoutingPolicyKind::HealthAware
        | RoutingPolicyKind::Bandit => binding(),
        // AC-R-2.3.2-14 — `learned` is deployable only through the
        // evolution pipeline: `params.evolution` must carry the campaign
        // evidence `{search_budget_ref, artifact_benefit_ref,
        // match_mode: "matched_total", expiry_condition[] ∋
        // "model_version_change", budget_match: "matched"}`. Anything
        // else stays a declared tier refusal — never a fall-through.
        // Once admitted, the runtime binds the role's sealed candidate
        // list (`binding()`): the learned choice is the accepted
        // candidate's frozen selection, not an in-router learner.
        RoutingPolicyKind::Learned => {
            let ev = policy.params.get("evolution").ok_or_else(|| {
                malformed("learned without params.evolution — not pipeline-bound")
            })?;
            let evo_str = |k: &str| ev.get(k).and_then(Json::as_str);
            for (k, what) in [
                ("search_budget_ref", "SearchBudgetRecord"),
                ("artifact_benefit_ref", "held-out artifact_benefit report"),
            ] {
                if evo_str(k).map(str::is_empty).unwrap_or(true) {
                    return Err(malformed(&format!(
                        "learned policy without evolution.{k} — AC-R-2.3.2-14 requires a {what}"
                    )));
                }
            }
            if evo_str("match_mode") != Some("matched_total") {
                return Err(malformed(
                    "learned policy without evolution.match_mode = matched_total",
                ));
            }
            if evo_str("budget_match") != Some("matched") {
                return Err(malformed(
                    "learned policy without evolution.budget_match = matched",
                ));
            }
            let expiry_ok = matches!(ev.get("expiry_condition"), Some(Json::Arr(c))
                if c.iter().any(|e| e.as_str() == Some("model_version_change")));
            if !expiry_ok {
                return Err(malformed(
                    "learned policy without expiry_condition ∋ model_version_change",
                ));
            }
            binding()
        }
    }
}

/// The `fallback_chain` policy params `{targets[], max_reroutes?,
/// allowed_classes[]}` — `max_reroutes`/`allowed_classes` gate
/// `on_attempt_failed`'s `Reroute` arm (absent members: unbounded reroutes
/// within the chain, every class reroutable).
fn chain_params(policy: &RoutingPolicy) -> (Option<u32>, Option<Vec<String>>) {
    let max_reroutes = policy
        .params
        .get("max_reroutes")
        .and_then(Json::as_int)
        .map(|n| n.max(0) as u32);
    let allowed = policy.params.get("allowed_classes").and_then(|a| {
        if let Json::Arr(v) = a {
            Some(
                v.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect(),
            )
        } else {
            None
        }
    });
    (max_reroutes, allowed)
}

/// `select(request, profile_env, account, health, policy, table, decision_id)
/// → RoutingDecision | RoutingRefusal` — the C0 entry: the guard composition
/// (ADR-0121 d.1/d.5) with no C1 views (`RoutingViews::none()` — a `cost_cap`
/// read without a pricing view is `NoPrice`, never zero-cost). Delegates to
/// [`select_with`] at `now_ms = 0` (no declared cooldown can precede epoch —
/// the view's `cooldown_until` is always ≥ a real `now`).
pub fn select(
    request: &RoutingRequest,
    profile_env: &dyn SelectorView,
    account: &mut dyn BudgetPort,
    health: &dyn HealthView,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
    decision_id: &str,
) -> Result<RoutingDecision, RoutingRefusal> {
    select_with(
        request,
        profile_env,
        account,
        health,
        &RoutingViews::none(),
        policy,
        table,
        decision_id,
        0,
        &BTreeSet::new(),
    )
}

/// `select_with(request, profile_env, account, health, views, policy, table,
/// decision_id, now_ms, attempted) → RoutingDecision | RoutingRefusal` — the
/// C1 guard composition (ADR-0121 d.1/d.5):
///
/// - **G-1** `resolve_profile` yields exactly one chain leaf with
///   `expiry.status ∈ {active, expiring}` — `expired` only with a recorded
///   `intent_ref`; `retired` never.
/// - **G-2** `required_capabilities ∪ params.capabilities ⊆ {declared,
///   probed}` — `unknown` ⇒ `CapabilityUnknown` unless a debt-recorded
///   `allow_unknown` rule exists.
/// - **G-3** `request.source_profile_ref` set and the candidate's leaf differs
///   ⇒ the projected `TranscriptMigration` must satisfy `max_migration_loss`
///   and no proposed effect may be in flight — `MigrationLossExceeded`.
/// - **G-4** `reserve(budget_id, max_output(profile), holder)` succeeds — the
///   reservation exists before the decision returns; `amend` never.
/// - **G-5** a policy that reads cost finds a pricing row — `NoPrice`, never
///   a zero-cost fallback.
/// - **G-6** `request.preferences` may *exclude* candidates it names
///   (`policy_excluded`) but never adds one outside the binding; a preference
///   claiming `authority > external` is `PolicyInvalid`.
/// - Health: a candidate in cooldown (`cooldown_until > now_ms`) is verdicted
///   `rejected:cooldown` and skipped.
///
/// `attempted` holds `model_ref` spellings already tried under this call — a
/// reroute never re-selects a tried target (the chain's forward-only
/// consumption; verdict `policy_excluded`). Candidates run in binding order
/// (primary, then alternates); `quality_target`/`health_aware` order
/// admissible candidates by score descending, binding order breaking ties.
/// Every candidate appears in `candidates_considered[]` with a verdict
/// (R-3). Total failure surfaces the shared refusal class when every
/// candidate failed identically, else `ChainExhausted{attempted[]}`.
#[allow(clippy::too_many_arguments)] // the arity is the contract's (§5b.2)
pub fn select_with(
    request: &RoutingRequest,
    profile_env: &dyn SelectorView,
    account: &mut dyn BudgetPort,
    health: &dyn HealthView,
    views: &RoutingViews,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
    decision_id: &str,
    now_ms: u64,
    attempted: &BTreeSet<String>,
) -> Result<RoutingDecision, RoutingRefusal> {
    // Policy validity first (T-LCD-05; a non-C1 kind is a declared tier
    // refusal — CC6, never a fall-through). `learned` (C4) is the one
    // exception: AC-R-2.3.2-14 admits it *only* pipeline-bound, which
    // `candidate_list`'s `params.evolution` evidence gate enforces — an
    // unbound `learned` still refuses there, never silently.
    policy.link_check()?;
    if !policy.kind.c1() && !matches!(policy.kind, RoutingPolicyKind::Learned) {
        return Err(RoutingRefusal::PolicyInvalid {
            rule_id: policy.policy_id.clone(),
            reason: format!("kind {} is not a C1 kind", policy.kind.as_str()),
        });
    }
    // G-6 — the lifted preference's authority ceiling (CC2: authority is
    // conferred; a record claiming more than `external` on a lifted
    // preference is refused, not trusted).
    if let Some(p) = &request.preferences {
        if !p.legal() {
            return Err(RoutingRefusal::PolicyInvalid {
                rule_id: "preferences".to_string(),
                reason: format!(
                    "lifted preference authority `{}` exceeds external",
                    p.authority.as_str()
                ),
            });
        }
    }

    let candidates = candidate_list(request, policy, table)?;
    let kind_caps: Vec<String> = match policy.kind {
        RoutingPolicyKind::CapabilityFilter => policy
            .params
            .get("capabilities")
            .and_then(|c| {
                if let Json::Arr(v) = c {
                    Some(
                        v.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => vec![],
    };

    // Admission pass — every candidate gets a verdict (R-3); the binding's
    // attempted targets are verdicted `policy_excluded` (never re-tried).
    struct Row {
        cand: RouteCandidate,
        admitted: Option<Admitted>,
        verdict: CandidateVerdict,
        refusal: Option<RoutingRefusal>,
    }
    let mut rows: Vec<Row> = Vec::new();
    for cand in &candidates {
        let spelling = model_ref_spelling(&cand.model_ref);
        if attempted.contains(&spelling) || attempted.contains(&cand.model_ref.provider_model_id) {
            rows.push(Row {
                cand: cand.clone(),
                admitted: None,
                verdict: CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded),
                refusal: Some(RoutingRefusal::ChainExhausted { attempted: vec![] }),
            });
            continue;
        }
        if let Some(prefs) = &request.preferences {
            if prefs.excludes(&spelling, &cand.model_ref.provider_model_id) {
                rows.push(Row {
                    cand: cand.clone(),
                    admitted: None,
                    verdict: CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded),
                    refusal: Some(RoutingRefusal::PolicyInvalid {
                        rule_id: "preferences".to_string(),
                        reason: "preference_excluded".to_string(),
                    }),
                });
                continue;
            }
        }
        match admit_candidate(
            cand,
            &request.required_capabilities,
            request,
            profile_env,
            health,
            now_ms,
            views,
            policy,
            &kind_caps,
        ) {
            Admit::Admitted(a) => rows.push(Row {
                cand: cand.clone(),
                admitted: Some(a),
                verdict: CandidateVerdict::Selected, // provisional — fixed below
                refusal: None,
            }),
            Admit::Rejected(reason, refusal) => rows.push(Row {
                cand: cand.clone(),
                admitted: None,
                verdict: CandidateVerdict::Rejected(reason),
                refusal: Some(refusal),
            }),
            Admit::Fatal(f) => return Err(f),
        }
    }

    // Selection order — ranked kinds order admissible candidates by score
    // (desc), binding order breaking ties; all others keep binding order
    // (`cost_cap`/`latency_cap` are *filters* — the cap binds, the binding's
    // order still selects; ranking by a filter's input would silently change
    // the declared intent).
    let ranked = matches!(
        policy.kind,
        RoutingPolicyKind::QualityTarget
            | RoutingPolicyKind::HealthAware
            | RoutingPolicyKind::Bandit
    );
    let order: Vec<usize> = {
        let mut ix: Vec<usize> = (0..rows.len()).collect();
        if ranked {
            ix.sort_by(|&a, &b| {
                let sa = rows[a].admitted.as_ref().and_then(|x| x.score);
                let sb = rows[b].admitted.as_ref().and_then(|x| x.score);
                sb.cmp(&sa)
            });
        }
        ix
    };

    // Reserve the first admissible candidate in selection order (G-4); the
    // reservation exists before the decision returns (R-2). An admissible
    // candidate whose reserve fails verdicts `insufficient_budget` and the
    // next admissible candidate is tried ("a cheaper candidate may be tried,
    // amend never" — ADR-0040 d.6).
    let mut chosen: Option<(usize, String)> = None;
    for &i in &order {
        if rows[i].admitted.is_none() {
            continue;
        }
        let max_output = rows[i].admitted.as_ref().expect("admitted").max_output;
        match account.reserve(&request.budget_id, max_output, &request.holder) {
            Ok(reservation_id) => {
                chosen = Some((i, reservation_id));
                break;
            }
            Err(dimension) => {
                rows[i].verdict =
                    CandidateVerdict::Rejected(CandidateRejectReason::InsufficientBudget);
                rows[i].refusal = Some(RoutingRefusal::InsufficientBudget {
                    dimension: dimension.clone(),
                });
            }
        }
    }

    let Some((chosen_ix, reservation_id)) = chosen else {
        // Every candidate failed — the shared class surfaces when the
        // failures agree; distinct classes are `ChainExhausted`.
        let meaningful: Vec<RoutingRefusal> = rows
            .iter()
            .filter_map(|r| r.refusal.clone())
            .filter(|r| {
                !matches!(r, RoutingRefusal::ChainExhausted { attempted } if attempted.is_empty())
            })
            .collect();
        let first = meaningful
            .first()
            .cloned()
            .unwrap_or(RoutingRefusal::ChainExhausted { attempted: vec![] });
        if !meaningful.is_empty()
            && meaningful.iter().all(|r| r.as_str() == first.as_str())
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
    };

    // Verdicts in binding order (R-3): the winner is `selected`; every other
    // admissible-but-not-taken row verdicts `lower_score` when the kind scored
    // it, `policy_excluded`… — the closed reason for "admissible, ranked
    // below" is `lower_score` (an unscored admissible loser on a non-ranked
    // kind simply was not reached: `lower_score` still names it — the
    // candidate lost on order).
    let mut verdicts: Vec<Candidate> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let spelling = model_ref_spelling(&row.cand.model_ref);
        let verdict = if i == chosen_ix {
            CandidateVerdict::Selected
        } else {
            match &row.verdict {
                CandidateVerdict::Selected => {
                    // Admissible but not selected.
                    CandidateVerdict::Rejected(CandidateRejectReason::LowerScore)
                }
                v => v.clone(),
            }
        };
        verdicts.push(Candidate {
            model_ref: spelling,
            verdict,
            score: row.admitted.as_ref().and_then(|a| a.score),
        });
    }

    let leaf_ref = rows[chosen_ix]
        .admitted
        .as_ref()
        .expect("chosen is admitted")
        .leaf_ref
        .clone();
    let cand = rows[chosen_ix].cand.clone();
    let mut selected_ref = cand.model_ref.clone();
    selected_ref.profile_ref = leaf_ref.clone();
    if let Some(e) = &request.effort {
        selected_ref.effort = Some(e.clone());
    }

    // `deviation` — the realized target differs from the role's configured
    // primary (ADR-0121 d.6); `relower_required` — the choice crosses the
    // source profile boundary (ADR-0122 d.3).
    let primary_spelling = table
        .roles
        .get(&request.role)
        .map(|b| model_ref_spelling(&b.primary.model_ref));
    let deviation = primary_spelling
        .as_deref()
        .is_some_and(|p| p != model_ref_spelling(&cand.model_ref));
    let relower_required = request
        .source_profile_ref
        .as_deref()
        .is_some_and(|src| src != leaf_ref);

    let mut inputs_read = vec![
        "profile_resolver".to_string(),
        "budget_account".to_string(),
        "health_view".to_string(),
        "role_table".to_string(),
    ];
    match policy.kind {
        RoutingPolicyKind::CostCap => inputs_read.push("pricing_table".to_string()),
        RoutingPolicyKind::QualityTarget => inputs_read.push("quality_prior".to_string()),
        RoutingPolicyKind::Bandit => inputs_read.push("bandit_view".to_string()),
        _ => {}
    }
    if request.source_profile_ref.is_some() {
        inputs_read.push("migration_projection".to_string());
    }
    if request.preferences.is_some() {
        inputs_read.push("lifted_preferences".to_string());
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
        inputs_read,
        deviation,
        relower_required,
    })
}

/// `AttemptState` — the per-logical-call retry/reroute cursor the control
/// envelope carries across attempts (F2 owns the attempt count; the router
/// tracks which targets the chain already tried and whether a compaction
/// already ran — R-8's "chain exhausted" is a function of this state).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttemptState {
    /// `model_ref` spellings already tried under this call (never re-tried).
    pub attempted: BTreeSet<String>,
    /// Reroutes already taken under this call (`max_reroutes` bound).
    pub reroutes_used: u32,
    /// A `compact_then_retry` already ran for this call.
    pub compact_tried: bool,
}

/// `GiveUpReason` — why `on_attempt_failed` closed the call (R-8's terminal;
/// the envelope maps it onto `model.call.failed{error.class, attempts}`).
#[derive(Debug, Clone, PartialEq)]
pub enum GiveUpReason {
    /// The `error_actions` row says `give_up` (or no row — the closed
    /// default).
    PolicyGiveUp,
    /// The class is not in `allowed_classes[]` — not reroutable under this
    /// policy.
    ClassNotReroutable,
    /// Every chain target was tried or `max_reroutes` was spent.
    ChainExhausted {
        /// The attempted `model_ref` spellings.
        attempted: Vec<String>,
    },
    /// The reroute `select` refused (e.g. `InsufficientBudget`,
    /// `NoProfile` on the remaining candidates).
    SelectRefused(RoutingRefusal),
}

impl GiveUpReason {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            GiveUpReason::PolicyGiveUp => "policy_give_up",
            GiveUpReason::ClassNotReroutable => "class_not_reroutable",
            GiveUpReason::ChainExhausted { .. } => "chain_exhausted",
            GiveUpReason::SelectRefused(_) => "select_refused",
        }
    }
}

/// `on_attempt_failed → Continue | Reroute | GiveUp` (§5b.2; ADR-0122 d.1).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // `Reroute` carries the fresh decision — the spec payload, not a hot-path alloc.
pub enum AttemptDisposition {
    /// Same target again — `not_before_ms` is the policy/`retry_after` delay;
    /// `compact_first` marks the `compact_then_retry` arm (compaction under
    /// the old profile runs before the retry — ADR-0122 d.3 ordering).
    Continue {
        /// `not_before` — the delay before the retry (ms; from
        /// `error.retry_after_ms`/`Retry-After`, 0 otherwise).
        not_before_ms: u64,
        /// A compaction runs under the old profile before the retry.
        compact_first: bool,
    },
    /// Reroute — the fresh `RoutingDecision` (its reservation is held; the
    /// old one was released first — R-7).
    Reroute(RoutingDecision),
    /// `GiveUp{reason}` — the envelope closes `model.call.failed`.
    GiveUp(GiveUpReason),
}

/// `on_attempt_failed(request, prior, error, attempts_on_target,
/// retry_after_ms, state, account, health, views, policy, table,
/// decision_id, now_ms)` — the ADR-0122 d.1–d.3 consult (R-5…R-8):
///
/// - R-6 — the action is read from the policy's closed `error_actions` table
///   (`error_action`), never from message text;
/// - `retry_same{max, then}` continues same-target while
///   `attempts_on_target < max`, then walks `then`;
/// - `compact_then_retry{then}` compacts once under the old profile
///   (`state.compact_tried`), then walks `then`;
/// - `reroute` (or a chain landing on it) releases `prior`'s reservation
///   **before** the new `select_with` holds its own (R-7), skips
///   `state.attempted` targets, obeys `fallback_chain`'s `max_reroutes` /
///   `allowed_classes`, and marks the fresh decision `deviation` /
///   `relower_required` (a cross-profile reroute needs `relower` first);
/// - `give_up`, a non-reroutable class, an exhausted chain, or a refused
///   re-select is `GiveUp` (R-8 — the envelope closes `model.call.failed`).
#[allow(clippy::too_many_arguments)] // the arity is the contract's (§5b.2)
pub fn on_attempt_failed(
    request: &RoutingRequest,
    prior: &RoutingDecision,
    error: &ModelErrorClass,
    attempts_on_target: u32,
    retry_after_ms: Option<u64>,
    state: &mut AttemptState,
    profile_env: &dyn SelectorView,
    account: &mut dyn BudgetPort,
    health: &dyn HealthView,
    views: &RoutingViews,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
    decision_id: &str,
    now_ms: u64,
) -> AttemptDisposition {
    state.attempted.insert(model_ref_spelling(&prior.selected));
    state
        .attempted
        .insert(prior.selected.provider_model_id.clone());
    let mut action = error_action(policy, error);
    // Walk the action chain — `retry_same`/`compact_then_retry` resolve to
    // `Continue` while their bound holds, else `then` (default `GiveUp`).
    loop {
        match action {
            ErrorAction::RetrySame { max, ref then } => {
                if attempts_on_target < max {
                    return AttemptDisposition::Continue {
                        not_before_ms: retry_after_ms.unwrap_or(0),
                        compact_first: false,
                    };
                }
                action = then.as_deref().cloned().unwrap_or(ErrorAction::GiveUp);
            }
            ErrorAction::CompactThenRetry { ref then } => {
                if !state.compact_tried {
                    state.compact_tried = true;
                    return AttemptDisposition::Continue {
                        not_before_ms: retry_after_ms.unwrap_or(0),
                        compact_first: true,
                    };
                }
                action = then.as_deref().cloned().unwrap_or(ErrorAction::GiveUp);
            }
            ErrorAction::GiveUp => return AttemptDisposition::GiveUp(GiveUpReason::PolicyGiveUp),
            ErrorAction::Reroute => break,
        }
    }
    // The `reroute` arm — `fallback_chain`'s bounds gate it; other kinds
    // reroute within the role's binding.
    let (max_reroutes, allowed_classes) = chain_params(policy);
    if let Some(classes) = &allowed_classes {
        if !classes.iter().any(|c| c == error.as_str()) {
            return AttemptDisposition::GiveUp(GiveUpReason::ClassNotReroutable);
        }
    }
    if let Some(max) = max_reroutes {
        if state.reroutes_used >= max {
            return AttemptDisposition::GiveUp(GiveUpReason::ChainExhausted {
                attempted: state.attempted.iter().cloned().collect(),
            });
        }
    }
    // R-7 — the old reservation is released before the new one is held. A
    // failed release is a typed `GiveUp{select_refused}`-class terminal —
    // the router never holds two reservations for one call.
    if let Some(res) = &prior.reservation_id {
        if let Err(e) = account.release(res) {
            return AttemptDisposition::GiveUp(GiveUpReason::SelectRefused(
                RoutingRefusal::PolicyInvalid {
                    rule_id: "reservation".to_string(),
                    reason: format!("release failed: {e}"),
                },
            ));
        }
    }
    // The reroute select — same request under the failed target's leaf
    // profile (G-3 projects the migration; `relower_required` marks a
    // cross-profile move), minus the tried targets.
    let mut req = request.clone();
    req.source_profile_ref = Some(prior.selected.profile_ref.clone());
    match select_with(
        &req,
        profile_env,
        account,
        health,
        views,
        policy,
        table,
        decision_id,
        now_ms,
        &state.attempted,
    ) {
        Ok(d) => {
            state.reroutes_used += 1;
            AttemptDisposition::Reroute(d)
        }
        Err(r) => AttemptDisposition::GiveUp(GiveUpReason::SelectRefused(r)),
    }
}

/// Decode a `RouteCandidate` from `params.target` / `params.targets[]`.
/// The member spells `candidate{profile_ref, provider_model_id, …}` —
/// the sealed-document grammar (a `model_ref` key inside HIR `params`
/// trips T-LCD-01's model-identity statics). `model_ref` stays admitted
/// on decode for authored records that predate the document spelling —
/// emission is always `candidate`.
fn candidate_from_json(j: &Json) -> Option<RouteCandidate> {
    let mref = j.get("candidate").or_else(|| j.get("model_ref"))?;
    Some(RouteCandidate {
        model_ref: model_ref_from_json(mref)?,
        coordinate: coordinate_from_json(j.get("coordinate")?)?,
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

/// `RerouteOrderError` — the AC-R-2.3.2-4 ordering violation (typed, never a
/// warning).
#[derive(Debug, Clone, PartialEq)]
pub enum RerouteOrderError {
    /// A `model.rerouted` has no preceding `model.call.attempt.failed` for
    /// the call.
    NoFailedAttempt {
        /// The call.
        model_call_id: String,
    },
    /// A cross-profile reroute (`relowered = true`) has no
    /// `model.surface.relowered` between the failure and the reroute.
    MissingRelower {
        /// The call.
        model_call_id: String,
    },
    /// A same-profile reroute (`relowered = false`) is preceded by a
    /// `model.surface.relowered` — a relower without a profile boundary
    /// crossing is a lie.
    UnexpectedRelower {
        /// The call.
        model_call_id: String,
    },
    /// No `model.call.attempt.started` follows the `model.rerouted` row.
    MissingRestart {
        /// The call.
        model_call_id: String,
    },
    /// The `model.surface.relowered` row lacks the derivation members
    /// (`old_profile_ref`/`new_profile_ref`/`reason`).
    MissingDerivation {
        /// The call.
        model_call_id: String,
    },
}

impl std::fmt::Display for RerouteOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RerouteOrderError::NoFailedAttempt { model_call_id } => {
                write!(f, "NoFailedAttempt: {model_call_id}")
            }
            RerouteOrderError::MissingRelower { model_call_id } => {
                write!(f, "MissingRelower: {model_call_id}")
            }
            RerouteOrderError::UnexpectedRelower { model_call_id } => {
                write!(f, "UnexpectedRelower: {model_call_id}")
            }
            RerouteOrderError::MissingRestart { model_call_id } => {
                write!(f, "MissingRestart: {model_call_id}")
            }
            RerouteOrderError::MissingDerivation { model_call_id } => {
                write!(f, "MissingDerivation: {model_call_id}")
            }
        }
    }
}
impl std::error::Error for RerouteOrderError {}

/// `check_reroute_order(events, model_call_id)` — the AC-R-2.3.2-4 ordering
/// oracle over emitted `(seq, class, payload)` rows: for every
/// `model.rerouted` of the call the sequence must read
/// `model.call.attempt.failed` → (optional `context.compaction.*` under the
/// old profile) → `model.surface.relowered{old_profile_ref, new_profile_ref,
/// reason}` → `model.rerouted{relowered = true}` → `model.call.attempt.started`
/// in `seq` order; a same-profile reroute (`relowered = false`) carries no
/// relower event. Returns the first violation.
pub fn check_reroute_order(
    events: &[(u64, String, Json)],
    model_call_id: &str,
) -> Result<(), RerouteOrderError> {
    let rows: Vec<(u64, &str, &Json)> = events
        .iter()
        .filter(|(_, _, p)| p.get("model_call_id").and_then(Json::as_str) == Some(model_call_id))
        .map(|(seq, class, p)| (*seq, class.as_str(), p))
        .collect();
    for (seq, class, p) in &rows {
        if *class != "model.rerouted" {
            continue;
        }
        let relowered = matches!(p.get("relowered"), Some(Json::Bool(true)));
        let prior: Vec<&(u64, &str, &Json)> = rows.iter().take_while(|(s, _, _)| s < seq).collect();
        let failed_at = prior
            .iter()
            .rposition(|(_, c, _)| *c == "model.call.attempt.failed")
            .ok_or_else(|| RerouteOrderError::NoFailedAttempt {
                model_call_id: model_call_id.to_string(),
            })?;
        let relower = prior[failed_at + 1..]
            .iter()
            .find(|(_, c, _)| *c == "model.surface.relowered");
        match (relowered, relower) {
            (true, None) => {
                return Err(RerouteOrderError::MissingRelower {
                    model_call_id: model_call_id.to_string(),
                });
            }
            (false, Some(_)) => {
                return Err(RerouteOrderError::UnexpectedRelower {
                    model_call_id: model_call_id.to_string(),
                });
            }
            (true, Some((_, _, rp))) => {
                for member in ["old_profile_ref", "new_profile_ref", "reason"] {
                    if rp.get(member).is_none() {
                        return Err(RerouteOrderError::MissingDerivation {
                            model_call_id: model_call_id.to_string(),
                        });
                    }
                }
            }
            (false, None) => {}
        }
        if !rows
            .iter()
            .any(|(s, c, _)| s > seq && *c == "model.call.attempt.started")
        {
            return Err(RerouteOrderError::MissingRestart {
                model_call_id: model_call_id.to_string(),
            });
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// `explain` (ADR-0121 d.1) + the lift/lower protocol round-trip (d.4;
// AC-R-2.3.2-12)
// ─────────────────────────────────────────────────────────────────────────────

/// `RoutingExplanation{decision?, reroutes[]}` — the trace viewer/Lab
/// projection over `model.route.decided` + `model.rerouted` (a fold over
/// ledger rows; no new state — §5b.2 interface table).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingExplanation {
    /// The `model.route.decided` payload for `decision_ref` (`None` when the
    /// fold didn't see it).
    pub decision: Option<Json>,
    /// The `model.rerouted` payloads linked to the decision (`decision_ref`
    /// on each row), in seq order.
    pub reroutes: Vec<Json>,
}

/// `explain(rows, decision_ref) → RoutingExplanation` — a pure fold over
/// `(seq, class, payload)` rows (ADR-0121 d.1). A `model.rerouted` links to
/// the decision it departed from via its `decision_ref` member.
pub fn explain(rows: &[(u64, &str, &Json)], decision_ref: &str) -> RoutingExplanation {
    let mut decision = None;
    let mut reroutes = Vec::new();
    let mut sorted: Vec<&(u64, &str, &Json)> = rows.iter().collect();
    sorted.sort_by_key(|(s, _, _)| *s);
    for (_, class, p) in sorted {
        match *class {
            "model.route.decided"
                if p.get("decision_id").and_then(Json::as_str) == Some(decision_ref) =>
            {
                decision = Some((*p).clone());
            }
            "model.rerouted"
                if p.get("decision_ref").and_then(Json::as_str) == Some(decision_ref) =>
            {
                reroutes.push((*p).clone());
            }
            _ => {}
        }
    }
    RoutingExplanation { decision, reroutes }
}

/// `LoweringLossReport{no_slot[]}` — the members a protocol preference
/// document cannot carry (T-LCD-11: `required_capabilities`, `budget_view`,
/// `role` are `no_slot` on `mcp_model_preferences`).
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// `no_slot[]` — request members with no slot in the protocol document.
    pub no_slot: Vec<String>,
}

/// `lower(request, mcp_model_preferences) → document + LoweringLossReport`
/// (ADR-0121 d.1; AC-R-2.3.2-12). The MCP shape carries `hints[]`,
/// `costPriority`, `speedPriority`, `intelligencePriority` (0.0–1.0 doubles
/// lower as ppm ints — canonical JSON carries integers only); the request's
/// authority-bearing members are `no_slot` loss — declared, never silently
/// dropped.
pub fn lower(request: &RoutingRequest) -> (Json, LoweringLossReport) {
    let mut m = BTreeMap::new();
    if let Some(p) = &request.preferences {
        if !p.hints.is_empty() {
            m.insert(
                "hints".to_string(),
                Json::Arr(
                    p.hints
                        .iter()
                        .map(|h| Json::obj([("name", Json::str(h.clone()))]))
                        .collect(),
                ),
            );
        }
        if let Some(v) = p.cost_priority {
            m.insert("costPriority".to_string(), Json::Int(v));
        }
        if let Some(v) = p.speed_priority {
            m.insert("speedPriority".to_string(), Json::Int(v));
        }
        if let Some(v) = p.intelligence_priority {
            m.insert("intelligencePriority".to_string(), Json::Int(v));
        }
        if !p.exclude.is_empty() {
            m.insert(
                "exclude".to_string(),
                Json::Arr(p.exclude.iter().map(Json::str).collect()),
            );
        }
    }
    (
        Json::Obj(m),
        LoweringLossReport {
            no_slot: vec![
                "required_capabilities".to_string(),
                "budget_view".to_string(),
                "role".to_string(),
            ],
        },
    )
}

/// `lift(document, origin_authority) → RoutingRequest.preferences +
/// LoweringLossReport` (ADR-0121 d.4 — lifted preferences are consumable only
/// by fields typed as *preferences*; `authority` is the *minted* class of the
/// origin — `unverified` for imported content, `external` at most — never
/// trusted from the document itself, CC2). A document claiming an authority
/// member above `external` is `PolicyInvalid`.
pub fn lift(
    document: &Json,
    origin_authority: hh_provenance::authority::AuthorityClass,
) -> Result<(LiftedPreferences, LoweringLossReport), RoutingRefusal> {
    if origin_authority > hh_provenance::authority::AuthorityClass::External {
        return Err(RoutingRefusal::PolicyInvalid {
            rule_id: "lift".to_string(),
            reason: format!(
                "lifted preference authority `{}` exceeds external",
                origin_authority.as_str()
            ),
        });
    }
    let ppm = |k: &str| document.get(k).and_then(Json::as_int);
    let hints = document
        .get("hints")
        .and_then(|h| {
            if let Json::Arr(v) = h {
                Some(
                    v.iter()
                        .filter_map(|e| {
                            e.get("name")
                                .and_then(Json::as_str)
                                .or_else(|| e.as_str())
                                .map(str::to_string)
                        })
                        .collect(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();
    let exclude = document
        .get("exclude")
        .and_then(|e| {
            if let Json::Arr(v) = e {
                Some(
                    v.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();
    Ok((
        LiftedPreferences {
            authority: origin_authority,
            cost_priority: ppm("costPriority"),
            speed_priority: ppm("speedPriority"),
            intelligence_priority: ppm("intelligencePriority"),
            exclude,
            hints,
        },
        LoweringLossReport {
            no_slot: vec![
                "required_capabilities".to_string(),
                "budget_view".to_string(),
                "role".to_string(),
            ],
        },
    ))
}
