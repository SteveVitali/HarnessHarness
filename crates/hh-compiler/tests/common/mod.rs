//! Shared fixtures for the `hh-compiler` suite (spec §3.2; ticket S1.10; R-2.1.3).
//!
//! The fixtures reuse the S1.9 shapes: a minimal valid document
//! (`{rule, budget, perm, agent}` + the Stage-1 assembly), a seeded registry (the two
//! mandatory classes + one in-process variant each), and `resolve` → `SealedDefinition`
//! (the compiler's only input). Profiles are `ModelProfile/1` records built in memory —
//! the bound view is a map, never a live registry (CF-046/047).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_assembly::catalog::ClassCatalog;
use hh_assembly::grammar::Assembly;
use hh_hir::document::{HirDocument, Node, SealedDefinition};
use hh_hir::kinds::{EntityKind, ToolEffects, ValidatorKind};
use hh_hir::leaves::Text;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::names::ResolveMode;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::kinds::{Cardinality, Placement};
use hh_registry::records::{
    AppliesTo, ClassRecord, ContractOperation, Implementation, RegistryRecord, VariantRecord,
};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

use hh_compiler::link::{TargetSpec, VariantView};
use hh_compiler::profile::{
    DebtStatus, ExpiryCondition, ExpiryKind, ModelProfile, ModelRole, ProfileCompatibility,
    ProfileDebtRecord, ProfileRule, ProfileRuleKind, ProfileSelector, ProfileView, VersionPattern,
};

// ── provenance / leaves ───────────────────────────────────────────────────────

pub fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("compiler.test", 0)
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

/// An unresolved ref (selector coordinate — doc-internal; `seal` pins it).
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

// ── document fixtures ─────────────────────────────────────────────────────────

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

/// A conditioned HarnessRule — `conditioned_on` set, `assumption_debt` as supplied.
#[allow(dead_code)]
pub fn conditioned_rule_node(
    id: &str,
    conditioned_on: ProfileRef,
    debt: Option<AssumptionDebtRecord>,
    seq: u64,
) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: Some(conditioned_on),
                assumption_debt: debt,
            }),
            seq,
        ),
        id,
    )
}

/// A complete §3.1 `AssumptionDebtRecord`.
#[allow(dead_code)]
pub fn complete_debt(rule_id: &str, status: hh_hir::DebtStatus, seq: u64) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: text("the assumption holds while the model family does", seq),
        evidence_refs: vec![hh_hir::EvidenceRef::legacy("sha256:ev")],
        owner: hh_hir::OwnerRef::principal("test:owner"),
        expiry_condition: hh_hir::ExpiryCondition {
            kind: hh_hir::ExpiryKind::Date,
            value: Some("2099-01-01".to_string()),
        },
        removal_test_ref: "sha256:test".to_string(),
        status,
        // `AssumptionDebtRecord/1` per-home completeness for the `harness_rule`
        // home (DebtHomes/1 id 1: `debt_class` + `scope.model_selectors` —
        // `needs_model_scope` — are required fields there; the seal-time
        // `validate_for_home` battery enforces them).
        debt_class: Some(hh_hir::DebtClass::Hypothesized),
        hypothesis_typed: None,
        scope: Some(hh_hir::DebtScope {
            model_selectors: vec![hh_hir::ModelSelector::Exact {
                model_id: "test:model".to_string(),
            }],
            ..Default::default()
        }),
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: Some(hh_hir::RemovalTest {
            kind: hh_hir::RemovalTestKind::Inspection,
            criteria: Some("human inspection".into()),
            ..hh_hir::RemovalTest::new(hh_hir::RemovalTestKind::Inspection)
        }),
        created_by: None,
        created_at: None,
        supersedes: None,
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

/// The root `AgentProcess` — a `native` body (slots materialise at resolve).
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
                    // The profile the definition pins — `test_profile()`'s
                    // `profile_id` matches so the bound chain head satisfies it.
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

pub fn tool_node(id: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(ToolCapabilityRecord {
                purpose: text("a test tool", seq),
                input_schema: Json::obj([(
                    "properties",
                    Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
                )]),
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

/// A tool carrying a `Tool` surface (the compiled-surface path).
pub fn surfaced_tool_node(id: &str, name: &str, args: &[&str], seq: u64) -> Node {
    let mut n = tool_node(id, seq);
    n.surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: name.into(),
        namespace: "test".into(),
        description_template: text("a surfaced tool", seq),
        argument_order: args.iter().map(|a| a.to_string()).collect(),
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

/// A `Validator` node (the `branch-on-validator` target / `Verify` operand).
pub fn validator_node(id: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Validator,
            KindRecord::Validator(ValidatorRecord {
                kind: ValidatorKind::Schema,
                inputs: vec![],
                verdict_type: "boolean".to_string(),
                deterministic: true,
                cost: Json::Null,
                evidence_out: Json::Null,
                assumption_debt: None,
            }),
            seq,
        ),
        id,
    )
}

/// A `Procedure` node with the given steps.
pub fn procedure_node(id: &str, steps: Vec<ProcedureStep>, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Procedure,
            KindRecord::Procedure(ProcedureRecord {
                preconditions: Json::Null,
                steps,
                expected_evidence: Json::Null,
                allowed_capabilities: vec![],
                failure_handlers: Json::Null,
            }),
            seq,
        ),
        id,
    )
}

/// The minimal valid document: `{rule, budget, perm, agent}` + the assembly section.
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
    let d = std::env::temp_dir().join(format!("hh-compiler-test-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A `ClassRecord` carrying the §3.3.3 invariants.
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
        depends_on: Vec::new(),
    }
}

/// A `VariantRecord` of `class_ref` named `variant_id` (`<ns>/<name>`), placed
/// in-process by default.
pub fn variant_record(
    class_ref: &str,
    variant_id: &str,
    placement: Placement,
    conditioned_rules: Vec<(String, AssumptionDebtRecord)>,
    seq: u64,
) -> VariantRecord {
    VariantRecord {
        variant_id: variant_id.to_string(),
        class_ref: class_ref.to_string(),
        contract_range: "1.0".to_string(),
        version_label: Some("1.0.0".to_string()),
        param_schema: BTreeMap::new(),
        implementation: Implementation {
            content: hh_identity::idp::address(
                format!("impl-{variant_id}").as_bytes(),
                "application/octet-stream",
            ),
            placement,
            host_requirements: Json::Null,
        },
        capability_declaration: BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]),
        conditioned_rules,
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: text(&format!("v {variant_id}"), seq),
        dialect_range: "registry/1".to_string(),
    }
}

/// A seeded store: the two Stage-1 classes plus one in-process variant per class,
/// published under `hh/<name>`. Returns `(store, {slot → variant_version_id})`.
#[allow(dead_code)] // `unit.rs` uses `sealed_doc`; `acceptance.rs` may need the raw store.
pub fn seeded_store(tag: &str) -> (RegistryStore, BTreeMap<String, String>) {
    seeded_store_with(tag, vec![], vec![])
}

/// A seeded store with extra conditioned rules on named variants
/// (`(slot, [(rule_id, debt)])`) and extra variants to register.
#[allow(clippy::type_complexity)]
pub fn seeded_store_with(
    tag: &str,
    conditioned: Vec<(&str, Vec<(String, AssumptionDebtRecord)>)>,
    extra: Vec<(RegistryRecord, &str, &str)>,
) -> (RegistryStore, BTreeMap<String, String>) {
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
        let rules = conditioned
            .iter()
            .find(|(slot, _)| *slot == class_id)
            .map(|(_, rs)| rs.clone())
            .unwrap_or_default();
        let v = s
            .register(
                RegistryRecord::Variant(variant_record(
                    &c.version_id,
                    &format!("hh/{name}"),
                    Placement::InProcess,
                    rules,
                    0,
                )),
                &kernel(),
                None,
            )
            .unwrap();
        s.publish("hh", name, &v.version_id, None, None, &kernel())
            .unwrap();
        vids.insert(class_id.to_string(), v.version_id);
    }
    for (rec, ns, name) in extra {
        let v = s.register(rec, &kernel(), None).unwrap();
        s.publish(ns, name, &v.version_id, None, None, &kernel())
            .unwrap();
    }
    (s, vids)
}

// ── the sealed-definition seam ────────────────────────────────────────────────

pub fn resolve_env<'a>(
    store: &'a mut RegistryStore,
    catalog: &'a dyn ClassCatalog,
    mode: ResolveMode,
) -> hh_assembly::resolve::ResolveEnv<'a> {
    hh_assembly::resolve::ResolveEnv {
        registry: store,
        catalog,
        snapshot_id: None,
        mode,
        registrar: kernel(),
        resolved_at: 7,
    }
}

/// `resolve(doc_with(stage1_assembly()))` — the compiler's input, sealed.
/// Returns `(store, sealed, {slot → variant_version_id})`.
pub fn sealed_doc(tag: &str) -> (RegistryStore, SealedDefinition, BTreeMap<String, String>) {
    sealed_doc_with(tag, doc_with(&stage1_assembly()), vec![], vec![])
}

/// `resolve` over a caller-shaped document, with the seeded store's extras.
#[allow(clippy::type_complexity)]
pub fn sealed_doc_with(
    tag: &str,
    doc: HirDocument,
    conditioned: Vec<(&str, Vec<(String, AssumptionDebtRecord)>)>,
    extra: Vec<(RegistryRecord, &str, &str)>,
) -> (RegistryStore, SealedDefinition, BTreeMap<String, String>) {
    let (mut store, vids) = seeded_store_with(tag, conditioned, extra);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    (store, sealed, vids)
}

// ── profile fixtures ──────────────────────────────────────────────────────────

/// A complete, dated profile-debt record (the §3.2.8 field shape).
pub fn profile_debt(rule_id: &str, status: DebtStatus) -> ProfileDebtRecord {
    ProfileDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: "the model honours the declared contract".to_string(),
        evidence_refs: vec![hh_hir::EvidenceRef::legacy("sha256:ev")],
        owner: "test:owner".to_string(),
        reach_via: Vec::new(),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::Date,
            value: Some("2099-01-01".to_string()),
        },
        removal_test_ref: "sha256:test".to_string(),
        removal_test: Some(hh_hir::RemovalTest {
            kind: hh_hir::RemovalTestKind::Inspection,
            criteria: Some("human inspection".into()),
            ..hh_hir::RemovalTest::new(hh_hir::RemovalTestKind::Inspection)
        }),
        status,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        created_at: None,
        supersedes: None,
    }
}

/// The minimal admissible `ModelProfile/1` — `profile_id = "sha256:profile"` matches the
/// definition's pinned profile ref; complete dated expiry debt; empty rules; computed
/// `content_hash`.
pub fn test_profile() -> ModelProfile {
    profile_with("sha256:profile", "1.0", vec![])
}

/// A `ModelProfile/1` with the given identity, version and rules (content_hash computed).
pub fn profile_with(id: &str, version: &str, rules: Vec<ProfileRule>) -> ModelProfile {
    let mut p = ModelProfile {
        profile_id: id.to_string(),
        version: version.to_string(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "test-api".to_string(),
            model_family: "test-model".to_string(),
            version_pattern: VersionPattern::Any,
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: Default::default(),
        rules,
        ext: BTreeMap::new(),
        expiry: profile_debt(&format!("{id}.expiry"), DebtStatus::Active),
        compatibility: ProfileCompatibility {
            inventory_version: "1.0".to_string(),
            min_compiler_version: "0.0.0".to_string(),
        },
        tests: Json::obj([]),
    };
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    p
}

/// A `ProfileRule` — `kind`, `owned_fields`, `params`, debt as supplied.
pub fn rule(
    rule_id: &str,
    kind: ProfileRuleKind,
    owned_fields: &[&str],
    params: Json,
    status: DebtStatus,
) -> ProfileRule {
    ProfileRule {
        rule_id: rule_id.to_string(),
        kind,
        owned_fields: owned_fields.iter().map(|f| f.to_string()).collect(),
        params,
        debt: profile_debt(rule_id, status),
        scope: None,
        supersedes: None,
        compliance: hh_compiler::profile::Compliance {
            detector_class: hh_compiler::profile::ComplianceDetector::None,
            followed_predicate_ref: None,
        },
    }
}

/// A `ProfileView` over a map (coordinate or content-hash lookup — the bound view).
pub struct MapProfileView(pub BTreeMap<String, ModelProfile>);

impl MapProfileView {
    pub fn of(profiles: Vec<ModelProfile>) -> MapProfileView {
        let mut m = BTreeMap::new();
        for p in profiles {
            m.insert(hh_compiler::profile::profile_coordinate(&p), p.clone());
            m.insert(p.content_hash.clone(), p);
        }
        MapProfileView(m)
    }
}

impl ProfileView for MapProfileView {
    fn profile(&self, coordinate: &str) -> Option<ModelProfile> {
        self.0.get(coordinate).cloned()
    }
}

/// A `VariantView` that records every requested `version_id` — the zero-resolutions
/// witness (DF-S1.9-3): link may only ever *look up* pinned version ids.
#[allow(dead_code)]
pub struct CountingVariantView<'a> {
    pub inner: &'a dyn VariantView,
    pub requested: RefCell<Vec<String>>,
}

impl<'a> VariantView for CountingVariantView<'a> {
    fn variant(&self, version_id: &str) -> Option<VariantRecord> {
        self.requested.borrow_mut().push(version_id.to_string());
        self.inner.variant(version_id)
    }
}

/// A registered target spec (`mcp` is in the Stage-1 table).
pub fn mcp_target() -> TargetSpec {
    TargetSpec {
        target_id: "mcp".to_string(),
        spec_version: "1.0".to_string(),
        content_hash: "sha256:target-mcp".to_string(),
    }
}
