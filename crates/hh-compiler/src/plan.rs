//! `RuntimePlan/1` (§3.2.4) and `lower_native` (§3.2.2 stage 2): the closed plan-node set
//! `{loop, step, branch-on-validator, delegate, stop-rule}`, the tool/policy tables, the
//! budget envelope + stop-rule nodes the control boundary lowers into (ADR-0106), and the
//! `hir_node_id` traceability every plan node carries. Everything the interpreter needs is
//! data here — the compiler emits no host-language code (ADR-0019 D2).

use std::collections::BTreeMap;

use hh_hir::{
    kinds::ToolEffects,
    records::{
        BudgetRecord, DimensionBound, KindRecord, NativeProcess, ProcedureRecord, ProcedureStep,
        RuleAction, SlotBinding,
    },
    refs::RefVersion,
    DefinitionVersionRef, HirDocument, Node, Ref,
};
use hh_ontology::control::{ControlBoundary, StopKind};
use hh_wire::json::Json;

use crate::errors::CompileError;
use crate::link::LinkedGraph;

/// `RuntimePlan/1` — the schema tag (§3.2.4).
pub const RUNTIME_PLAN_DIALECT: &str = "RuntimePlan/1";

/// The `RuntimePlan/1` record (§3.2.4): `{control, tools, policies, budget,
/// context_policy, validators, ids}`.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimePlan {
    /// The control spine — the closed `PlanNode` set.
    pub control: Vec<PlanNode>,
    /// The tool table (`ToolBinding[]`).
    pub tools: Vec<ToolBinding>,
    /// The policy tables — derived only from `Permission`/`EffectClass`/`Budget`
    /// entities, never from surfaces (§3.2.4).
    pub policies: PolicyTables,
    /// The budget envelope the control boundary lowers into (ADR-0106) — `None` only when
    /// the definition binds no process budget.
    pub budget: Option<BudgetEnvelope>,
    /// The `context_policy` slot's resolved parameters (`context_policy_params`).
    pub context_policy: Option<BoundSlot>,
    /// Every bound slot (slot → pinned variant + params) — the bound components the plan
    /// carries for the interpreter.
    pub bound_slots: BTreeMap<String, BoundSlot>,
    /// The validator bindings (`ValidatorBinding[]`).
    pub validators: Vec<ValidatorBinding>,
    /// `ids{definition_ref, artifact_ids[]}` — the artifact/ledger id block.
    pub ids: PlanIds,
}

/// A pinned entity coordinate inside the plan — `{semantic_id, version_id}` (the plan
/// never carries a selector; §3.2.2 stage 1 has already refused unpinned input).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinnedRef {
    /// The target's semantic id.
    pub semantic_id: String,
    /// The target's pinned version id.
    pub version_id: String,
}

/// Resolve a (pinned) `Ref` to its `PinnedRef`; `None` when the ref carries a selector —
/// which cannot happen post-`link_precheck` but is checked, not assumed.
pub fn pinned_ref(r: &Ref) -> Option<PinnedRef> {
    match &r.version {
        RefVersion::Pinned(v) => Some(PinnedRef {
            semantic_id: r.semantic_id.clone(),
            version_id: v.clone(),
        }),
        RefVersion::Selector(_) => None,
    }
}

/// The `ids` block — artifact/ledger identity coordinates the plan carries.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanIds {
    /// The sealed definition's `{semantic_id, version_id}`.
    pub definition_ref: DefinitionVersionRef,
    /// `Artifact` nodes the plan references — `{node semantic_id → content_hash}`.
    pub artifact_ids: BTreeMap<String, String>,
}

/// A plan node — `{node_id, hir_node_id, hir_version_id, payload}`. `node_id` is a
/// deterministic plan-local coordinate (`n<depth-first index>`); `hir_node_id` is the
/// source node's `semantic_id` — **every** node carries it (§3.2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    /// The plan-local coordinate.
    pub node_id: String,
    /// The source node's semantic id (traceability — never absent).
    pub hir_node_id: String,
    /// The source node's pinned version id.
    pub hir_version_id: String,
    /// The closed node payload.
    pub payload: PlanNodePayload,
}

/// The closed `RuntimePlan/1` node kinds (§3.2.4): `{loop, step, branch-on-validator,
/// delegate, stop-rule}`. No other construct is a plan node — anything else is a
/// `PlanError{unsupported_construct}`.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanNodePayload {
    /// `loop{bound_budget, replan_on, body}`.
    Loop(LoopNode),
    /// `step{mode, output_schema, action}`.
    Step(StepNode),
    /// `branch-on-validator{validator, then, else}`.
    BranchOnValidator(BranchOnValidatorNode),
    /// `delegate{spec, budget, permission}`.
    Delegate(DelegateNode),
    /// `stop-rule{reason, bound?, condition?}` — the boundary's envelope-reserved points
    /// plus per-`Budget` `budget_exhausted` rules (ADR-0106).
    StopRule(StopRuleNode),
}

/// `loop` — bounded by construction; `replan_on` is the ADR-0103 attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopNode {
    /// The budget bounding the loop.
    pub bound_budget: PinnedRef,
    /// `replan_on` — the closed replan triggers.
    pub replan_on: ReplanOn,
    /// The body nodes.
    pub body: Vec<PlanNode>,
}

/// `loop.replan_on` (ADR-0103/CF-219 — the closed trigger set `{never, failure,
/// always}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanOn {
    /// Never replan.
    Never,
    /// Replan on a step/branch failure.
    Failure,
    /// Replan every iteration.
    Always,
}

impl ReplanOn {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ReplanOn::Never => "never",
            ReplanOn::Failure => "failure",
            ReplanOn::Always => "always",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "never" => ReplanOn::Never,
            "failure" => ReplanOn::Failure,
            "always" => ReplanOn::Always,
            _ => return None,
        })
    }
}

/// `step{mode, output_schema, action}` (ADR-0103: `step.mode` and `step.output_schema`
/// are plan-node attributes; `output_schema` is a `Ref<Validator{kind: schema}>` —
/// pinned by stage 1 like every plan ref).
#[derive(Debug, Clone, PartialEq)]
pub struct StepNode {
    /// `sequential | parallel`.
    pub mode: StepMode,
    /// The step's output schema — a pinned `Ref<Validator{kind: schema}>`. No HIR source
    /// declares it at C0 (`None` until the declared source lands — CC8 additive field).
    pub output_schema: Option<PinnedRef>,
    /// The step's action.
    pub action: StepAction,
}

/// `step.mode` — the closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// The step runs in sequence.
    Sequential,
    /// The step may run in parallel with its siblings.
    Parallel,
}

/// `step.action` — what a step does (the closed Stage-1 set).
#[derive(Debug, Clone, PartialEq)]
pub enum StepAction {
    /// An `Instruction` step — the prose is content-addressed (`content_hash`), never
    /// inlined into the plan's identity-bearing fields.
    Instruction {
        /// The `Text` leaf's content address (`hir.text` idp/1).
        content_hash: String,
        /// The prose owner (the `Text` leaf's owner — carried, not conferred).
        owner: String,
    },
    /// An `Invoke` step — a capability call.
    Invoke {
        /// The bound capability.
        capability: PinnedRef,
        /// The argument binding record.
        args: Json,
    },
}

/// `branch-on-validator{validator, then, else}` — the only conditional the plan knows.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchOnValidatorNode {
    /// The validator whose verdict steers.
    pub validator: PinnedRef,
    /// The pass body.
    pub then_body: Vec<PlanNode>,
    /// The fail body.
    pub else_body: Vec<PlanNode>,
}

/// `delegate{spec, budget, permission}`.
#[derive(Debug, Clone, PartialEq)]
pub struct DelegateNode {
    /// The sub-process spec (§3.2-owned record, carried as data).
    pub spec: Json,
    /// The sub-budget (must be `≤` the enclosing budget — validated upstream).
    pub budget: PinnedRef,
    /// The delegated permission.
    pub permission: PinnedRef,
}

/// `stop-rule{reason, bound?, condition?}` — a stop-rule node the boundary's
/// envelope-reserved points and bound budgets lower into.
#[derive(Debug, Clone, PartialEq)]
pub struct StopRuleNode {
    /// The `StopKind` tag of the control-plane `StopReason` the rule fires
    /// (§5e.2; ADR-0106 D6 — the sum is owned by `hh_ontology::control`; this
    /// node's `reason` member carries the tag spelling, never a second
    /// three-member subset — retired at S1.20, CC1).
    pub reason: StopKind,
    /// The budget the rule binds (for `budget_exhausted`).
    pub bound: Option<PinnedRef>,
    /// The guard condition (a boundary `guards` entry or rule trigger — carried as data).
    pub condition: Option<Json>,
}

/// `ToolBinding` — the plan's tool table row (§3.2.4): the capability's declared
/// semantic content (effects/preconditions/scope/observation contract) plus the
/// `SurfaceBinding` when the node carries a `Tool` surface (ADR-0090).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolBinding {
    /// The capability.
    pub capability: PinnedRef,
    /// The declared effect set (`[]` iff `effects = pure`).
    pub effects: Vec<hh_hir::EffectClass>,
    /// The declared precondition domains.
    pub preconditions: Vec<String>,
    /// The scope-bindings record (`scope_bindings` or `{"unknown": true}` — carried, never
    /// coerced).
    pub scope_bindings: Json,
    /// The observation contract (§5b-owned shape — carried as data).
    pub observation_contract: Json,
    /// The compiled surface — the declared `ToolSurface` + its `SurfaceArgMap`.
    pub surface: Option<crate::equiv::SurfaceBinding>,
}

/// `PolicyTables` (§3.2.4) — derived only from `Permission`/`EffectClass`/`Budget`
/// entities; **never** from surfaces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolicyTables {
    /// One row per `Permission` node: holder + grants + issuer authority.
    pub permissions: Vec<PermissionRow>,
    /// One row per `(ToolCapability, EffectClass)` — the declared effect table.
    pub effect_classes: Vec<EffectRow>,
    /// One row per `Budget` node — the budget containment table.
    pub budgets: Vec<BudgetRow>,
}

/// A permission row.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRow {
    /// The permission node.
    pub permission: PinnedRef,
    /// The holder.
    pub holder: PinnedRef,
    /// The grants (canonical `Grant` JSONs).
    pub grants: Vec<Json>,
    /// The issuer authority class name.
    pub issuer_authority: String,
}

/// An effect-table row: `capability → declared EffectClass`.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    /// The capability's semantic id.
    pub capability: String,
    /// The declared class.
    pub effect_class: hh_hir::EffectClass,
}

/// A budget row.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetRow {
    /// The budget node.
    pub budget: PinnedRef,
    /// The scope pattern.
    pub scope: String,
    /// The parent budget's semantic id.
    pub parent: Option<String>,
    /// The dimension bounds.
    pub dimensions: BTreeMap<String, DimensionBound>,
}

/// The budget envelope (§3.2.4 `budget`): the process budget, the verbatim control
/// boundary (the β record — `hh_ontology` owns the type, CC7), and the plan-node ids of
/// the stop-rule nodes the boundary lowered into (ADR-0106).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetEnvelope {
    /// The bound process budget.
    pub budget: PinnedRef,
    /// The budget's dimensions.
    pub dimensions: BTreeMap<String, DimensionBound>,
    /// β — the verbatim boundary record.
    pub control_boundary: ControlBoundary,
    /// The `stop-rule` plan-node ids this envelope produced.
    pub stop_rule_nodes: Vec<String>,
}

/// A bound slot the plan carries — `{slot, variant(pinned), params, enabled,
/// declared_on}`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundSlot {
    /// The slot name.
    pub slot: String,
    /// The class the slot binds.
    pub class_id: String,
    /// The bound variant tag.
    pub variant_id: String,
    /// The pinned variant version.
    pub version_id: String,
    /// The resolved params.
    pub params: BTreeMap<String, Json>,
    /// The ablation switch (a disabled binding still counts toward identity — §3.3.2).
    pub enabled: bool,
    /// The `AgentProcess` node's semantic id that declares this binding (traceability —
    /// `slots/<name>` entries trace to it).
    pub declared_on: String,
}

/// `ValidatorBinding` — a validator the plan binds (§3.2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatorBinding {
    /// The validator node.
    pub validator: PinnedRef,
    /// The kind tag (`executable|schema|predicate|judge|human`).
    pub kind: String,
    /// `deterministic` (a judge is never deterministic — §3.1.4).
    pub deterministic: bool,
    /// The inputs the validator reads.
    pub inputs: Vec<PinnedRef>,
    /// The `HarnessRule.rule_id`s whose `require_validator` action bound this validator.
    pub required_by: Vec<String>,
    /// `true` when the validator carries its assumption-debt record.
    pub carries_debt: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// lower_native (stage 2)
// ─────────────────────────────────────────────────────────────────────────────

struct Lowering {
    doc: HirDocument,
    next: usize,
    /// validator semantic_id → rule_ids requiring it.
    required_by: BTreeMap<String, Vec<String>>,
}

impl Lowering {
    fn fresh_id(&mut self) -> String {
        let id = format!("n{}", self.next);
        self.next += 1;
        id
    }

    fn node_ref(&self, r: &Ref, node_id_of: &str) -> Result<PinnedRef, CompileError> {
        pinned_ref(r).ok_or_else(|| CompileError::PlanError {
            node: node_id_of.to_string(),
            detail: format!("ref to {} is not pinned post-link", r.semantic_id),
        })
    }

    fn lower_step(&mut self, step: &ProcedureStep, proc: &Node) -> Result<PlanNode, CompileError> {
        let src = || (proc.semantic_id(), proc.version_id());
        let node_id = self.fresh_id();
        let (hir_node_id, hir_version_id) = src();
        let payload = match step {
            ProcedureStep::Instruction(t) => PlanNodePayload::Step(StepNode {
                mode: StepMode::Sequential,
                output_schema: None,
                action: StepAction::Instruction {
                    content_hash: t.content_hash.clone(),
                    owner: t.owner.clone(),
                },
            }),
            ProcedureStep::Invoke { tool, args } => PlanNodePayload::Step(StepNode {
                mode: StepMode::Sequential,
                output_schema: None,
                action: StepAction::Invoke {
                    capability: self.node_ref(tool, &hir_node_id)?,
                    args: args.clone(),
                },
            }),
            ProcedureStep::Delegate {
                spec,
                budget,
                permission,
            } => PlanNodePayload::Delegate(DelegateNode {
                spec: spec.clone(),
                budget: self.node_ref(budget, &hir_node_id)?,
                permission: self.node_ref(permission, &hir_node_id)?,
            }),
            ProcedureStep::Loop { bound, body } => {
                let body = self.lower_steps(body, proc)?;
                PlanNodePayload::Loop(LoopNode {
                    bound_budget: self.node_ref(bound, &hir_node_id)?,
                    replan_on: ReplanOn::Never,
                    body,
                })
            }
            ProcedureStep::Verify { validator } => {
                // A bare `Verify` lowers to the gate form of `branch-on-validator`:
                // empty bodies — the verdict gates the sequence (§3.2.4 closed set).
                PlanNodePayload::BranchOnValidator(BranchOnValidatorNode {
                    validator: self.node_ref(validator, &hir_node_id)?,
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                })
            }
            ProcedureStep::Branch {
                condition,
                then_body,
                else_body,
            } => {
                // The Stage-1 admitted branch-condition grammar: `{"validator":
                // "<semantic_id>"}` — the only conditional the plan knows is
                // `branch-on-validator`. Any other condition shape is
                // `PlanError{unsupported_construct}` (ADR-0019 D3; recorded in the
                // S1.10 ADR — a richer grammar is a dialect bump).
                let sid = condition
                    .get("validator")
                    .and_then(Json::as_str)
                    .ok_or_else(|| CompileError::PlanError {
                        node: hir_node_id.clone(),
                        detail: format!(
                            "Branch.condition {condition:?} is outside the admitted `{{\"validator\": …}}` grammar"
                        ),
                    })?;
                let target = self.doc.node(sid).ok_or_else(|| CompileError::PlanError {
                    node: hir_node_id.clone(),
                    detail: format!("branch validator {sid} is not a node in the document"),
                })?;
                let validator = PinnedRef {
                    semantic_id: target.semantic_id(),
                    version_id: target.version_id(),
                };
                PlanNodePayload::BranchOnValidator(BranchOnValidatorNode {
                    validator,
                    then_body: self.lower_steps(then_body, proc)?,
                    else_body: self.lower_steps(else_body, proc)?,
                })
            }
            ProcedureStep::Opaque(p) => {
                return Err(CompileError::PlanError {
                    node: hir_node_id,
                    detail: format!(
                        "Opaque step (CompiledPayload {}) has no RuntimePlan/1 lowering — a variant contract executes it",
                        p.format_tag
                    ),
                })
            }
        };
        Ok(PlanNode {
            node_id,
            hir_node_id,
            hir_version_id,
            payload,
        })
    }

    fn lower_steps(
        &mut self,
        steps: &[ProcedureStep],
        proc: &Node,
    ) -> Result<Vec<PlanNode>, CompileError> {
        steps.iter().map(|s| self.lower_step(s, proc)).collect()
    }
}

/// `lower_native(linked) → RuntimePlan/1` (§3.2.2 stage 2): the closed node set only —
/// an unsupported construct is `PlanError{unsupported_construct}`, never a silent
/// fallback.
pub fn lower_native(linked: &LinkedGraph) -> Result<RuntimePlan, CompileError> {
    let doc = &linked.sealed.document;
    let mut lw = Lowering {
        doc: doc.clone(),
        next: 0,
        required_by: BTreeMap::new(),
    };
    // `require_validator` rules → which validators a rule demands.
    for node in &doc.nodes {
        if let KindRecord::HarnessRule(r) = &node.semantic {
            if let RuleAction::RequireValidator(v) = &r.action {
                lw.required_by
                    .entry(v.semantic_id.clone())
                    .or_default()
                    .push(r.rule_id.clone());
            }
        }
    }

    let mut control: Vec<PlanNode> = Vec::new();
    let mut tools: Vec<ToolBinding> = Vec::new();
    let mut policies = PolicyTables::default();
    let mut validators: Vec<ValidatorBinding> = Vec::new();
    let mut artifact_ids: BTreeMap<String, String> = BTreeMap::new();
    let mut budget_envelope: Option<BudgetEnvelope> = None;
    let mut bound_slots: BTreeMap<String, BoundSlot> = BTreeMap::new();

    for node in &doc.nodes {
        match &node.semantic {
            KindRecord::Procedure(p) => {
                control.extend(lw.lower_steps(&p.steps, node)?);
                let _ = p as &ProcedureRecord;
            }
            KindRecord::ToolCapability(t) => {
                let effects: Vec<hh_hir::EffectClass> = match &t.effects {
                    ToolEffects::Pure => Vec::new(),
                    ToolEffects::Declared(set) => set.iter().cloned().collect(),
                };
                for e in &effects {
                    policies.effect_classes.push(EffectRow {
                        capability: node.semantic_id(),
                        effect_class: e.clone(),
                    });
                }
                let surface = match &node.surface {
                    Some(hh_hir::SurfaceRecord::Tool(ts)) => Some(crate::equiv::bind_surface(
                        node,
                        ts.as_ref(),
                        &t.input_schema,
                    )),
                    // A non-`Tool` surface record on a `ToolCapability` is a
                    // composite/synthesized surface — admissible only as a `PlanMap`
                    // (§3.2.6 rule iii; ADR-0090 §6). Unexpressible at C0:
                    // `UncheckableSurface`, never a silent drop.
                    Some(other) => {
                        let kind = match other {
                            hh_hir::SurfaceRecord::Validator(_) => "Validator",
                            hh_hir::SurfaceRecord::Procedure(_) => "Procedure",
                            hh_hir::SurfaceRecord::ContextItem(_) => "ContextItem",
                            hh_hir::SurfaceRecord::Tool(_) => unreachable!("handled above"),
                        };
                        return Err(CompileError::UncheckableSurface {
                            surface: node.semantic_id(),
                            reason: format!(
                                "a {kind} surface record on a ToolCapability is a composite/synthesized surface — expressible only as a PlanMap (ADR-0090 §6); unexpressible at C0"
                            ),
                        });
                    }
                    None => None,
                };
                tools.push(ToolBinding {
                    capability: PinnedRef {
                        semantic_id: node.semantic_id(),
                        version_id: node.version_id(),
                    },
                    effects,
                    preconditions: t
                        .preconditions
                        .iter()
                        .map(|p| p.name().to_string())
                        .collect(),
                    scope_bindings: crate::schema::scope_bindings_json(&t.scope_bindings),
                    observation_contract: t.observation_contract.clone(),
                    surface,
                });
            }
            KindRecord::Permission(p) => {
                let holder = pinned_ref(&p.holder).ok_or_else(|| CompileError::PlanError {
                    node: node.semantic_id(),
                    detail: "permission holder ref unpinned post-link".to_string(),
                })?;
                policies.permissions.push(PermissionRow {
                    permission: PinnedRef {
                        semantic_id: node.semantic_id(),
                        version_id: node.version_id(),
                    },
                    holder,
                    grants: p.grants.iter().map(crate::schema::grant_json).collect(),
                    issuer_authority: p.issuer.authority.as_str().to_string(),
                });
            }
            KindRecord::Budget(b) => {
                policies.budgets.push(BudgetRow {
                    budget: PinnedRef {
                        semantic_id: node.semantic_id(),
                        version_id: node.version_id(),
                    },
                    scope: b.scope.clone(),
                    parent: b.parent.as_ref().map(|r| r.semantic_id.clone()),
                    dimensions: b.dimensions.clone(),
                });
                let _ = b as &BudgetRecord;
            }
            KindRecord::Validator(v) => {
                validators.push(ValidatorBinding {
                    validator: PinnedRef {
                        semantic_id: node.semantic_id(),
                        version_id: node.version_id(),
                    },
                    kind: validator_kind_name(&v.kind).to_string(),
                    deterministic: v.deterministic,
                    inputs: v
                        .inputs
                        .iter()
                        .map(|r| {
                            pinned_ref(r).ok_or_else(|| CompileError::PlanError {
                                node: node.semantic_id(),
                                detail: "validator input ref unpinned post-link".to_string(),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    required_by: lw
                        .required_by
                        .get(&node.semantic_id())
                        .cloned()
                        .unwrap_or_default(),
                    carries_debt: v.assumption_debt.is_some(),
                });
            }
            KindRecord::Artifact(a) => {
                artifact_ids.insert(node.semantic_id(), a.content_hash.clone());
            }
            KindRecord::AgentProcess(ap) => {
                if let hh_hir::AgentProcessBody::Native(n) = &ap.body {
                    lower_native_process(
                        &mut lw,
                        node,
                        n,
                        &mut control,
                        &mut budget_envelope,
                        &mut bound_slots,
                    )?;
                }
            }
            _ => {}
        }
    }

    Ok(RuntimePlan {
        control,
        tools,
        policies,
        budget: budget_envelope,
        context_policy: bound_slots.get("context_policy").cloned(),
        bound_slots,
        validators,
        ids: PlanIds {
            definition_ref: linked.sealed.definition_ref.clone(),
            artifact_ids,
        },
    })
}

fn lower_native_process(
    lw: &mut Lowering,
    node: &Node,
    n: &NativeProcess,
    control: &mut Vec<PlanNode>,
    budget_envelope: &mut Option<BudgetEnvelope>,
    bound_slots: &mut BTreeMap<String, BoundSlot>,
) -> Result<(), CompileError> {
    // Bound slots → the plan's slot table.
    for (slot, bindings) in &n.slots {
        let one: Vec<&SlotBinding> = match bindings {
            hh_hir::records::SlotBindings::One(b) => vec![b],
            hh_hir::records::SlotBindings::Many(bs) => bs.iter().collect(),
        };
        for (i, b) in one.iter().enumerate() {
            let version_id = match &b.variant.version {
                RefVersion::Pinned(v) => v.clone(),
                RefVersion::Selector(_) => {
                    return Err(CompileError::PlanError {
                        node: node.semantic_id(),
                        detail: format!("slot {slot} variant unpinned post-link"),
                    })
                }
            };
            let key = if i == 0 {
                slot.clone()
            } else {
                format!("{slot}[{i}]")
            };
            bound_slots.insert(
                key,
                BoundSlot {
                    slot: slot.clone(),
                    class_id: b.variant.class_id.clone(),
                    variant_id: b.variant.variant_id.clone(),
                    version_id,
                    params: b.params.clone(),
                    enabled: b.enabled,
                    declared_on: node.semantic_id(),
                },
            );
        }
    }
    // The boundary lowers into the budget envelope + stop-rule nodes (ADR-0106): one
    // `stop-rule{reason: budget_exhausted}` per bound process budget, plus a
    // `stop-rule{reason: cancelled}` and `stop-rule{reason: context_exhausted}` for the
    // envelope-reserved `Stop` point the boundary's guard spells out (when present).
    if let Some(budget_ref) = pinned_ref(&n.budget) {
        let doc = &lw.doc;
        let dims = doc
            .node(&budget_ref.semantic_id)
            .and_then(|bn| match &bn.semantic {
                KindRecord::Budget(b) => Some(b.dimensions.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let mut stop_nodes: Vec<String> = Vec::new();
        let mut mk = |lw: &mut Lowering, reason: StopKind, condition: Option<Json>| {
            let id = lw.fresh_id();
            stop_nodes.push(id.clone());
            PlanNode {
                node_id: id,
                hir_node_id: node.semantic_id(),
                hir_version_id: node.version_id(),
                payload: PlanNodePayload::StopRule(StopRuleNode {
                    reason,
                    bound: Some(budget_ref.clone()),
                    condition,
                }),
            }
        };
        control.push(mk(lw, StopKind::BudgetExhausted, None));
        use hh_ontology::control::DecisionPoint;
        if let Some(guard) = n.control_boundary.guards.get(&DecisionPoint::Stop) {
            control.push(mk(
                lw,
                StopKind::Cancelled,
                Some(Json::obj([("guard", Json::str(guard.clone()))])),
            ));
        }
        *budget_envelope = Some(BudgetEnvelope {
            budget: budget_ref,
            dimensions: dims,
            control_boundary: n.control_boundary.clone(),
            stop_rule_nodes: stop_nodes,
        });
    }
    Ok(())
}

fn validator_kind_name(k: &hh_hir::ValidatorKind) -> &'static str {
    match k {
        hh_hir::ValidatorKind::Executable(_) => "executable",
        hh_hir::ValidatorKind::Schema => "schema",
        hh_hir::ValidatorKind::Predicate => "predicate",
        hh_hir::ValidatorKind::Judge(_) => "judge",
        hh_hir::ValidatorKind::Human => "human",
    }
}
