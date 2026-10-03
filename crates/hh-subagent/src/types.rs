//! The §5e.3 §"Data model" records — the `SubagentSpec` (MUST-data, ADR-0185
//! D3), `SubagentResult`/`ChildOutcome`/`ReturnContract` (ADR-0187 D1/D2 as
//! amended, ADR-0193 D2), `MessagingPolicy` (ADR-0191 M-2), the closed
//! `DelegationReason` sum (ADR-0186 D4), and the coordination halves the
//! spawn carries (`ownership_grants[]`, `reserved_keys[]`,
//! `consistency_declarations[]`, `merge_policy_ref` — ADR-0193 D4).
//!
//! Everything here is canonical `Json` at the boundary (CC4/CC7): the spec is
//! the decision's `spec` payload; `spec_hash()` is the idempotency key's spec
//! half. No `Text` leaf is ever *read* — `Goal.statement` travels as its
//! content-addressed leaf (SP-3: a child never sees parent transcript bytes).

use hh_budget::spec::BudgetSpec;
use hh_budget::BudgetMode;
use hh_env::handle::{DeriveMode, OnParentEnd};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
use hh_hir::refs::{Ref, RefVersion};
use hh_ontology::control::Owner;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

fn str_of(j: &Json) -> Option<&str> {
    j.as_str()
}

fn bool_of(j: &Json) -> Option<bool> {
    match j {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

fn arr_of(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

fn is_obj(j: &Json) -> bool {
    matches!(j, Json::Obj(_))
}

// ─────────────────────────────────────────────────────────────────────────────
// DelegationReason (ADR-0186 D4 — mandatory on every `control.decision{delegate}`)
// ─────────────────────────────────────────────────────────────────────────────

/// `delegation_reason` — the closed sum (ADR-0186 D4). Mandatory on every
/// `control.decision{kind: delegate}` with `owner ∈ {code, model}`; a `model`-
/// owned reason is a `model_claim` at `authority = delegate` (the owner stamp
/// decides the label — the field itself is data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DelegationReason {
    /// Independent subtasks run concurrently (T1's declared reason).
    Parallelism,
    /// A bounded context for a bounded subtask (H4's measured factor).
    CleanContext,
    /// A stronger isolation boundary than the parent's (T4's declared reason).
    Isolation,
    /// An independent verification pass (judge role).
    IndependentCheck,
    /// A specialized profile/role the parent does not have.
    Specialization,
}

impl DelegationReason {
    /// The closed set (the envelope refuses any other spelling).
    pub const ALL: [DelegationReason; 5] = [
        DelegationReason::Parallelism,
        DelegationReason::CleanContext,
        DelegationReason::Isolation,
        DelegationReason::IndependentCheck,
        DelegationReason::Specialization,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DelegationReason::Parallelism => "parallelism",
            DelegationReason::CleanContext => "clean_context",
            DelegationReason::Isolation => "isolation",
            DelegationReason::IndependentCheck => "independent_check",
            DelegationReason::Specialization => "specialization",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<DelegationReason> {
        DelegationReason::ALL
            .iter()
            .copied()
            .find(|r| r.as_str() == s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The child process + isolation + supplies (SubagentSpec members)
// ─────────────────────────────────────────────────────────────────────────────

/// `process` — `native{harness_def, profile_binding?, control_strategy?, slots?}`
/// or `hosted(Ref<OpaqueProcess>)` (SP-7: the `harness_def` resolves to a sealed
/// definition or registered variant — a `version_selector` is refused
/// `DefinitionUnresolvable`; `harness_def` may equal the parent's semantic id —
/// T3). `hosted` is the C2/T7 arm (SP-8; the hosted-peer slice is a later
/// ticket — declared here, refused `ModeUnsupported` at `spawn`).
#[derive(Debug, Clone, PartialEq)]
pub enum ChildProcess {
    /// A native child definition.
    Native {
        /// The sealed definition ref (pinned — T3 allows semantic equality
        /// with the parent's).
        harness_def: Ref,
        /// The profile binding (`None` = the parent's subagent role —
        /// ADR-0121/0124 default).
        profile_binding: Option<String>,
        /// The child's control strategy id (`None` = the definition's).
        control_strategy: Option<String>,
        /// Definition parameter overrides (`experiment`-class layer — OQ-189).
        slots: Option<Json>,
    },
    /// A hosted peer (`Ref<OpaqueProcess>`) — the T7/C2 arm.
    Hosted(Ref),
}

impl ChildProcess {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            ChildProcess::Native {
                harness_def,
                profile_binding,
                control_strategy,
                slots,
            } => {
                let mut m = vec![
                    ("kind", Json::str("native")),
                    ("harness_def", harness_def.to_json()),
                ];
                if let Some(p) = profile_binding {
                    m.push(("profile_binding", Json::str(p.clone())));
                }
                if let Some(s) = control_strategy {
                    m.push(("control_strategy", Json::str(s.clone())));
                }
                if let Some(s) = slots {
                    m.push(("slots", s.clone()));
                }
                Json::obj(m)
            }
            ChildProcess::Hosted(r) => {
                Json::obj([("kind", Json::str("hosted")), ("process", r.to_json())])
            }
        }
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<ChildProcess> {
        match str_of(j.get("kind")?)? {
            "native" => Some(ChildProcess::Native {
                harness_def: ref_from_json(j.get("harness_def")?)?,
                profile_binding: j
                    .get("profile_binding")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                control_strategy: j
                    .get("control_strategy")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                slots: j.get("slots").cloned(),
            }),
            "hosted" => Some(ChildProcess::Hosted(ref_from_json(j.get("process")?)?)),
            _ => None,
        }
    }
}

/// `role ∈ {subagent, judge}` — the `judge` role binds `independent_check`
/// and `grants ∩ read-only` (T4 — ADR-0116's own stage; refused
/// `ModeUnsupported` at `validate` at this slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildRole {
    /// A plain subagent.
    Subagent,
    /// An independent verifier (T4 — ADR-0116's own stage).
    Judge,
}

impl ChildRole {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ChildRole::Subagent => "subagent",
            ChildRole::Judge => "judge",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ChildRole> {
        Some(match s {
            "subagent" => ChildRole::Subagent,
            "judge" => ChildRole::Judge,
            _ => return None,
        })
    }
}

/// `isolation.environment` — `none | derive{mode, spec}` (`share` is the
/// Stage-5/§5e.5 arm — declared, refused `ModeUnsupported` at `spawn`;
/// `scoped_subtree` derives the scoped half).
#[derive(Debug, Clone, PartialEq)]
pub enum EnvIsolation {
    /// The child runs without a derived environment handle.
    None,
    /// `derive(parent_env, mode, spec)` at spawn step 5 (ADR-0138).
    Derive { mode: DeriveMode, spec: Json },
}

impl EnvIsolation {
    /// The canonical JSON (`isolation.environment` member).
    pub fn to_json(&self) -> Json {
        match self {
            EnvIsolation::None => Json::obj([("kind", Json::str("none"))]),
            EnvIsolation::Derive { mode, spec } => Json::obj([
                ("kind", Json::str("derive")),
                ("mode", Json::str(derive_mode_str(*mode))),
                ("spec", spec.clone()),
            ]),
        }
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<EnvIsolation> {
        Some(match str_of(j.get("kind")?)? {
            "none" => EnvIsolation::None,
            "derive" => EnvIsolation::Derive {
                mode: parse_derive_mode(str_of(j.get("mode")?)?)?,
                spec: j.get("spec").cloned().unwrap_or(Json::Null),
            },
            _ => return None,
        })
    }

    /// The isolation-mode spelling the `spawned` payload carries.
    pub fn mode_str(&self) -> &'static str {
        match self {
            EnvIsolation::None => "none",
            EnvIsolation::Derive { mode, .. } => derive_mode_str(*mode),
        }
    }
}

/// The `DeriveMode` spelling (one spelling table — CC1).
pub fn derive_mode_str(m: DeriveMode) -> &'static str {
    match m {
        DeriveMode::FreshFromImage => "fresh_from_image",
        DeriveMode::ForkSnapshot => "fork_snapshot",
        DeriveMode::Share => "share",
        DeriveMode::ScopedSubtree => "scoped_subtree",
    }
}

/// Parse a `DeriveMode` spelling.
pub fn parse_derive_mode(s: &str) -> Option<DeriveMode> {
    Some(match s {
        "fresh_from_image" => DeriveMode::FreshFromImage,
        "fork_snapshot" => DeriveMode::ForkSnapshot,
        "share" => DeriveMode::Share,
        "scoped_subtree" => DeriveMode::ScopedSubtree,
        _ => return None,
    })
}

/// `OnParentEnd` spellings — the spec's `cancel | detach_to_child` (hh-env's
/// `Teardown`/`DetachToChild` machine states; one spelling table).
pub fn on_parent_end_str(m: OnParentEnd) -> &'static str {
    match m {
        OnParentEnd::Teardown => "cancel",
        OnParentEnd::DetachToChild => "detach_to_child",
    }
}

/// Parse an `OnParentEnd` spelling (`teardown` accepted as the machine name).
pub fn parse_on_parent_end(s: &str) -> Option<OnParentEnd> {
    Some(match s {
        "cancel" | "teardown" => OnParentEnd::Teardown,
        "detach_to_child" => OnParentEnd::DetachToChild,
        _ => return None,
    })
}

/// `supplies` — the only parent→child material (SP-3: `ContextItem`s with
/// their own `delivery_id`/labels, declared procedures, `tools ⊆` the parent's
/// sealed tool table, memories). The tools-⊆ check runs against
/// `SpawnCtx.parent_tool_table`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Supplies {
    /// Supplied context items (each carries its own provenance/delivery id).
    pub context_items: Vec<Json>,
    /// Supplied procedure refs.
    pub procedures: Vec<Ref>,
    /// Supplied tool capability refs (⊆ the parent's sealed tool table).
    pub tools: Vec<Ref>,
    /// Supplied memory refs/records.
    pub memories: Vec<Json>,
}

impl Supplies {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("context_items", Json::Arr(self.context_items.clone())),
            (
                "procedures",
                Json::Arr(self.procedures.iter().map(Ref::to_json).collect()),
            ),
            (
                "tools",
                Json::Arr(self.tools.iter().map(Ref::to_json).collect()),
            ),
            ("memories", Json::Arr(self.memories.clone())),
        ])
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<Supplies> {
        Some(Supplies {
            context_items: j
                .get("context_items")
                .and_then(arr_of)
                .cloned()
                .unwrap_or_default(),
            procedures: j
                .get("procedures")
                .and_then(arr_of)
                .map(|a| a.iter().filter_map(ref_from_json).collect())
                .unwrap_or_default(),
            tools: j
                .get("tools")
                .and_then(arr_of)
                .map(|a| a.iter().filter_map(ref_from_json).collect())
                .unwrap_or_default(),
            memories: j
                .get("memories")
                .and_then(arr_of)
                .cloned()
                .unwrap_or_default(),
        })
    }

    /// Whether every member is empty (the `supplies_digest` is still recorded).
    pub fn is_empty(&self) -> bool {
        self.context_items.is_empty()
            && self.procedures.is_empty()
            && self.tools.is_empty()
            && self.memories.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ReturnContract (M-2; ADR-0187; OQ-413 — `summary.max_tokens` may be absent)
// ─────────────────────────────────────────────────────────────────────────────

/// One `return_contract.artifacts[]` member — `{kind, schema?, required}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactDecl {
    /// The artifact kind.
    pub kind: String,
    /// A schema ref/idp pin the artifact's canonical form must satisfy.
    pub schema: Option<String>,
    /// Whether absence violates the contract (M-2).
    pub required: bool,
}

/// `return_contract.summary` — `{schema?, max_tokens?}` (OQ-413's ratified
/// default admits `max_tokens = none` — the child's envelope enforces the
/// declared bound when present; the contract check never truncates).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SummaryDecl {
    /// A schema ref the summary artifact must satisfy.
    pub schema: Option<String>,
    /// The token cap (`None` under OQ-413 — the default is profile-declared).
    pub max_tokens: Option<u64>,
}

/// `ReturnContract{artifacts[], summary | none, claims}` — enforced by the
/// child's envelope at its `stop`; violations are typed records, never
/// silent truncation (ADR-0092/M-2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReturnContract {
    /// Required/optional artifact declarations.
    pub artifacts: Vec<ArtifactDecl>,
    /// The summary declaration (`None` = the `none` arm).
    pub summary: Option<SummaryDecl>,
    /// Whether completion claims are expected on the result.
    pub claims: bool,
}

impl ReturnContract {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "artifacts",
                Json::Arr(
                    self.artifacts
                        .iter()
                        .map(|a| {
                            let mut m = vec![
                                ("kind", Json::str(a.kind.clone())),
                                ("required", Json::Bool(a.required)),
                            ];
                            if let Some(s) = &a.schema {
                                m.push(("schema", Json::str(s.clone())));
                            }
                            Json::obj(m)
                        })
                        .collect(),
                ),
            ),
            (
                "summary",
                match &self.summary {
                    None => Json::str("none"),
                    Some(s) => {
                        let mut m = vec![];
                        if let Some(sc) = &s.schema {
                            m.push(("schema", Json::str(sc.clone())));
                        }
                        if let Some(t) = s.max_tokens {
                            m.push(("max_tokens", Json::Int(t.min(i64::MAX as u64) as i64)));
                        }
                        Json::obj(m)
                    }
                },
            ),
            ("claims", Json::Bool(self.claims)),
        ])
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<ReturnContract> {
        let mut rc = ReturnContract::default();
        if let Some(a) = j.get("artifacts").and_then(arr_of) {
            for m in a {
                rc.artifacts.push(ArtifactDecl {
                    kind: str_of(m.get("kind")?)?.to_string(),
                    schema: m.get("schema").and_then(Json::as_str).map(str::to_string),
                    required: m.get("required").and_then(bool_of).unwrap_or(false),
                });
            }
        }
        rc.summary = match j.get("summary") {
            Some(Json::Str(s)) if s == "none" => None,
            Some(o) if is_obj(o) => Some(SummaryDecl {
                schema: o.get("schema").and_then(Json::as_str).map(str::to_string),
                max_tokens: o
                    .get("max_tokens")
                    .and_then(Json::as_int)
                    .map(|v| v.max(0) as u64),
            }),
            Some(_) => return None,
            None => None,
        };
        if let Some(c) = j.get("claims").and_then(bool_of) {
            rc.claims = c;
        }
        Some(rc)
    }

    /// The payload member the `spawned` row cites (`return_contract_ref` is the
    /// contract's content address under the `hh.subagent.contract` domain).
    pub fn contract_ref(&self) -> String {
        hh_identity::idp::idp_id(
            "hh.subagent.contract",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The SubagentSpec itself (ADR-0185 D3)
// ─────────────────────────────────────────────────────────────────────────────

/// `wait.mode ∈ {await, background}` — `await` ⇒ `Cue.wait{delegation_
/// completed}` (the parent may `suspend{awaiting_child}`); `background` ⇒
/// return immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitMode {
    /// The parent waits for the `delegation_completed` cue.
    Await,
    /// The child runs detached from the parent's decision loop (the terminal
    /// still reaches the parent's `child_terminal` subscription — C-1).
    Background,
}

impl WaitMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WaitMode::Await => "await",
            WaitMode::Background => "background",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<WaitMode> {
        Some(match s {
            "await" => WaitMode::Await,
            "background" => WaitMode::Background,
            _ => return None,
        })
    }
}

/// `SubagentSpec` — the typed spawn input (the `control.decision{kind:
/// delegate}` row's `spec` payload — MUST-data; ADR-0185 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentSpec {
    /// The child process.
    pub process: ChildProcess,
    /// The delegated goal (`origin = delegated`, `parent` = the parent's goal).
    pub goal: GoalRecord,
    /// `subagent` | `judge`.
    pub role: ChildRole,
    /// The grants the child requests (attenuated to ⊆ `delegable(parent)`).
    pub requested_grants: Vec<Grant>,
    /// The child's ceiling request (`None` = the parent's effective ceiling at
    /// spawn — `min(requested, eff(parent))` still applies; SP-1).
    pub ceiling: Option<AuthorityClass>,
    /// The child's budget node — `budget.spec` (`spec.mode` is authoritative;
    /// `mode` mirrors for readers — they must agree).
    pub budget_spec: BudgetSpec,
    /// `budget.mode ∈ {slice, pool}` (must equal `budget_spec.mode`).
    pub budget_mode: BudgetMode,
    /// `isolation.environment`.
    pub environment: EnvIsolation,
    /// Supplied material (SP-3's only parent→child channel).
    pub supplies: Supplies,
    /// The return contract.
    pub return_contract: ReturnContract,
    /// `wait.mode`.
    pub wait_mode: WaitMode,
    /// The `subagent` scope deadline bound (ms — C-2's drain bound and C-4's
    /// wait bound derive from the same declared `TimeoutPolicy[subagent]`).
    pub wait_timeout_ms: u64,
    /// `cancel | detach_to_child` (T6 requires `detach_to_child`).
    pub on_parent_end: OnParentEnd,
    /// The mandatory delegation reason (ADR-0186 D4).
    pub delegation_reason: DelegationReason,
    /// The `Ref<TopologyPreset>` this spawn belongs under (C3 carries it).
    pub topology_ref: Option<String>,
    /// The T2 stage index (`None` outside pipelines).
    pub stage_index: Option<u64>,
    /// `ownership_grants[]` — ⊆ the parent's valid ownerships (ADR-0191 O-2;
    /// step 4b).
    pub ownership_grants: Vec<OwnedObject>,
    /// `reserved_keys[]` — the resource keys `share` isolation would lock
    /// (recorded on the `spawned` row; the lock itself is the Stage-5 arm).
    pub reserved_keys: Vec<String>,
    /// `consistency_declarations[]` — declared `ConsistencyLevel`s per
    /// coordination object (ADR-0193 D5 — carried as data at this slice).
    pub consistency_declarations: Vec<Json>,
    /// The `MergePolicy` ref the child's workspace changes merge under
    /// (`merge_policy_ref` — ADR-0193 D4).
    pub merge_policy_ref: Option<String>,
    /// The `MessagingPolicy` the child's sealed definition declares
    /// (ADR-0191 M-2 — carried on the `spawned` row; the C3 preset binds
    /// it, the C1 kernel only records it).
    pub messaging_policy: Option<MessagingPolicy>,
}

impl SubagentSpec {
    /// The canonical JSON (the decision row's `spec` member).
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("process", self.process.to_json()),
            ("goal", goal_to_json(&self.goal)),
            ("role", Json::str(self.role.as_str())),
            (
                "requested_grants",
                Json::Arr(self.requested_grants.iter().map(grant_to_json).collect()),
            ),
            (
                "budget",
                Json::obj([
                    ("mode", Json::str(budget_mode_str(self.budget_mode))),
                    ("spec", self.budget_spec.to_json()),
                ]),
            ),
            (
                "isolation",
                Json::obj([
                    ("context", Json::str("fresh")),
                    ("environment", self.environment.to_json()),
                    ("memory_namespace", Json::str("child")),
                ]),
            ),
            ("supplies", self.supplies.to_json()),
            ("return_contract", self.return_contract.to_json()),
            (
                "wait",
                Json::obj([
                    ("mode", Json::str(self.wait_mode.as_str())),
                    (
                        "timeout_ms",
                        Json::Int(self.wait_timeout_ms.min(i64::MAX as u64) as i64),
                    ),
                ]),
            ),
            (
                "on_parent_end",
                Json::str(on_parent_end_str(self.on_parent_end)),
            ),
            (
                "delegation_reason",
                Json::str(self.delegation_reason.as_str()),
            ),
            (
                "ownership_grants",
                Json::Arr(
                    self.ownership_grants
                        .iter()
                        .map(OwnedObject::to_json)
                        .collect(),
                ),
            ),
            (
                "reserved_keys",
                Json::Arr(
                    self.reserved_keys
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
            (
                "consistency_declarations",
                Json::Arr(self.consistency_declarations.clone()),
            ),
        ];
        if let Some(c) = self.ceiling {
            m.push(("ceiling", Json::str(c.as_str())));
        }
        if let Some(t) = &self.topology_ref {
            m.push(("topology_ref", Json::str(t.clone())));
        }
        if let Some(i) = self.stage_index {
            m.push(("stage_index", Json::Int(i.min(i64::MAX as u64) as i64)));
        }
        if let Some(mp) = &self.merge_policy_ref {
            m.push(("merge_policy_ref", Json::str(mp.clone())));
        }
        if let Some(p) = &self.messaging_policy {
            m.push(("messaging_policy", p.to_json()));
        }
        Json::obj(m)
    }

    /// From canonical JSON (the decision row's `spec` member).
    pub fn from_json(j: &Json) -> Option<SubagentSpec> {
        let budget = j.get("budget")?;
        let budget_spec = BudgetSpec::from_json(budget.get("spec")?)?;
        let budget_mode = parse_budget_mode(str_of(budget.get("mode")?)?)?;
        if budget_mode != budget_spec.mode {
            return None;
        }
        let wait = j.get("wait")?;
        let isolation = j.get("isolation")?;
        Some(SubagentSpec {
            process: ChildProcess::from_json(j.get("process")?)?,
            goal: goal_from_json(j.get("goal")?)?,
            role: ChildRole::parse(j.get("role").and_then(Json::as_str).unwrap_or("subagent"))?,
            requested_grants: j
                .get("requested_grants")
                .and_then(arr_of)
                .map(|a| a.iter().filter_map(grant_from_json).collect())
                .unwrap_or_default(),
            ceiling: j
                .get("ceiling")
                .and_then(Json::as_str)
                .and_then(AuthorityClass::parse),
            budget_spec,
            budget_mode,
            environment: EnvIsolation::from_json(isolation.get("environment")?)?,
            supplies: j
                .get("supplies")
                .and_then(Supplies::from_json)
                .unwrap_or_default(),
            return_contract: ReturnContract::from_json(j.get("return_contract")?)?,
            wait_mode: WaitMode::parse(str_of(wait.get("mode")?)?)?,
            wait_timeout_ms: wait
                .get("timeout_ms")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
                .unwrap_or(0),
            on_parent_end: parse_on_parent_end(
                j.get("on_parent_end")
                    .and_then(Json::as_str)
                    .unwrap_or("cancel"),
            )?,
            delegation_reason: DelegationReason::parse(str_of(j.get("delegation_reason")?)?)?,
            topology_ref: j
                .get("topology_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            stage_index: j
                .get("stage_index")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            ownership_grants: j
                .get("ownership_grants")
                .and_then(arr_of)
                .map(|a| a.iter().filter_map(OwnedObject::from_json).collect())
                .unwrap_or_default(),
            reserved_keys: j
                .get("reserved_keys")
                .and_then(arr_of)
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            consistency_declarations: j
                .get("consistency_declarations")
                .and_then(arr_of)
                .cloned()
                .unwrap_or_default(),
            merge_policy_ref: j
                .get("merge_policy_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            messaging_policy: j
                .get("messaging_policy")
                .and_then(MessagingPolicy::from_json),
        })
    }

    /// `H(spec)` — the spec half of the `(decision, H(spec))` idempotency key:
    /// idp/1 over the canonical JSON (one hashing scheme — CC1/CC7).
    pub fn spec_hash(&self) -> String {
        hh_identity::idp::idp_id(
            "hh.subagent.spec",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// The static half of `validate` (step 2's `authorize` pre-checks —
    /// refused typed before `delegate` runs; SP-7 sealed-definition and
    /// goal-shape rules are data checks, never inferred).
    pub fn definition_errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        match &self.process {
            ChildProcess::Native { harness_def, .. } => {
                if !harness_def.is_pinned() {
                    out.push("harness_def is not pinned (version_selector)".into());
                }
            }
            ChildProcess::Hosted(_) => {
                out.push("hosted children are the T7/C2 arm".into());
            }
        }
        if self.goal.origin != GoalOrigin::Delegated {
            out.push("goal.origin must be delegated".into());
        }
        if self.budget_mode != self.budget_spec.mode {
            out.push("budget.mode and budget.spec.mode disagree".into());
        }
        if self.role == ChildRole::Judge {
            out.push("role = judge is the T4/ADR-0116 arm".into());
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ownership (ADR-0191 O-1…O-6; `OwnershipRecord` is `no-precedent`)
// ─────────────────────────────────────────────────────────────────────────────

/// `OwnedObject` — a coordination object an ownership record names. The closed
/// sum at this slice: `{fs_path_prefix | resource_key}` — environment writes
/// merge against `fs_path_prefix` ownership (the `NotOwner` check); declared
/// resource keys cover `reserved_keys` objects.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnedObject {
    /// A workspace path prefix (merge/NotOwner territory).
    FsPathPrefix(String),
    /// A named resource key (share/reservation territory).
    ResourceKey(String),
}

impl OwnedObject {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            OwnedObject::FsPathPrefix(p) => Json::obj([
                ("kind", Json::str("fs_path_prefix")),
                ("key", Json::str(p.clone())),
            ]),
            OwnedObject::ResourceKey(k) => Json::obj([
                ("kind", Json::str("resource_key")),
                ("key", Json::str(k.clone())),
            ]),
        }
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<OwnedObject> {
        let key = str_of(j.get("key")?)?.to_string();
        Some(match str_of(j.get("kind")?)? {
            "fs_path_prefix" => OwnedObject::FsPathPrefix(key),
            "resource_key" => OwnedObject::ResourceKey(key),
            _ => return None,
        })
    }

    /// Whether `self` covers `other` (`⊆` — a prefix owns its descendants).
    pub fn covers(&self, other: &OwnedObject) -> bool {
        match (self, other) {
            (OwnedObject::FsPathPrefix(a), OwnedObject::FsPathPrefix(b)) => {
                a == "/" || a.is_empty() || b == a || {
                    let mut p = a.clone();
                    if !p.ends_with('/') {
                        p.push('/');
                    }
                    b.starts_with(&p)
                }
            }
            (OwnedObject::ResourceKey(a), OwnedObject::ResourceKey(b)) => a == b,
            _ => false,
        }
    }
}

/// `OwnershipRecord` — the ledger-side record: an `OwnedObject`, the owning
/// holder (a `Ref<AgentProcess>` — the run's process ref), and the basis
/// (`grant_ref`: the `control.ownership.transferred` event that conferred it —
/// manifest/workspace-declared roots carry `grant_ref = none`).
#[derive(Debug, Clone, PartialEq)]
pub struct OwnershipRecord {
    /// The coordination object.
    pub object: OwnedObject,
    /// The current owner (an `AgentProcess`/`run` ref string).
    pub owner: String,
    /// The conferring `control.ownership.transferred` event ref (`None` for a
    /// declared root).
    pub grant_ref: Option<hh_ledger::manifest::EventRef>,
    /// The fencing generation the grant was minted under (the lease's).
    pub lease_generation: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// ChildOutcome + SubagentResult (ADR-0193 D2; ADR-0187 D1)
// ─────────────────────────────────────────────────────────────────────────────

/// `cancelled.reason ∈ {parent_stop, timeout, revoked, principal, deadline}`
/// (the `control.subagent.cancelled` closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// The parent stopped (C-2 drain).
    ParentStop,
    /// The `subagent` scope deadline passed (C-4).
    Timeout,
    /// Authority was revoked under the child (C-7).
    Revoked,
    /// The principal cancelled through a declared channel.
    Principal,
    /// A deadline force-close (scope expiry).
    Deadline,
}

impl CancelReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CancelReason::ParentStop => "parent_stop",
            CancelReason::Timeout => "timeout",
            CancelReason::Revoked => "revoked",
            CancelReason::Principal => "principal",
            CancelReason::Deadline => "deadline",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CancelReason> {
        Some(match s {
            "parent_stop" => CancelReason::ParentStop,
            "timeout" => CancelReason::Timeout,
            "revoked" => CancelReason::Revoked,
            "principal" => CancelReason::Principal,
            "deadline" => CancelReason::Deadline,
            _ => return None,
        })
    }
}

/// `ChildOutcome` — the closed sum (ADR-0193 D2; `scored ↦ result` in the
/// result record — a scored child reports `Result`; `outcome ≠ result`
/// children land in `MergeReport.absent[]` — G-5).
#[derive(Debug, Clone, PartialEq)]
pub enum ChildOutcome {
    /// The child produced a result (the `scored` arm lowers here).
    Result,
    /// `cancelled{reason}` — the closed `CancelReason` set.
    Cancelled(CancelReason),
    /// `infrastructure_failure{kind}` — `child_unresponsive` (C-2's
    /// drain-exceeded case), `crash` (the child's own recovery never
    /// completed), `ledger_fault`.
    InfrastructureFailure(String),
}

impl ChildOutcome {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChildOutcome::Result => "result",
            ChildOutcome::Cancelled(_) => "cancelled",
            ChildOutcome::InfrastructureFailure(_) => "infrastructure_failure",
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            ChildOutcome::Result => Json::obj([("kind", Json::str("result"))]),
            ChildOutcome::Cancelled(r) => Json::obj([
                ("kind", Json::str("cancelled")),
                ("reason", Json::str(r.as_str())),
            ]),
            ChildOutcome::InfrastructureFailure(k) => Json::obj([
                ("kind", Json::str("infrastructure_failure")),
                ("failure_kind", Json::str(k.clone())),
            ]),
        }
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<ChildOutcome> {
        Some(match str_of(j.get("kind")?)? {
            "result" | "scored" => ChildOutcome::Result,
            "cancelled" => ChildOutcome::Cancelled(CancelReason::parse(str_of(j.get("reason")?)?)?),
            "infrastructure_failure" => ChildOutcome::InfrastructureFailure(
                j.get("failure_kind")
                    .and_then(Json::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
            ),
            _ => return None,
        })
    }
}

/// A `ReturnContract` violation — `{field, reason}` (typed, retained; the
/// kernel never truncates — M-2).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractViolation {
    /// The contract member violated (`artifacts[i]`, `summary.max_tokens`, …).
    pub field: String,
    /// The violation kind (`missing_required`, `schema_failure`,
    /// `over_max_tokens`, `claims_absent`).
    pub reason: String,
}

/// `SubagentResult` — the `control.subagent.result` payload (ADR-0187 D1 as
/// amended). Written under the parent's lease after the child's terminal;
/// results are artifacts and events, never a transcript (M-1).
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentResult {
    /// The child run.
    pub child_run_id: String,
    /// The `control.decision{kind: delegate}` ref the spawn hung from.
    pub delegation_ref: String,
    /// The closed `ChildOutcome`.
    pub outcome: Option<ChildOutcome>,
    /// The ADR-0045 outcome class spelling.
    pub outcome_class: String,
    /// The ADR-0106 stop reason spelling.
    pub stop_reason: String,
    /// `child_head{seq, hash, checkpoint_ref?}` — the anchor a result/cancelled
    /// row carries (I-A6).
    pub child_head: Option<Json>,
    /// `artifacts: [ArtifactRef]` — content addresses produced by the child.
    pub artifacts: Vec<Json>,
    /// `fs_changes` — `FsChangeSet | SnapshotRef` (the merge input).
    pub fs_changes: Option<Json>,
    /// `summary` — `Ref<Artifact{kind: subagent_summary}>` (`authority =
    /// min(⊔ child inputs, delegate)`).
    pub summary: Option<Json>,
    /// `claims: [ClaimRef]`.
    pub claims: Vec<String>,
    /// `memories_written[]`.
    pub memories_written: Vec<String>,
    /// `effects_irreversible[]`.
    pub effects_irreversible: Vec<String>,
    /// `usage: ResourceVector` — the child's accounted spend.
    pub usage: Json,
    /// `budget_released` — the slice remainder moved back to the parent.
    pub budget_released: bool,
    /// `verdicts: [VerdictRef]`.
    pub verdicts: Vec<String>,
    /// `return_contract_satisfied`.
    pub return_contract_satisfied: bool,
    /// `violations: [{field, reason}]` — typed, retained.
    pub violations: Vec<ContractViolation>,
}

impl SubagentResult {
    /// The canonical JSON — the `control.subagent.result` payload.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("child_run_id", Json::str(self.child_run_id.clone())),
            ("delegation_ref", Json::str(self.delegation_ref.clone())),
            ("outcome_class", Json::str(self.outcome_class.clone())),
            ("stop_reason", Json::str(self.stop_reason.clone())),
            ("artifacts", Json::Arr(self.artifacts.clone())),
            (
                "claims",
                Json::Arr(self.claims.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "memories_written",
                Json::Arr(
                    self.memories_written
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
            (
                "effects_irreversible",
                Json::Arr(
                    self.effects_irreversible
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
            ("usage", self.usage.clone()),
            ("budget_released", Json::Bool(self.budget_released)),
            (
                "verdicts",
                Json::Arr(self.verdicts.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "return_contract_satisfied",
                Json::Bool(self.return_contract_satisfied),
            ),
            (
                "violations",
                Json::Arr(
                    self.violations
                        .iter()
                        .map(|v| {
                            Json::obj([
                                ("field", Json::str(v.field.clone())),
                                ("reason", Json::str(v.reason.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ];
        if let Some(o) = &self.outcome {
            m.push(("outcome", o.to_json()));
        }
        if let Some(h) = &self.child_head {
            m.push(("child_head", h.clone()));
        }
        if let Some(f) = &self.fs_changes {
            m.push(("fs_changes", f.clone()));
        }
        if let Some(s) = &self.summary {
            m.push(("summary", s.clone()));
        }
        Json::obj(m)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MessagingPolicy + caps (ADR-0191 M-2; OQ-417 kernel constants; OQ-428 defaults)
// ─────────────────────────────────────────────────────────────────────────────

/// `MessagingPolicy{parent_to_child, child_to_parent, sibling, broadcast,
/// max_pending, coalesce}` — MUST-data of the sealed definition (ADR-0191
/// M-2). The OQ-428 ratified defaults are `sibling: false`, `broadcast: false`
/// — admissible as declared data at this slice (tree-local reach only).
#[derive(Debug, Clone, PartialEq)]
pub struct MessagingPolicy {
    /// Parent → child permitted.
    pub parent_to_child: bool,
    /// Child → parent permitted.
    pub child_to_parent: bool,
    /// Sibling → sibling permitted (OQ-428 default `false`; relayed via the
    /// parent when `true` — never direct).
    pub sibling: bool,
    /// Broadcast permitted (OQ-428 default `false`).
    pub broadcast: bool,
    /// Pending-message bound per receiver (`max_pending`; queue-full is
    /// `MessageRefused{queue}`, never a drop).
    pub max_pending: u64,
    /// Coalescing rule spelling (`none | latest`).
    pub coalesce: String,
}

impl Default for MessagingPolicy {
    fn default() -> Self {
        MessagingPolicy {
            parent_to_child: true,
            child_to_parent: true,
            sibling: false,
            broadcast: false,
            max_pending: 16,
            coalesce: "none".to_string(),
        }
    }
}

impl MessagingPolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("parent_to_child", Json::Bool(self.parent_to_child)),
            ("child_to_parent", Json::Bool(self.child_to_parent)),
            ("sibling", Json::Bool(self.sibling)),
            ("broadcast", Json::Bool(self.broadcast)),
            (
                "max_pending",
                Json::Int(self.max_pending.min(i64::MAX as u64) as i64),
            ),
            ("coalesce", Json::str(self.coalesce.clone())),
        ])
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<MessagingPolicy> {
        Some(MessagingPolicy {
            parent_to_child: j.get("parent_to_child").and_then(bool_of).unwrap_or(true),
            child_to_parent: j.get("child_to_parent").and_then(bool_of).unwrap_or(true),
            sibling: j.get("sibling").and_then(bool_of).unwrap_or(false),
            broadcast: j.get("broadcast").and_then(bool_of).unwrap_or(false),
            max_pending: j
                .get("max_pending")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
                .unwrap_or(16),
            coalesce: j
                .get("coalesce")
                .and_then(Json::as_str)
                .unwrap_or("none")
                .to_string(),
        })
    }
}

/// The kernel message caps (OQ-417 — constants plus the `MessagingPolicy`,
/// never a new accounting dimension at this stage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageCaps {
    /// Max body bytes (`size` refusal past it).
    pub max_body_bytes: u64,
    /// Max sent messages per sender run (`rate` refusal past it).
    pub max_per_sender: u64,
    /// Hard bound on any receiver's pending queue (`queue` refusal — the
    /// policy's `max_pending` may only tighten it).
    pub max_pending_hard: u64,
}

impl MessageCaps {
    /// The Stage-4 kernel constants (ADR-0214 OQ-417 interim).
    pub const KERNEL: MessageCaps = MessageCaps {
        max_body_bytes: 16 * 1024,
        max_per_sender: 256,
        max_pending_hard: 64,
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// MergePolicy + MergeReport + conflicts (ADR-0192; M-3; G-1/G-2/G-5)
// ─────────────────────────────────────────────────────────────────────────────

/// `MergePolicy` — the one closed sum (ADR-0192 D1). This slice implements
/// `single_writer` (`refuse`) and `parent_decides` (`ask`); the
/// `three_way_text{line}`/`validator_selected` arms are the R-2.6.5 ticket's
/// (declared here — `MergePolicyUnsupported`, never a silent refusal).
#[derive(Debug, Clone, PartialEq)]
pub enum MergePolicy {
    /// `refuse` — conflicts refuse the merge (single writer owns each object).
    SingleWriter,
    /// `ask` — conflicts surface as `MergeConflict` records the parent
    /// resolves by `control.merge.resolved{resolution: choose{side}}` (G-2).
    ParentDecides,
    /// `three_way_text{line}` — declared (the S4.8/R-2.6.5 arm).
    ThreeWayText,
    /// `validator_selected` — declared (the S4.8/R-2.6.5 arm).
    ValidatorSelected,
}

impl MergePolicy {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            MergePolicy::SingleWriter => "single_writer",
            MergePolicy::ParentDecides => "parent_decides",
            MergePolicy::ThreeWayText => "three_way_text",
            MergePolicy::ValidatorSelected => "validator_selected",
        }
    }

    /// Parse a canonical spelling (the `refuse`/`ask` CoordinationPolicy
    /// spellings lower here — ADR-0192 D1's equivalence).
    pub fn parse(s: &str) -> Option<MergePolicy> {
        Some(match s {
            "single_writer" | "refuse" => MergePolicy::SingleWriter,
            "parent_decides" | "ask" => MergePolicy::ParentDecides,
            "three_way_text" | "three_way_text{line}" => MergePolicy::ThreeWayText,
            "validator_selected" => MergePolicy::ValidatorSelected,
            _ => return None,
        })
    }

    /// Whether this slice implements the arm.
    pub fn implemented(&self) -> bool {
        matches!(self, MergePolicy::SingleWriter | MergePolicy::ParentDecides)
    }
}

/// `MergeConflictRecord{conflict_id, kind: workspace, path, parent_baseline,
/// child_before, child_after, parent_current}` (M-3).
#[derive(Debug, Clone, PartialEq)]
pub struct MergeConflictRecord {
    /// The conflict id (content-addressed over the four heads).
    pub conflict_id: String,
    /// The conflict kind (`workspace` at this slice).
    pub kind: String,
    /// The object (path).
    pub path: String,
    /// The baseline head (fork-point content address).
    pub parent_baseline: Option<String>,
    /// The child's pre-write content address.
    pub child_before: Option<String>,
    /// The child's post-write content address.
    pub child_after: Option<String>,
    /// The parent's current content address.
    pub parent_current: Option<String>,
    /// The child the conflict came from.
    pub child_run_id: String,
}

impl MergeConflictRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("conflict_id", Json::str(self.conflict_id.clone())),
            ("kind", Json::str(self.kind.clone())),
            ("path", Json::str(self.path.clone())),
            ("child_run_id", Json::str(self.child_run_id.clone())),
        ];
        for (k, v) in [
            ("parent_baseline", &self.parent_baseline),
            ("child_before", &self.child_before),
            ("child_after", &self.child_after),
            ("parent_current", &self.parent_current),
        ] {
            match v {
                Some(x) => m.push((k, Json::str(x.clone()))),
                None => m.push((k, Json::Null)),
            }
        }
        Json::obj(m)
    }
}

/// `MergeResolution` — the closed sum (G-2; `choose`/`supersede` are the
/// parent's `delegate`-class resolutions; `escalate` routes through WS-H7 —
/// refused `IllegitimateResolution` at this slice).
#[derive(Debug, Clone, PartialEq)]
pub enum MergeResolution {
    /// `choose{side}` — pick the parent or the child side.
    Choose { side: ChooseSide },
    /// `supersede{new_ref}` — a new value supersedes both sides.
    Supersede { new_ref: String },
    /// `escalate` — human escalation (requires the H7 channel — refused at
    /// this slice as `IllegitimateResolution`).
    Escalate,
}

/// The `choose` side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChooseSide {
    /// The parent's current value stands.
    Parent,
    /// The child's value is applied as the parent's own effect.
    Child,
}

/// `MergeReport{merge_id, parent_run_id, children[], merged[], conflicts[],
/// absent[], lost_write_count, provenance, derived_from}` — a `project()` view
/// (ADR-0192 D3 — rebuild-equality tested).
#[derive(Debug, Clone, PartialEq)]
pub struct MergeReport {
    /// The merge id (`control.merge.started`'s event id).
    pub merge_id: String,
    /// The parent run.
    pub parent_run_id: String,
    /// The children merged.
    pub children: Vec<String>,
    /// The merged inputs (`{child_run_id, path, after_ref, merged_by}`).
    pub merged: Vec<Json>,
    /// The conflict records.
    pub conflicts: Vec<MergeConflictRecord>,
    /// `outcome ≠ result` children (G-5 — they contribute nothing).
    pub absent: Vec<String>,
    /// `lost_write_count` — kernel-computed (G-1; must be 0 to complete).
    pub lost_write_count: u64,
    /// The provenance record of the fold.
    pub provenance: Json,
    /// `derived_from` — the child results/reports the fold consumed.
    pub derived_from: Json,
}

impl MergeReport {
    /// The canonical JSON (`control.merge.completed{merge_report_ref}` cites
    /// this record's content address).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("merge_id", Json::str(self.merge_id.clone())),
            ("parent_run_id", Json::str(self.parent_run_id.clone())),
            (
                "children",
                Json::Arr(self.children.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            ("merged", Json::Arr(self.merged.clone())),
            (
                "conflicts",
                Json::Arr(
                    self.conflicts
                        .iter()
                        .map(MergeConflictRecord::to_json)
                        .collect(),
                ),
            ),
            (
                "absent",
                Json::Arr(self.absent.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "lost_write_count",
                Json::Int(self.lost_write_count.min(i64::MAX as u64) as i64),
            ),
            ("provenance", self.provenance.clone()),
            ("derived_from", self.derived_from.clone()),
        ])
    }

    /// The report's content address (`merge_report_ref`).
    pub fn report_ref(&self) -> String {
        hh_identity::idp::address(
            self.to_json().to_canonical_string().as_bytes(),
            "application/json",
        )
        .id()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Spawned / SpawnRefused / MessageRefused — the typed outcomes
// ─────────────────────────────────────────────────────────────────────────────

/// `Spawned{child_run_id, child_handles[], budget_id, env_handle_id?,
/// spawn_event}` — the successful spawn's return (§5e.3 op table). `spawn_event`
/// is the `EventRef` of the parent's `control.subagent.spawned` row — the
/// `delegated-to` edge the child's manifest names.
#[derive(Debug, Clone)]
pub struct Spawned {
    /// The child run id (`sub-<H(decision, spec)>` — deterministic for the
    /// `(decision, H(spec))` idempotency pair).
    pub child_run_id: String,
    /// The minted child `HandleId`s (delegation-basis; `ceiling` recorded on
    /// the `spawned` row).
    pub child_handles: Vec<String>,
    /// The child's `BudgetNode` (a `slice`/`pool` child of the parent's node).
    pub budget_id: String,
    /// The derived environment handle (`isolation.environment = derive{…}`).
    pub env_handle_id: Option<String>,
    /// The `control.subagent.spawned` event ref — the delegated-to edge.
    pub spawn_event: hh_ledger::manifest::EventRef,
    /// The child's writer lease (its own — SP-5). `None` on the adopt path
    /// when the original attempt's lease is still live elsewhere (lease
    /// expiry fences it — the caller never fabricates one).
    pub child_lease: Option<hh_ledger::store::Lease>,
    /// The `wait` arm the caller drives (`await` ⇒ the parent waits for the
    /// `delegation_completed` cue; `background` ⇒ return immediately).
    pub wait_mode: WaitMode,
    /// The `child_terminal` subscription id on the parent.
    pub subscription_id: String,
}

/// `SpawnRefused` — the closed refusal sum (§5e.3; a typed observation to the
/// model, never a run stop).
#[derive(Debug, Clone, PartialEq)]
pub enum SpawnRefused {
    /// `fan_out`/`spawns` cap — the next increment refused (C-8).
    FanOut { dimension: String },
    /// `delegation_depth + 1 > cap`.
    Depth { depth: u64, cap: u64 },
    /// The `spawns` reservation or a budget quantity could not be held.
    InsufficientBudget { dimension: String },
    /// A requested grant is not ⊆ `delegable(parent)` (SP-1).
    AuthorityWidening { domain: String },
    /// The parent handle is not `delegable`.
    NotDelegable { handle: String },
    /// `spec.budget` exceeds the parent's remaining (ledgered `refused`).
    BudgetExceedsParent { detail: String },
    /// `ownership_grants[] ⊄` the parent's valid ownerships (step 4b).
    NotOwned { object: Json },
    /// `share` isolation resource locks could not be held all-or-nothing
    /// (the Stage-5 arm — declared for payload completeness).
    ResourceLockTimeout { keys: Vec<String> },
    /// A declared option is out of this slice (`share`, `hosted`, `judge`,
    /// `three_way_*` merges, `steer` delivery, …).
    ModeUnsupported { detail: String },
    /// The child definition does not resolve (SP-7: unpinned
    /// `harness_def`, malformed goal, `tools ⊄` the sealed table).
    DefinitionUnresolvable { detail: String },
    /// No delegation machinery is bound (T0; the profile lacks the
    /// `subagents` capability).
    DelegationUnavailable,
    /// An `unattended` child asked under an `interactive` parent where the
    /// policy column does not admit it (OQ-418 interim — currently
    /// `attendance` inherits; the refusal variant is declared).
    UnattendedAsk { detail: String },
    /// `control.decision{kind: delegate}` without `delegation_reason`.
    MissingDelegationReason,
}

impl SpawnRefused {
    /// The canonical spelling (the refused row's `reason`).
    pub fn as_str(&self) -> &'static str {
        match self {
            SpawnRefused::FanOut { .. } => "fan_out",
            SpawnRefused::Depth { .. } => "depth",
            SpawnRefused::InsufficientBudget { .. } => "insufficient_budget",
            SpawnRefused::AuthorityWidening { .. } => "authority_widening",
            SpawnRefused::NotDelegable { .. } => "not_delegable",
            SpawnRefused::BudgetExceedsParent { .. } => "budget_exceeds_parent",
            SpawnRefused::NotOwned { .. } => "not_owned",
            SpawnRefused::ResourceLockTimeout { .. } => "resource_lock_timeout",
            SpawnRefused::ModeUnsupported { .. } => "mode_unsupported",
            SpawnRefused::DefinitionUnresolvable { .. } => "definition_unresolvable",
            SpawnRefused::DelegationUnavailable => "delegation_unavailable",
            SpawnRefused::UnattendedAsk { .. } => "unattended_ask",
            SpawnRefused::MissingDelegationReason => "missing_delegation_reason",
        }
    }

    /// The typed observation payload (`{reason, …detail}`).
    pub fn to_json(&self) -> Json {
        let mut m = vec![("reason", Json::str(self.as_str()))];
        match self {
            SpawnRefused::FanOut { dimension } => {
                m.push(("dimension", Json::str(dimension.clone())));
            }
            SpawnRefused::Depth { depth, cap } => {
                m.push(("depth", Json::Int(*depth as i64)));
                m.push(("cap", Json::Int(*cap as i64)));
            }
            SpawnRefused::InsufficientBudget { dimension } => {
                m.push(("dimension", Json::str(dimension.clone())));
            }
            SpawnRefused::AuthorityWidening { domain } => {
                m.push(("domain", Json::str(domain.clone())));
            }
            SpawnRefused::NotDelegable { handle } => {
                m.push(("handle", Json::str(handle.clone())));
            }
            SpawnRefused::BudgetExceedsParent { detail }
            | SpawnRefused::ModeUnsupported { detail }
            | SpawnRefused::DefinitionUnresolvable { detail }
            | SpawnRefused::UnattendedAsk { detail } => {
                m.push(("detail", Json::str(detail.clone())));
            }
            SpawnRefused::NotOwned { object } => {
                m.push(("object", object.clone()));
            }
            SpawnRefused::ResourceLockTimeout { keys } => {
                m.push((
                    "keys",
                    Json::Arr(keys.iter().map(|s| Json::str(s.clone())).collect()),
                ));
            }
            SpawnRefused::DelegationUnavailable | SpawnRefused::MissingDelegationReason => {}
        }
        Json::obj(m)
    }
}

/// The spawn error channel — a spec refusal (typed, observable) or a kernel
/// failure (store/budget/ledger — the caller's error channel, never a silent
/// `SpawnRefused`).
#[derive(Debug)]
pub enum SpawnError {
    /// The typed refusal (a typed observation to the model — never a stop).
    Refused(SpawnRefused),
    /// A kernel-side failure (the append path, budget engine, derive port,
    /// store errors — `Durability{injected}` faults surface here too).
    Kernel(String),
}

impl From<SpawnRefused> for SpawnError {
    fn from(r: SpawnRefused) -> SpawnError {
        SpawnError::Refused(r)
    }
}

/// `MessageRefused{policy | rate | size | queue | reach}` — the closed
/// message-refusal sum (§5e.3 op table; caps refuse, never drop).
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRefused {
    /// The `MessagingPolicy` forbids the direction (`sibling: false`, …).
    Policy { direction: String },
    /// The sender's `max_per_sender` cap.
    Rate,
    /// The body exceeds `max_body_bytes`.
    Size { bytes: u64 },
    /// The receiver's pending queue is full (`min(policy.max_pending,
    /// caps.max_pending_hard)`).
    Queue,
    /// The target is outside tree-local reach (parent/direct children/
    /// siblings/relay) — `broadcast` with `broadcast: false`.
    Reach { target: String },
}

impl MessageRefused {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRefused::Policy { .. } => "policy",
            MessageRefused::Rate => "rate",
            MessageRefused::Size { .. } => "size",
            MessageRefused::Queue => "queue",
            MessageRefused::Reach { .. } => "reach",
        }
    }
}

/// `Receipt{message_id, status}` — `send_message`'s return (§5e.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    /// The `control.message.sent` event id (or the `refused` row's).
    pub message_id: String,
    /// `queued | delivered | refused{reason} | undeliverable`.
    pub status: MessageStatus,
}

/// The `Receipt.status` sum.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageStatus {
    /// Recorded on the receiver's subscription (delivery pending the wake
    /// drain).
    Queued,
    /// Fired into the receiver's inbox.
    Delivered,
    /// `refused{reason}` (typed).
    Refused(MessageRefused),
    /// A `finished` receiver without a live goal-scoped subscription (the
    /// occurrence is `skipped{run_finished}` — audited, C-9).
    Undeliverable { reason: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// Small helpers — the single Ref/Grant/Goal JSON tables (CC7)
// ─────────────────────────────────────────────────────────────────────────────

/// `Ref` from canonical JSON (`{semantic_id, version_id | version_selector}`).
pub fn ref_from_json(j: &Json) -> Option<Ref> {
    let semantic_id = str_of(j.get("semantic_id")?)?.to_string();
    if let Some(v) = j.get("version_id").and_then(Json::as_str) {
        return Some(Ref::pinned(semantic_id, v.to_string()));
    }
    if let Some(v) = j.get("version_selector").and_then(Json::as_str) {
        return Some(Ref::selected(semantic_id, v.to_string()));
    }
    None
}

fn grant_to_json(g: &Grant) -> Json {
    Json::obj([
        ("effect", g.effect.to_json()),
        ("scope", Json::str(g.scope.clone())),
        ("constraints", grant_constraints_json(&g.constraints)),
        ("delegable", Json::Bool(g.delegable)),
    ])
}

fn grant_constraints_json(c: &GrantConstraints) -> Json {
    let mut m = vec![];
    if let Some(b) = &c.budget {
        m.push(("budget", b.clone()));
    }
    if let Some(t) = c.time {
        m.push(("time", Json::Int(t.min(i64::MAX as u64) as i64)));
    }
    if let Some(n) = c.count {
        m.push(("count", Json::Int(n.min(i64::MAX as u64) as i64)));
    }
    Json::obj(m)
}

fn grant_from_json(j: &Json) -> Option<Grant> {
    let c = j.get("constraints");
    Some(Grant {
        effect: hh_hir::kinds::EffectClass::from_json(j.get("effect")?, "grant.effect").ok()?,
        scope: str_of(j.get("scope")?)?.to_string(),
        constraints: GrantConstraints {
            budget: c.and_then(|x| x.get("budget")).cloned(),
            time: c
                .and_then(|x| x.get("time"))
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            count: c
                .and_then(|x| x.get("count"))
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
        },
        delegable: j.get("delegable").and_then(bool_of).unwrap_or(false),
    })
}

/// `BudgetMode` spelling (`slice | pool` — the hh-budget enum carries no codec).
pub fn budget_mode_str(m: BudgetMode) -> &'static str {
    match m {
        BudgetMode::Slice => "slice",
        BudgetMode::Pool => "pool",
    }
}

/// Parse a `BudgetMode` spelling.
pub fn parse_budget_mode(s: &str) -> Option<BudgetMode> {
    Some(match s {
        "slice" => BudgetMode::Slice,
        "pool" => BudgetMode::Pool,
        _ => return None,
    })
}

fn goal_to_json(g: &GoalRecord) -> Json {
    let mut m = vec![
        ("statement", g.statement.to_json()),
        (
            "success_criteria",
            Json::Arr(g.success_criteria.iter().map(Ref::to_json).collect()),
        ),
        ("budget", g.budget.to_json()),
        ("origin", Json::str(g.origin.name())),
    ];
    if let Some(u) = &g.unverifiable_reason {
        m.push(("unverifiable_reason", u.to_json()));
    }
    if let Some(p) = &g.parent {
        m.push(("parent", p.to_json()));
    }
    Json::obj(m)
}

fn goal_from_json(j: &Json) -> Option<GoalRecord> {
    // The `statement` leaf arrives content-addressed (canonical form), never
    // as raw bytes (SP-3). A bare string is a shorthand — it stays a leaf
    // (`authority = external`; the goal's statement is never a decider).
    let stmt = j.get("statement")?;
    let statement = if let Some(s) = stmt.as_str() {
        let prov = hh_provenance::ProvenanceRecord::kernel("hh-subagent", 0);
        let mut t = Text::new(s, "parent", prov);
        t.authority = AuthorityClass::External;
        t
    } else {
        Text::from_json(stmt, "goal.statement").ok()?
    };
    Some(GoalRecord {
        statement,
        success_criteria: j
            .get("success_criteria")
            .and_then(arr_of)
            .map(|a| a.iter().filter_map(ref_from_json).collect())
            .unwrap_or_default(),
        unverifiable_reason: None,
        budget: j
            .get("budget")
            .and_then(ref_from_json)
            .unwrap_or_else(|| Ref {
                semantic_id: "budget".to_string(),
                version: RefVersion::Pinned("sha256:zero".to_string()),
            }),
        origin: match j.get("origin").and_then(Json::as_str) {
            Some("human") => GoalOrigin::Human,
            Some("system") => GoalOrigin::System,
            Some("scheduled") => GoalOrigin::Scheduled,
            _ => GoalOrigin::Delegated,
        },
        parent: j.get("parent").and_then(ref_from_json),
    })
}

/// The `Owner` spelling (the spawn row's `owner` + the `delegation_reason`'s
/// claimed owner — `model_claim` vs `code`-declared labels derive from it).
pub fn owner_str(o: Owner) -> &'static str {
    match o {
        Owner::Code => "code",
        Owner::Model => "model",
        Owner::Human => "human",
    }
}

impl Default for SubagentResult {
    fn default() -> SubagentResult {
        SubagentResult {
            child_run_id: String::new(),
            delegation_ref: String::new(),
            outcome: None,
            outcome_class: String::new(),
            stop_reason: String::new(),
            child_head: None,
            artifacts: Vec::new(),
            fs_changes: None,
            summary: None,
            claims: Vec::new(),
            memories_written: Vec::new(),
            effects_irreversible: Vec::new(),
            usage: Json::Null,
            budget_released: false,
            verdicts: Vec::new(),
            return_contract_satisfied: false,
            violations: Vec::new(),
        }
    }
}

impl Default for MergeReport {
    fn default() -> MergeReport {
        MergeReport {
            merge_id: String::new(),
            parent_run_id: String::new(),
            children: Vec::new(),
            merged: Vec::new(),
            conflicts: Vec::new(),
            absent: Vec::new(),
            lost_write_count: 0,
            provenance: Json::Null,
            derived_from: Json::Null,
        }
    }
}
