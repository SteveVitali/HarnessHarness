//! Shared HIR/1 fixtures for the integration suite.
//!
//! Semantic ids are **author-assigned** here (`version.semantic_id = Some("test:<name>")`):
//! the `Permission.holder ↔ AgentProcess.permissions` record-ref cycle (CF-076) has no
//! content-hash fixpoint, so fixture documents pin their ids explicitly — exactly what an
//! authored pre-seal document does before `identity`/`seal` runs. `seal` keeps stored sids.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::document::{Edge, HirDocument, Node};
use hh_hir::kinds::{EdgeKind, EffectClass, EntityKind, ToolEffects};
use hh_hir::leaves::Text;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

/// Minted human-authored provenance at definition scope (minted = at ceiling, validates).
pub fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

/// A kernel-origin record.
pub fn prov_kernel(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::kernel("test:kernel"),
        PersistenceScope::Definition,
        seq,
    )
}

pub fn text(s: &str, seq: u64) -> Text {
    Text::new(s, "test:owner", prov(seq))
}

/// An unresolved ref (selector coordinate).
pub fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

/// Stamp an authored semantic id on a node.
pub fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

pub fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

pub fn rule_node(id: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            seq,
        ),
        id,
    )
}

/// A conditioned rule (`conditioned_on` set) — `assumption_debt` must be complete.
pub fn conditioned_rule_node(id: &str, seq: u64, debt: Option<AssumptionDebtRecord>) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: Some(ProfileRef {
                    profile: "sha256:profile".into(),
                    pinned: true,
                }),
                assumption_debt: debt,
            }),
            seq,
        ),
        id,
    )
}

pub fn debt_record(rule_id: &str, seq: u64) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: rule_id.into(),
        hypothesis: text("debt hypothesis", seq),
        evidence_refs: vec![],
        owner: "test:owner".into(),
        expiry_condition: "superseded".into(),
        removal_test_ref: "sha256:test".into(),
        status: DebtStatus::Open,
    }
}

pub fn budget_node(id: &str, dims: &[(&str, u64)], seq: u64) -> Node {
    let dimensions = dims
        .iter()
        .map(|(k, h)| {
            (
                k.to_string(),
                DimensionBound {
                    hard: Some(*h),
                    soft: None,
                },
            )
        })
        .collect();
    sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions,
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            seq,
        ),
        id,
    )
}

pub fn child_budget_node(id: &str, parent: &str, dims: &[(&str, u64)], seq: u64) -> Node {
    let mut n = budget_node(id, dims, seq);
    if let KindRecord::Budget(b) = &mut n.semantic {
        b.parent = Some(sel(parent));
    }
    n
}

/// A Permission held by `holder` (an AgentProcess sid).
pub fn perm_node(id: &str, holder: &str, grants: Vec<Grant>, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel(holder),
                grants,
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            seq,
        ),
        id,
    )
}

pub fn grant(domain: &str, scope: &str, delegable: bool) -> Grant {
    Grant {
        effect: EffectClass::domain_only(
            hh_hir::kinds::EffectDomain::parse(domain).expect("effect domain"),
        ),
        scope: scope.into(),
        constraints: GrantConstraints::default(),
        delegable,
    }
}

/// The root `AgentProcess` — a `native` body self-referencing `harness_def`.
pub fn agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
    let mut slots = BTreeMap::new();
    for name in ["control_strategy", "context_policy"] {
        slots.insert(
            name.to_string(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::pinned(
                name,
                format!("hh/{name}/test"),
                format!("sha256:variant-{name}"),
            ))),
        );
    }
    sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel(id),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel(budget),
                    permissions: sel(perm),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            seq,
        ),
        id,
    )
}

/// A `hosted` `AgentProcess` (an `OpaqueProcess` body).
pub fn hosted_agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Hosted(OpaqueProcess {
                    participant_ref: "participant:test".into(),
                    declared_capabilities: CapabilityDeclarationRecord::all_unknown(),
                    observability_levels: BTreeSet::new(),
                    participant_version: "sha256:pv".into(),
                    version_identity: None,
                    hosting_mechanism: hh_ontology::participant::HostingMechanism::SessionAbi,
                    supplies: Supplies::default(),
                    budget: sel(budget),
                    permissions: sel(perm),
                    params: None,
                }),
            }),
            seq,
        ),
        id,
    )
}

pub fn goal_node(id: &str, budget: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Goal,
            KindRecord::Goal(GoalRecord {
                statement: text("do the thing", seq),
                success_criteria: vec![],
                unverifiable_reason: Some(text("not checkable", seq)),
                budget: sel(budget),
                origin: GoalOrigin::Human,
                parent: None,
            }),
            seq,
        ),
        id,
    )
}

pub fn tool_node(id: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(ToolCapabilityRecord {
                purpose: text("a test tool", seq),
                input_schema: Json::Null,
                output_schema: None,
                effects: ToolEffects::Pure,
                preconditions: vec![],
                scope_bindings: ScopeBindings::Unknown,
                resources: Resources::NoneDeclared,
                observation_contract: Json::Null,
                cost_model: None,
                execution_requirement: Json::Null,
                source: Json::Null,
                exposure_hint: Json::Null,
                postconditions: vec![],
                flow_contract: None,
            }),
            seq,
        ),
        id,
    )
}

/// The minimal valid document: `{rule, budget, perm, agent}` DAG-shaped except the
/// `holder ↔ permissions` cycle — refs resolve through the authored sids.
pub fn valid_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node("test:rule", 1));
    doc.nodes
        .push(budget_node("test:budget", &[("tokens.blended", 1000)], 2));
    doc.nodes
        .push(perm_node("test:perm", "test:agent", vec![], 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

/// A document whose root AgentProcess is `hosted` (an OpaqueProcess body).
pub fn hosted_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node("test:rule", 1));
    doc.nodes
        .push(budget_node("test:budget", &[("tokens.blended", 1000)], 2));
    doc.nodes
        .push(perm_node("test:perm", "test:agent", vec![], 3));
    doc.nodes.push(hosted_agent_node(
        "test:agent",
        "test:budget",
        "test:perm",
        4,
    ));
    doc
}

/// `depends-on` edge between two sids.
pub fn dep_edge(from: &str, to: &str, seq: u64) -> Edge {
    Edge::new(
        EdgeKind::DependsOn,
        from,
        to,
        EdgeRecord::DependsOn {
            reason: "test".into(),
        },
        prov(seq),
    )
}
