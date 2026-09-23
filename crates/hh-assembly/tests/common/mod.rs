//! Shared fixtures for the `hh-assembly` acceptance suite (spec §3.3; ticket S1.9).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_assembly::grammar::Assembly;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::{EntityKind, ToolEffects};
use hh_hir::leaves::Text;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::idp::address;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::kinds::{Cardinality, Placement};
use hh_registry::records::{
    AppliesTo, ClassRecord, ContractOperation, Implementation, RegistryRecord, VariantRecord,
};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

pub fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("assembly.test", 0)
}

/// Minted human-authored provenance at definition scope (minted = at ceiling).
pub fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
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

pub fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel(holder),
                grants: vec![],
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

/// The root `AgentProcess` — a `native` body; `slots` are authored `native.slots`
/// (assembly bindings materialise over them at resolve).
pub fn agent_node(
    id: &str,
    budget: &str,
    perm: &str,
    slots: BTreeMap<String, SlotBindings>,
    seq: u64,
) -> Node {
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

/// A `hosted` `AgentProcess` (an `OpaqueProcess` body — no slots).
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

/// A named tool (the `surface.name` `verify_resume` joins delivered events on).
pub fn named_tool_node(id: &str, name: &str, seq: u64) -> Node {
    let mut n = tool_node(id, seq);
    n.surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: name.into(),
        namespace: "test".into(),
        description_template: text("a named tool", seq),
        argument_order: vec![],
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: None,
        icon_ref: None,
    })));
    n
}

/// The minimal valid document: `{rule, budget, perm, agent}` with the assembly
/// section attached.
pub fn doc_with(assembly: &Assembly) -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node("test:rule", 1));
    doc.nodes
        .push(budget_node("test:budget", &[("tokens.blended", 1000)], 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes.push(agent_node(
        "test:agent",
        "test:budget",
        "test:perm",
        BTreeMap::new(),
        4,
    ));
    doc.assembly = Some(assembly.to_json());
    doc
}

/// The minimal valid document whose root is `hosted`.
pub fn hosted_doc_with(assembly: &Assembly) -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node("test:rule", 1));
    doc.nodes
        .push(budget_node("test:budget", &[("tokens.blended", 1000)], 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes.push(hosted_agent_node(
        "test:agent",
        "test:budget",
        "test:perm",
        4,
    ));
    doc.assembly = Some(assembly.to_json());
    doc
}

/// An assembly with both Stage-1 slots bound to selector variants
/// (`hh/round_robin`, `hh/full_window` at selector `latest`).
pub fn stage1_assembly() -> Assembly {
    let mut a = Assembly::empty();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    a.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    a
}

// ── registry fixtures ─────────────────────────────────────────────────────────

pub fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-assembly-test-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A `ClassRecord` carrying the §3.3.3 invariants (non-empty contract
/// invariants/failure-modes, the two mandatory required inputs).
pub fn class_record(id: &str) -> ClassRecord {
    ClassRecord {
        class_id: id.to_string(),
        contract: vec![ContractOperation {
            name: "run".to_string(),
            inputs: Json::Null,
            outputs: Json::Null,
            invariants: vec!["i".to_string()],
            failure_modes: vec!["refuse".to_string()],
        }],
        cardinality: Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            ("additionalProperties", Json::Bool(false)),
            ("properties", Json::obj([("deterministic", Json::Null)])),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec![],
        slot_key: id.to_string(),
        tier: "C0".to_string(),
    }
}

/// A `VariantRecord` of `class_ref` named `variant_id` (`<ns>/<name>`), placed
/// in-process by default.
pub fn variant_record(class_ref: &str, variant_id: &str, placement: Placement) -> VariantRecord {
    VariantRecord {
        variant_id: variant_id.to_string(),
        class_ref: class_ref.to_string(),
        contract_range: "1.0".to_string(),
        version_label: Some("1.0.0".to_string()),
        param_schema: BTreeMap::new(),
        implementation: Implementation {
            content: address(
                format!("impl-{variant_id}").as_bytes(),
                "application/octet-stream",
            ),
            placement,
            host_requirements: Json::Null,
        },
        capability_declaration: BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]),
        conditioned_rules: vec![],
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: text(&format!("v {variant_id}"), 0),
        dialect_range: "registry/1".to_string(),
    }
}

/// A seeded store: the two Stage-1 classes plus one in-process variant per
/// class, published under `hh/<name>` (`round_robin` / `full_window`).
/// Returns `(store, {slot → variant_version_id})`.
pub fn seeded_store(tag: &str) -> (RegistryStore, BTreeMap<String, String>) {
    let mut s = RegistryStore::open(dir(tag), &kernel()).unwrap();
    let mut vids = BTreeMap::new();
    for (class_id, name) in [
        ("control_strategy", "round_robin"),
        ("context_policy", "full_window"),
    ] {
        let c = s
            .register(
                RegistryRecord::Class(class_record(class_id)),
                &kernel(),
                None,
            )
            .unwrap();
        let v = s
            .register(
                RegistryRecord::Variant(variant_record(
                    &c.version_id,
                    &format!("hh/{name}"),
                    Placement::InProcess,
                )),
                &kernel(),
                None,
            )
            .unwrap();
        s.publish("hh", name, &v.version_id, None, None, &kernel())
            .unwrap();
        vids.insert(class_id.to_string(), v.version_id);
    }
    (s, vids)
}
