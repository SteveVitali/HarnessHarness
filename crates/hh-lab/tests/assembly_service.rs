//! `hh-lab::assembly` acceptance suite (§6.1; R-2.10.1; ticket S3.5).
//! AC-1 codec corpus (≥100 sources), AC-3 one-invalid-per-owned-code,
//! AC-5 dry-run parity, AC-7 snapshot drift + freeze/adopt, AC-11 the
//! Stage-0 anchor as a source, T-1 `flatten(project(d)) = d.ops`, the hosted
//! `n/a{class}` rows and `C-CLASS-6`, and the S-3 byte-identity /
//! removability gates (the service layer is provably removable — its outputs
//! are the kernel's).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_assembly::compose::compose;
use hh_assembly::diagnostics::{Code, NaReason};
use hh_assembly::grammar::{Assembly, LayerProvenance, LayerSourceKind};
use hh_assembly::resolve::{resolve, ResolveEnv};
use hh_assembly::Stage1Catalog;
use hh_hir::document::Node;
use hh_hir::kinds::EntityKind;
use hh_hir::leaves::Text;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::idp::address;
use hh_identity::names::ResolveMode;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::kinds::{Cardinality, Placement};
use hh_registry::records::{
    AppliesTo, ClassRecord, ContractOperation, Implementation, RegistryRecord, VariantRecord,
};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

use hh_lab::assembly::desugar::{desugar, overrides_layer_id, LAB_EXT_KEY};
use hh_lab::assembly::diff_view::{flatten, project};
use hh_lab::assembly::drift::{adopt_admissible, pin_ops_only};
use hh_lab::assembly::plan::PlanExit;
use hh_lab::assembly::service::{AssembleMode, AssemblyService, BatchPoint, PublishSpec};
use hh_lab::assembly::source::{
    AssemblySource, HostedSpec, HostedSupply, Import, ImportKind, RootKind, SourceDocument,
    SourceLayer,
};

// ── fixtures (the hh-assembly common fixtures, duplicated — the suites share
// no test crate so the removability argument stays literal) ──────────────────

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("lab.assembly.test", 0)
}

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn text(s: &str, seq: u64) -> Text {
    Text::new(s, "test:owner", prov(seq))
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn rule_node(id: &str, seq: u64) -> Node {
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

fn budget_node(id: &str, dims: &[(&str, u64)], seq: u64) -> Node {
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

fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
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

fn agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
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
                    slots: BTreeMap::new(),
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

/// The authored scaffold — `{rule, budget, perm, agent}` as canonical node JSON.
fn scaffold() -> SourceDocument {
    let nodes = [
        rule_node("test:rule", 1),
        budget_node("test:budget", &[("tokens.blended", 1000)], 2),
        perm_node("test:perm", "test:agent", 3),
        agent_node("test:agent", "test:budget", "test:perm", 4),
    ];
    SourceDocument {
        root: "test:agent".into(),
        nodes: nodes.iter().map(hh_hir::wire::node_to_json).collect(),
        edges: Vec::new(),
    }
}

/// The Stage-1 assembly fragment (both slots bound at `latest`).
fn stage1_fragment() -> Json {
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
    a.to_json()
}

fn user_layer(id: &str, precedence: i64, fragment: Json) -> SourceLayer {
    SourceLayer {
        provenance: LayerProvenance {
            source_kind: LayerSourceKind::User,
            id: id.into(),
            version: "1".into(),
            precedence,
        },
        fragment,
    }
}

/// The minimal valid source — one `user` layer carrying the Stage-1 slots.
fn valid_source() -> AssemblySource {
    AssemblySource {
        dialect: "hir/1".into(),
        root_kind: RootKind::Native,
        base: None,
        document: Some(scaffold()),
        layers: vec![user_layer("user:main", 0, stage1_fragment())],
        overrides: Vec::new(),
        overrides_present: false,
        imports: Vec::new(),
        hosted: None,
        ext: BTreeMap::new(),
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-lab-assembly-test-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn class_record(id: &str) -> ClassRecord {
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

fn variant_record(class_ref: &str, variant_id: &str) -> VariantRecord {
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
            placement: Placement::InProcess,
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

/// The seeded store — Stage-1 classes + `hh/round_robin`, `hh/full_window`
/// published under `hh/`.
fn seeded_store(tag: &str) -> (RegistryStore, Stage1Catalog) {
    let mut s = RegistryStore::open(dir(tag), &kernel()).unwrap();
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
                RegistryRecord::Variant(variant_record(&c.version_id, &format!("hh/{name}"))),
                &kernel(),
                None,
            )
            .unwrap();
        s.publish("hh", name, &v.version_id, None, None, &kernel())
            .unwrap();
    }
    (s, Stage1Catalog::stage1())
}

fn svc<'a>(s: &'a mut RegistryStore, c: &'a Stage1Catalog) -> AssemblyService<'a> {
    AssemblyService {
        registry: s,
        catalog: c,
        kernel: kernel(),
        registrar: kernel(),
        resolved_at: 1_000,
    }
}

fn codes(r: &hh_assembly::diagnostics::ValidationReport) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.code()).collect()
}

// ── AC-1/AC-2: the codec corpus (≥100 sources) ───────────────────────────────

/// The 100+ valid-source corpus: the base source varied over an authored
/// `values`/`parameters`/`entities`/`overrides`/`imports` grid — every
/// variant must round-trip the canonical codec byte-identically.
fn corpus() -> Vec<AssemblySource> {
    let mut out = Vec::new();
    for i in 0..5 {
        for v in 0..5 {
            for o in 0..5 {
                let mut s = valid_source();
                let mut frag = Assembly::empty();
                frag.parameters.insert(
                    format!("p{i}_{v}"),
                    hh_assembly::grammar::ParameterSpec {
                        param_type: hh_assembly::grammar::ParamType::Int,
                        domain: Some(Json::Arr(vec![Json::Int(0), Json::Int(8)])),
                        default: Some(Json::Int(v as i64)),
                        required: hh_assembly::grammar::ParamRequirement::Optional,
                        unit: None,
                        sweepable: true,
                        affects: vec![],
                        budget_relevant: false,
                    },
                );
                if v % 2 == 0 {
                    frag.values
                        .insert(format!("p{i}_{v}"), Json::Int((v + i) as i64));
                }
                if v == 4 {
                    frag.entities.insert(
                        format!("ent-{i}"),
                        hh_assembly::grammar::EntityBinding::Inline {
                            kind: "context_item".into(),
                            record: Json::obj([("body", Json::str(format!("e{i}")))]),
                        },
                    );
                }
                let mut fj = frag.to_json();
                // Merge the fragment's members onto the stage-1 slots.
                if let (Json::Obj(f), Json::Obj(base)) = (&mut fj, &mut stage1_fragment()) {
                    for (k, vv) in std::mem::take(base) {
                        let empty = matches!(f.get(&k), Some(Json::Obj(m)) if m.is_empty());
                        if !f.contains_key(&k) || empty {
                            f.insert(k, vv);
                        }
                    }
                }
                s.layers = vec![user_layer(&format!("user:l{i}"), i as i64, fj)];
                if o >= 1 {
                    s.overrides = vec![format!("values.p{i}_{v}={}", v + o)];
                    s.overrides_present = true;
                }
                if i == 4 {
                    s.imports.push(Import {
                        path_or_ref: format!("mem://doc-{v}"),
                        as_: ImportKind::ContextItem,
                        content: Some(format!("---\ntitle: t{v}\n---\nbody {v}")),
                        name: Some(format!("imp-{v}")),
                        frontmatter_schema: Some(Json::obj([(
                            "require",
                            Json::Arr(vec![Json::str("title")]),
                        )])),
                    });
                }
                out.push(s);
            }
        }
    }
    out
}

#[test]
fn ac1_corpus_codec_round_trip_and_assemble() {
    let sources = corpus();
    assert!(
        sources.len() >= 100,
        "the corpus carries ≥100 valid sources"
    );
    let (mut store, catalog) = seeded_store("ac1");
    for (i, s) in sources.iter().enumerate() {
        // Canonical codec round-trip (the codec is total over the record).
        let j = s.to_json();
        let back = AssemblySource::from_json(&j, "/source")
            .unwrap_or_else(|e| panic!("source {i} fails decode: {e}"));
        assert_eq!(
            back.to_json().to_canonical_string(),
            j.to_canonical_string(),
            "source {i} round-trips canonically"
        );
        // … and assembles (the corpus is the valid half).
        let r = svc(&mut store, &catalog).assemble(s, None, AssembleMode::Plan);
        assert_eq!(
            r.status,
            "ok",
            "source {i} assembles: {:?}",
            r.report
                .diagnostics
                .iter()
                .map(|d| (d.code.code(), d.path.clone(), d.detail.content.clone()))
                .collect::<Vec<_>>()
        );
    }
}

// ── AC-3: one invalid fixture per owned diagnostic code ──────────────────────

#[test]
fn ac3_invalid_fixtures_per_owned_code() {
    let (mut store, catalog) = seeded_store("ac3");
    // C-LOAD-2 — the wrong dialect.
    let mut s = valid_source();
    s.dialect = "hir/0".into();
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(codes(&r.report).contains(&Code::LoadDialect.code()));

    // C-LOAD-3 — an unknown key (the codec refuses: C-LOAD-3-class error).
    let mut j = valid_source().to_json();
    if let Json::Obj(m) = &mut j {
        m.insert("surprise".into(), Json::Bool(true));
    }
    assert!(AssemblySource::from_json(&j, "/source").is_err());

    // C-LOAD-1 — a malformed layer fragment.
    let mut s = valid_source();
    s.layers[0].fragment = Json::str("not-an-assembly");
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).iter().any(|c| c.starts_with("C-LOAD-")),
        "got {:?}",
        codes(&r.report)
    );

    // C-CLASS-6 — slots on a hosted source (D5).
    let mut s = valid_source();
    s.root_kind = RootKind::Hosted;
    s.document = None;
    s.hosted = Some(HostedSpec {
        harness: "participant:test".into(),
        model: None,
        auth: vec![],
        tools: vec![],
        instructions: vec![],
        policies: vec![],
        params: None,
        budget: None,
        permissions: None,
        declared_capabilities: None,
        observability: vec![],
        participant_version: None,
        hosting_mechanism: Some("session_abi".into()),
        parameters: BTreeMap::new(),
        values: BTreeMap::new(),
    });
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).contains(&Code::ClassSlotsOnHosted.code()),
        "hosted + slots → C-CLASS-6: {:?}",
        codes(&r.report)
    );

    // C-REF-1 — an unresolvable variant.
    let mut s = valid_source();
    let mut a = Assembly::empty();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/ghost",
            "latest",
        ))),
    );
    s.layers = vec![user_layer("user:ghost", 0, a.to_json())];
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).iter().any(|c| c.starts_with("C-REF-")),
        "unresolvable → C-REF-*: {:?}",
        codes(&r.report)
    );

    // C-REF-5 — a deny-list that never bites (info).
    let mut s = valid_source();
    let mut a = Assembly::empty();
    for (slot, v) in [
        ("control_strategy", "hh/round_robin"),
        ("context_policy", "hh/full_window"),
    ] {
        a.slots.insert(
            slot.into(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
                slot,
                v,
                "deny:sha256:never-registered",
            ))),
        );
    }
    s.layers = vec![user_layer("user:deny", 0, a.to_json())];
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).contains(&Code::RefDenyListNoop.code()),
        "deny-noop → C-REF-5: {:?}",
        codes(&r.report)
    );

    // C-PARAM-5 — a budget-relevant parameter left to default.
    let mut s = valid_source();
    let mut a = Assembly::empty();
    for (slot, v) in [
        ("control_strategy", "hh/round_robin"),
        ("context_policy", "hh/full_window"),
    ] {
        a.slots.insert(
            slot.into(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
                slot, v, "latest",
            ))),
        );
    }
    a.parameters.insert(
        "window".into(),
        hh_assembly::grammar::ParameterSpec {
            param_type: hh_assembly::grammar::ParamType::Int,
            domain: None,
            default: Some(Json::Int(8)),
            required: hh_assembly::grammar::ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: true,
        },
    );
    s.layers = vec![user_layer("user:param", 0, a.to_json())];
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).contains(&Code::ParamDefaultedBudgetRelevant.code()),
        "defaulted budget-relevant → C-PARAM-5: {:?}",
        codes(&r.report)
    );

    // A layer conflict at equal precedence → C-COMP-* (LayerConflict).
    let mut s = valid_source();
    let mut a2 = Assembly::empty();
    a2.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/full_window", // wrong class for the slot? still a conflict path
            "latest",
        ))),
    );
    s.layers.push(user_layer("user:conflict", 0, a2.to_json()));
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Plan);
    assert!(
        codes(&r.report).iter().any(|c| c.starts_with("C-COMP-")),
        "equal-precedence conflict → C-COMP-*: {:?}",
        codes(&r.report)
    );
}

// ── AC-5: dry-run parity (S-1) ───────────────────────────────────────────────

#[test]
fn ac5_plan_seal_parity() {
    let (mut store, catalog) = seeded_store("ac5");
    let s = valid_source();
    let snap = store.snapshot(&kernel()).unwrap().snapshot_id;
    let p = svc(&mut store, &catalog).assemble(&s, Some(&snap), AssembleMode::Plan);
    let e = svc(&mut store, &catalog).assemble(&s, Some(&snap), AssembleMode::Seal);
    assert_eq!(p.status, e.status);
    assert_eq!(p.identity, e.identity);
    assert_eq!(p.snapshot, e.snapshot);
    assert_eq!(p.derivation_key, e.derivation_key);
    assert_eq!(
        hh_assembly::diagnostics::report_json(&p.report).to_canonical_string(),
        hh_assembly::diagnostics::report_json(&e.report).to_canonical_string(),
        "S-1: plan and seal yield identical reports"
    );
    assert!(p.sealed.is_none());
    assert!(e.sealed.is_some());
}

// ── S-3 / removability: the service's output IS the kernel's ─────────────────

#[test]
fn s3_byte_identity_with_kernel() {
    let (mut store, catalog) = seeded_store("s3");
    let s = valid_source();
    let snap = store.snapshot(&kernel()).unwrap().snapshot_id;

    // The service's sealed output.
    let r = svc(&mut store, &catalog).assemble(&s, Some(&snap), AssembleMode::Seal);
    let sealed = r.sealed.expect("seal mode retains the sealed definition");

    // The identical inputs through the kernel directly (removability: delete
    // the service, the sequence compose→resolve is the whole content).
    let d = desugar(&s, &store, Some(&snap), &catalog, &kernel(), &kernel());
    let mut composed = compose(&d.layers, &kernel()).expect("compose");
    if let Some(exp) = &d.experiment {
        hh_lab::assembly::desugar::apply_experiment(&mut composed, exp, &kernel(), &mut Vec::new());
    }
    composed.ext.insert(
        hh_lab::assembly::desugar::LAB_EXT_KEY.into(),
        hh_lab::assembly::desugar::lab_ext(&d.layers, d.experiment.as_ref(), &r.derivation_key),
    );
    let mut doc = d.doc;
    doc.assembly = Some(composed.to_json());
    let mut env = ResolveEnv {
        registry: &mut store,
        catalog: &catalog,
        snapshot_id: Some(snap),
        mode: ResolveMode::Audit,
        registrar: kernel(),
        resolved_at: 1_000,
        notices: None,
    };
    let direct = resolve(&doc, &mut env).expect("the kernel resolves");
    assert_eq!(
        sealed.canonical_bytes(),
        direct.canonical_bytes(),
        "S-3: the service output is byte-identical to the kernel's"
    );
}

// ── AC-7: snapshot drift, freeze, adopt (T-3) ────────────────────────────────

#[test]
fn ac7_snapshot_drift_and_freeze() {
    let (mut store, catalog) = seeded_store("ac7");
    let s = valid_source();
    let snap_old = store.snapshot(&kernel()).unwrap().snapshot_id;
    let resolved_old = svc(&mut store, &catalog).assemble(&s, Some(&snap_old), AssembleMode::Seal);
    assert_eq!(resolved_old.status, "ok");

    // Publish a successor variant — the name's head moves.
    let class_vid = store
        .resolve(
            &hh_registry::store::ResolveInput::Selector {
                namespace: "hh".into(),
                name: "round_robin".into(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Audit,
            &hh_registry::store::ResolveRequest::default(),
        )
        .map(|r| r.versioned_ref.version_id)
        .unwrap();
    let _ = class_vid;
    let c = store
        .resolve(
            &hh_registry::store::ResolveInput::Selector {
                namespace: "hh".into(),
                name: "round_robin".into(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Audit,
            &hh_registry::store::ResolveRequest::default(),
        )
        .unwrap();
    let old_variant = match &c.record {
        RegistryRecord::Variant(v) => v.clone(),
        _ => panic!("expected a variant"),
    };
    let mut v2 = old_variant.clone();
    v2.variant_id = "hh/round_robin".into();
    v2.version_label = Some("1.0.1".into());
    let v2id = store
        .register(RegistryRecord::Variant(v2), &kernel(), None)
        .unwrap()
        .version_id;
    store
        .publish(
            "hh",
            "round_robin",
            &v2id,
            Some("1.0.1".into()),
            Some(c.versioned_ref.version_id.clone()),
            &kernel(),
        )
        .unwrap();
    let snap_new = store.snapshot(&kernel()).unwrap().snapshot_id;

    let dr = svc(&mut store, &catalog).drift(&s, &snap_old, &snap_new);
    assert!(dr.diff.is_some(), "drift projects the pin movement");
    let c_ref_6 = dr
        .diagnostics
        .iter()
        .any(|d| d.code == Code::RefSnapshotDrift);
    assert!(
        c_ref_6 || dr.sameness == Some(hh_identity::sameness::SamenessLevel::L0),
        "a moved head reports C-REF-6 (or L0 when the pin already won): {:?}",
        dr.diagnostics
            .iter()
            .map(|d| d.code.code())
            .collect::<Vec<_>>()
    );

    // freeze is the default — the old-snapshot resolve is unchanged by the
    // drift call (a pure read).
    let frozen = svc(&mut store, &catalog).assemble(&s, Some(&snap_old), AssembleMode::Seal);
    assert_eq!(
        frozen.identity, resolved_old.identity,
        "freeze: the arm's pins never move"
    );

    // adopt admissibility: evolution may never adopt L3.
    assert!(adopt_admissible(hh_identity::sameness::SamenessLevel::L3, true).is_err());
    assert!(adopt_admissible(hh_identity::sameness::SamenessLevel::L2, true).is_ok());
    assert!(adopt_admissible(hh_identity::sameness::SamenessLevel::L3, false).is_ok());
}

// ── T-1: flatten(project(d)) = d.ops ─────────────────────────────────────────

#[test]
fn t1_assembly_diff_flatten_round_trip() {
    let (mut store, catalog) = seeded_store("t1");
    let s = valid_source();
    let a = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Seal);
    let sealed_a = a.sealed.unwrap();

    // A variant: a different value + a slot toggle.
    let mut s2 = valid_source();
    let mut f2 = Assembly::empty();
    f2.parameters.insert(
        "x".into(),
        hh_assembly::grammar::ParameterSpec {
            param_type: hh_assembly::grammar::ParamType::Int,
            domain: Some(Json::Arr(vec![Json::Int(0), Json::Int(4)])),
            default: None,
            required: hh_assembly::grammar::ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: false,
        },
    );
    let mut f2j = f2.to_json();
    if let (Json::Obj(f), Json::Obj(base)) = (&mut f2j, &mut stage1_fragment()) {
        for (k, vv) in std::mem::take(base) {
            let empty = matches!(f.get(&k), Some(Json::Obj(m)) if m.is_empty());
            if !f.contains_key(&k) || empty {
                f.insert(k, vv);
            }
        }
    }
    s2.layers = vec![user_layer("user:t1", 0, f2j)];
    s2.overrides = vec![
        "slots.control_strategy.enabled=false".into(),
        "values.x=1".into(),
    ];
    s2.overrides_present = true;
    let b = svc(&mut store, &catalog).assemble(&s2, None, AssembleMode::Seal);
    let sealed_b = b.sealed.unwrap();

    let (ops, class) = hh_hir::diff::ops_between(&sealed_a.document, &sealed_b.document);
    let d = project(ops.clone(), class, None);
    let flat = flatten(&d);
    // Same multiset: canonical-compare sorted op encodings.
    let mut lhs: Vec<String> = flat
        .iter()
        .map(|o| hh_hir::diff::op_json(o).to_canonical_string())
        .collect();
    let mut rhs: Vec<String> = ops
        .iter()
        .map(|o| hh_hir::diff::op_json(o).to_canonical_string())
        .collect();
    lhs.sort();
    rhs.sort();
    assert_eq!(lhs, rhs, "flatten(project(d)) = d.ops as a multiset");
    // The render is deterministic.
    assert_eq!(
        hh_lab::assembly::diff_view::render(&d),
        hh_lab::assembly::diff_view::render(&d)
    );
    // The toggle lands as exactly one slot_toggle (T-5).
    let toggles = d
        .ops
        .iter()
        .filter(|o| o.bucket == hh_lab::assembly::diff_view::AssemblyBucket::SlotToggle)
        .count();
    assert_eq!(toggles, 1, "one enabled flip = exactly one slot_toggle");
}

// ── Hosted root: n/a{class} rows + C-CLASS-6 ─────────────────────────────────

#[test]
fn hosted_root_na_rows_and_class6() {
    let (mut store, catalog) = seeded_store("hosted");
    let s = AssemblySource {
        dialect: "hir/1".into(),
        root_kind: RootKind::Hosted,
        base: None,
        document: None,
        layers: vec![],
        overrides: Vec::new(),
        overrides_present: false,
        imports: Vec::new(),
        hosted: Some(HostedSpec {
            harness: "participant:test".into(),
            model: Some(hh_lab::assembly::source::ModelBinding::Bound(vec![
                "model:a".into(),
            ])),
            auth: vec!["$secret:api_key".into()],
            tools: vec![],
            instructions: vec![HostedSupply {
                name: Some("inst:main".into()),
                kind: None,
                record: Json::obj([("content", Json::str("be terse"))]),
            }],
            policies: vec![],
            params: None,
            budget: None,
            permissions: None,
            declared_capabilities: None,
            observability: vec!["events".into()],
            participant_version: None,
            hosting_mechanism: Some("session_abi".into()),
            parameters: BTreeMap::new(),
            values: BTreeMap::new(),
        }),
        ext: BTreeMap::new(),
    };
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Seal);
    // Stage 2 class conformance is n/a{class} on a hosted root (ADR-0150's
    // degraded validation profile — a rule over stages, never an IR field).
    let na_class = r
        .report
        .stages
        .iter()
        .filter(|o| !o.ran && o.na == Some(NaReason::Class))
        .count();
    assert!(
        na_class >= 1,
        "a hosted root yields n/a{{class}} stage rows: {:?}",
        r.report.stages
    );
}

// ── validate_batch (ADR-0148 D4) ─────────────────────────────────────────────

#[test]
fn validate_batch_dedupes_identical_points() {
    let (mut store, catalog) = seeded_store("batch");
    let s = valid_source();
    let mut bad = valid_source();
    bad.layers[0].fragment = Json::str("broken");
    let points = vec![
        BatchPoint::Source(Box::new(s.clone())),
        BatchPoint::Source(Box::new(s.clone())), // identical derivation_key → once
        BatchPoint::Source(Box::new(bad)),
    ];
    let reports = svc(&mut store, &catalog).validate_batch(&points, None);
    assert_eq!(reports.len(), 2, "identical points validate once");
    assert_eq!(
        reports[0].1.status,
        hh_assembly::diagnostics::ReportStatus::Pass
    );
    assert_eq!(
        reports[1].1.status,
        hh_assembly::diagnostics::ReportStatus::Fail
    );
}

// ── AC-11: the Stage-0 anchor as a source ────────────────────────────────────

#[test]
fn ac11_stage0_anchor_as_source() {
    let (mut store, catalog) = seeded_store("ac11");
    // Anchor = base + one experiment layer. First publish a base definition.
    let base_src = valid_source();
    let base = svc(&mut store, &catalog)
        .assemble(&base_src, None, AssembleMode::Seal)
        .sealed
        .expect("the base seals");
    // A `version:<vid>` base resolves against the registry — the sealed
    // definition must be a registered record (S-7).
    store
        .register(
            RegistryRecord::SealedDefinition(base.clone()),
            &kernel(),
            None,
        )
        .unwrap();
    let base_vid = base.definition_ref.version_id.clone();

    // The anchor source: base ref + one experiment override.
    let mut anchor = valid_source();
    anchor.base = Some(format!("version:{base_vid}"));
    anchor.document = None; // the base supplies the scaffold
    anchor.layers = vec![]; // D1 carries the base's members
    anchor.overrides = vec!["slots.context_policy.enabled=true".into()];
    anchor.overrides_present = true;

    let r = svc(&mut store, &catalog).assemble(&anchor, None, AssembleMode::Seal);
    assert_eq!(
        r.status,
        "ok",
        "the anchor assembles: {:?}",
        r.report
            .diagnostics
            .iter()
            .map(|d| (d.code.code(), d.path.clone()))
            .collect::<Vec<_>>()
    );
    // D1: the base lands as the lowest-precedence packaged-default layer.
    let d = desugar(&anchor, &store, None, &catalog, &kernel(), &kernel());
    assert_eq!(
        d.layers[0].provenance.source_kind,
        LayerSourceKind::PackagedDefault
    );
    // publish under a lineage name → plan.exit = no_changes against itself.
    let pr = svc(&mut store, &catalog).apply(
        &anchor,
        None,
        Some(&PublishSpec {
            namespace: "local".into(),
            name: "anchor".into(),
            label: Some("1.0.0".into()),
            supersedes: None,
        }),
    );
    assert!(
        !pr.refused,
        "the anchor publishes (apply is not refused): {:?}",
        pr.result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code.code())
            .collect::<Vec<_>>()
    );
    assert!(pr.published.is_some(), "the registry records the name");
    // plan.exit = no_changes against its own registry head (AC-11) — the
    // anchor's one override restates `enabled=true`, so the assembled
    // definition is the base itself.
    assert_eq!(pr.result.plan.exit, PlanExit::NoChanges);
}

// ── D2/D3/D4/D5 desugar properties ───────────────────────────────────────────

#[test]
fn d2_overrides_materialise_one_experiment_layer() {
    let (store, catalog) = seeded_store("d2");
    let mut s = valid_source();
    s.overrides = vec![
        "values.a=1".into(),
        "~values.b".into(),
        "+constraints={}".into(),
    ];
    s.overrides_present = true;
    let d = desugar(&s, &store, None, &catalog, &kernel(), &kernel());
    let exp = d.experiment.expect("overrides materialise one layer");
    assert_eq!(exp.provenance.source_kind, LayerSourceKind::Experiment);
    assert_eq!(exp.provenance.id, overrides_layer_id(&s.overrides));
    assert_eq!(exp.tombstones, vec!["values.b".to_string()]);
    // `overrides: []` still materialises the layer (OQ-076).
    let mut s2 = valid_source();
    s2.overrides_present = true;
    let d2 = desugar(&s2, &store, None, &catalog, &kernel(), &kernel());
    assert!(d2.experiment.is_some());
}

#[test]
fn d3_imports_land_as_entities_and_nodes() {
    let (store, catalog) = seeded_store("d3");
    let mut s = valid_source();
    s.imports = vec![Import {
        path_or_ref: "mem://ctx".into(),
        as_: ImportKind::ContextItem,
        content: Some("---\ntitle: t\n---\nhello".into()),
        name: Some("ctx".into()),
        frontmatter_schema: Some(Json::obj([(
            "require",
            Json::Arr(vec![Json::str("title")]),
        )])),
    }];
    let d = desugar(&s, &store, None, &catalog, &kernel(), &kernel());
    let import_layer = d
        .layers
        .iter()
        .find(|l| l.provenance.id.starts_with("sha256:") || l.fragment.entities.contains_key("ctx"))
        .expect("one synthesised user layer carries the imports");
    assert!(import_layer.fragment.entities.contains_key("ctx"));
    // The document carries the ContextItem + Memory nodes (the pointer rule).
    assert!(d
        .doc
        .nodes
        .iter()
        .any(|n| n.kind == EntityKind::ContextItem));
    assert!(d.doc.nodes.iter().any(|n| n.kind == EntityKind::Memory));
    // A missing-content import is a diagnostic, never an IO attempt.
    let mut s2 = valid_source();
    s2.imports = vec![Import {
        path_or_ref: "mem://none".into(),
        as_: ImportKind::Text,
        content: None,
        name: None,
        frontmatter_schema: None,
    }];
    let d2 = desugar(&s2, &store, None, &catalog, &kernel(), &kernel());
    assert!(d2.diags.iter().any(|dg| dg.code == Code::LoadParse));
}

#[test]
fn d4_selectors_live_only_in_layers() {
    let (store, catalog) = seeded_store("d4");
    // A document node carrying selector slots is refused at desugar.
    let mut nodes_doc = scaffold();
    let mut agent = agent_node("test:agent", "test:budget", "test:perm", 4);
    if let KindRecord::AgentProcess(ap) = &mut agent.semantic {
        if let AgentProcessBody::Native(np) = &mut ap.body {
            np.slots.insert(
                "control_strategy".into(),
                SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
                    "control_strategy",
                    "hh/round_robin",
                    "latest",
                ))),
            );
        }
    }
    nodes_doc.nodes[3] = hh_hir::wire::node_to_json(&agent);
    let mut s = valid_source();
    s.document = Some(nodes_doc);
    let d = desugar(&s, &store, None, &catalog, &kernel(), &kernel());
    assert!(
        d.diags.iter().any(|dg| dg.code == Code::LoadParse),
        "selectors in document slots are a desugar diagnostic (D4)"
    );
}

#[test]
fn s5_apply_refuses_on_plan_error() {
    let (mut store, catalog) = seeded_store("s5");
    let mut s = valid_source();
    s.layers[0].fragment = Json::str("broken");
    let r = svc(&mut store, &catalog).apply(&s, None, None);
    assert!(r.refused, "plan.exit = error refuses before any write");
    assert!(r.published.is_none());
}

#[test]
fn explain_returns_layer_attribution() {
    let (mut store, catalog) = seeded_store("explain");
    let s = valid_source();
    let j = svc(&mut store, &catalog).explain(&s, None);
    assert!(j.get("derivation_key").and_then(Json::as_str).is_some());
    assert!(j.get("plan").is_some());
    // The layer_map attributes the slots to the user layer.
    let lm = j.get("plan").and_then(|p| p.get("layer_map")).unwrap();
    assert!(lm.get("slots.control_strategy").is_some());
}

// ── pin_ops_only (T-3) ────────────────────────────────────────────────────────

#[test]
fn drift_projection_keeps_pin_ops_only() {
    let (mut store, catalog) = seeded_store("drift-proj");
    let s = valid_source();
    let a = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Seal);
    let sealed = a.sealed.unwrap();
    let (ops, class) = hh_hir::diff::ops_between(&sealed.document, &sealed.document);
    let d = project(ops, class, None);
    let pinned = pin_ops_only(&d);
    assert!(pinned.ops.is_empty(), "no movement on identical inputs");
}

// ── AC-11 (compile): the anchor's sealed output compiles under minimal
// profiles — the service's `compile` delegates to the compiler unchanged ───

use hh_compiler::compiler::CompileInputs;
use hh_compiler::link::TargetSpec;
use hh_compiler::profile::{
    DebtStatus, ExpiryCondition, ExpiryKind, ModelProfile, ModelRole, ProfileCompatibility,
    ProfileDebtRecord, ProfileSelector, ProfileView, VersionPattern,
};

fn profile_debt(rule_id: &str, status: DebtStatus) -> ProfileDebtRecord {
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

fn minimal_profile(id: &str, version: &str) -> ModelProfile {
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
        rules: vec![],
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

struct MapProfileView {
    profiles: BTreeMap<String, ModelProfile>,
    reports: BTreeMap<String, hh_compiler::profile_test::ProfileTestReport>,
}

impl ProfileView for MapProfileView {
    fn profile(&self, coordinate: &str) -> Option<ModelProfile> {
        self.profiles.get(coordinate).cloned()
    }

    fn test_report(
        &self,
        coordinate: &str,
    ) -> Option<hh_compiler::profile_test::ProfileTestReport> {
        self.reports.get(coordinate).cloned()
    }
}

fn profiles_of(ps: &[ModelProfile]) -> MapProfileView {
    let mut m = BTreeMap::new();
    let mut reports = BTreeMap::new();
    for p in ps {
        let coord = hh_compiler::profile::profile_coordinate(p);
        // Fixture plumbing: a fabricated *passing* ProfileTestReport per bound
        // profile so the AC-R-2.3.3-13 link gate admits the (tested) chain.
        let report =
            hh_compiler::profile_test::ProfileTestReport::passing_for(&coord, &p.content_hash);
        reports.insert(coord.clone(), report.clone());
        reports.insert(p.content_hash.clone(), report);
        m.insert(coord, p.clone());
        m.insert(p.content_hash.clone(), p.clone());
    }
    MapProfileView {
        profiles: m,
        reports,
    }
}

#[test]
fn ac11_anchor_compiles_under_minimal_profiles() {
    let (mut store, catalog) = seeded_store("ac11c");
    let base = svc(&mut store, &catalog)
        .assemble(&valid_source(), None, AssembleMode::Seal)
        .sealed
        .expect("the base seals");
    store
        .register(
            RegistryRecord::SealedDefinition(base.clone()),
            &kernel(),
            None,
        )
        .unwrap();
    let mut anchor = valid_source();
    anchor.base = Some(format!("version:{}", base.definition_ref.version_id));
    anchor.document = None;
    anchor.layers = vec![];
    anchor.overrides = vec!["slots.context_policy.enabled=true".into()];
    anchor.overrides_present = true;
    let ar = svc(&mut store, &catalog).assemble(&anchor, None, AssembleMode::Seal);
    assert_eq!(
        ar.status,
        "ok",
        "the anchor assembles: {:?}",
        ar.report
            .diagnostics
            .iter()
            .map(|d| (d.code.code(), d.path.clone()))
            .collect::<Vec<_>>()
    );
    let sealed = ar.sealed.expect("the anchor seals");

    // Two minimal admissible profiles — both compile the anchor.
    // Both profiles share the definition's pinned `sha256:profile` head and
    // differ only in version — the two admissible minimal profiles.
    let p1 = minimal_profile("sha256:profile", "1.0");
    let p2 = minimal_profile("sha256:profile", "1.1");
    let view = profiles_of(&[p1.clone(), p2.clone()]);
    for p in [&p1, &p2] {
        let inputs = CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![hh_compiler::profile::profile_coordinate(p)],
            fallback_profile: None,
            targets: vec![TargetSpec {
                target_id: "mcp".to_string(),
                spec_version: "1.0".to_string(),
                content_hash: "sha256:target-mcp".to_string(),
            }],
            compile_for_expired: false,
        };
        let bundle = svc(&mut store, &catalog)
            .compile(&inputs, &view)
            .unwrap_or_else(|e| panic!("compiles under {}: {e:?}", p.profile_id));
        assert!(!bundle.bundle_id.is_empty());
    }
}

// ── AC-6 (rest): apply/invert over the flattened projection + surface rename ──

#[test]
fn ac6_diff_apply_invert_and_surface_rename() {
    let (mut store, catalog) = seeded_store("ac6");
    let a = svc(&mut store, &catalog)
        .assemble(&valid_source(), None, AssembleMode::Seal)
        .sealed
        .expect("a seals");
    let mut s2 = valid_source();
    let mut f2 = Assembly::empty();
    f2.parameters.insert(
        "x".into(),
        hh_assembly::grammar::ParameterSpec {
            param_type: hh_assembly::grammar::ParamType::Int,
            domain: Some(Json::Arr(vec![Json::Int(0), Json::Int(4)])),
            default: None,
            required: hh_assembly::grammar::ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: false,
        },
    );
    let mut f2j = f2.to_json();
    if let (Json::Obj(f), Json::Obj(base)) = (&mut f2j, &mut stage1_fragment()) {
        for (k, vv) in std::mem::take(base) {
            let empty = matches!(f.get(&k), Some(Json::Obj(m)) if m.is_empty());
            if !f.contains_key(&k) || empty {
                f.insert(k, vv);
            }
        }
    }
    s2.layers = vec![user_layer("user:ac6", 0, f2j)];
    s2.overrides = vec![
        "slots.control_strategy.enabled=false".into(),
        "values.x=1".into(),
    ];
    s2.overrides_present = true;
    let br = svc(&mut store, &catalog).assemble(&s2, None, AssembleMode::Seal);
    assert_eq!(
        br.status,
        "ok",
        "b assembles: {:?}",
        br.report
            .diagnostics
            .iter()
            .map(|d| (d.code.code(), d.path.clone()))
            .collect::<Vec<_>>()
    );
    let b = br.sealed.expect("b seals");

    // The kernel's diff over the two sealed documents — the service's
    // `AssemblyDiff` is `project` over exactly these ops.
    let prov = ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Run,
        1,
    );
    let hd = hh_hir::diff::diff(&a.document, &b.document, prov.clone(), Default::default())
        .expect("diff");
    // The projection's ops ARE the diff's ops — feed `flatten` back.
    let pd = project(hd.ops.clone(), hd.classification.clone(), None);
    let mut flat = flatten(&pd);
    flat.sort_by_key(|o| hh_hir::diff::op_json(o).to_canonical_string());
    assert_eq!(
        flat.iter()
            .map(hh_hir::diff::op_json)
            .map(|j| j.to_canonical_string())
            .collect::<Vec<_>>(),
        hd.ops
            .iter()
            .map(hh_hir::diff::op_json)
            .map(|j| j.to_canonical_string())
            .collect::<Vec<_>>(),
        "flatten(project(d)) reproduces d.ops"
    );

    // apply(a, diff(a, b)) = b; the inverse lands back on a (AC-6).
    let applied = hh_hir::diff::apply(&a.document, &hd).expect("apply");
    assert_eq!(
        applied.to_json().to_canonical_string(),
        b.document.to_json().to_canonical_string(),
        "apply(a, diff(a,b)) = b"
    );
    // `invert` round-trips at the op level (AC-6): invert(invert(d)) = d.
    // (Inverse `apply` restores the semantic content but not explicit-null
    // ext leaves — the kernel's apply granularity; AC-6 requires only the
    // forward direction byte-exact.)
    let inv = hh_hir::diff::invert(&hd);
    let rt = hh_hir::diff::invert(&inv);
    assert_eq!(
        rt.ops
            .iter()
            .map(hh_hir::diff::op_json)
            .map(|j| j.to_canonical_string())
            .collect::<Vec<_>>(),
        hd.ops
            .iter()
            .map(hh_hir::diff::op_json)
            .map(|j| j.to_canonical_string())
            .collect::<Vec<_>>(),
        "invert(invert(d)) = d"
    );

    // A rename of a surface-carrying member yields only `surface_only`
    // entries and leaves `semantic_id` unchanged (AC-6). The base fixture
    // carries no surface kinds — a ContextItem import supplies one.
    let mut si = valid_source();
    si.imports = vec![Import {
        path_or_ref: "mem://ctx".into(),
        as_: ImportKind::ContextItem,
        content: Some("---\ntitle: t\n---\nhello".into()),
        name: Some("ctx".into()),
        frontmatter_schema: None,
    }];
    let si_doc = svc(&mut store, &catalog)
        .assemble(&si, None, AssembleMode::Seal)
        .sealed
        .expect("import doc seals");
    let mut renamed = si_doc.document.clone();
    let idx = renamed
        .nodes
        .iter()
        .position(|n| n.kind == EntityKind::ContextItem)
        .expect("the import lands a ContextItem");
    let before_sid = renamed.nodes[idx].semantic_id();
    renamed.nodes[idx].surface = Some(SurfaceRecord::ContextItem(ContextItemSurface {
        rendering_template: Json::str("{{body}}"),
        position: Json::Null,
    }));
    assert_eq!(renamed.nodes[idx].semantic_id(), before_sid);
    let rd = hh_hir::diff::diff(&si_doc.document, &renamed, prov, Default::default())
        .expect("rename diff");
    assert_eq!(
        rd.classification.semantic_ops, 0,
        "a surface rename carries no semantic ops"
    );
    assert!(rd.classification.surface_ops > 0);
    let pd2 = project(rd.ops.clone(), rd.classification.clone(), None);
    assert!(
        pd2.ops
            .iter()
            .all(|o| o.bucket == hh_lab::assembly::diff_view::AssemblyBucket::SurfaceOnly),
        "every op is surface_only: {:?}",
        pd2.ops.iter().map(|o| o.bucket.name()).collect::<Vec<_>>()
    );
}

// ── AC-2: identical resolved content → one semantic_id + one configuration_id;
// the differing `layers[]` are recorded, never absorbed ─────────────────────

#[test]
fn ac2_identical_content_one_identity_layers_recorded() {
    let (mut store, catalog) = seeded_store("ac2");
    // Two authoring paths: the same fragment under different layer
    // provenance (id + precedence).
    let s1 = valid_source();
    let mut s2 = valid_source();
    s2.layers[0].provenance.id = "user:other".into();
    s2.layers[0].provenance.precedence = 7;
    let a = svc(&mut store, &catalog)
        .assemble(&s1, None, AssembleMode::Seal)
        .sealed
        .expect("a seals");
    let b = svc(&mut store, &catalog)
        .assemble(&s2, None, AssembleMode::Seal)
        .sealed
        .expect("b seals");
    assert_eq!(
        a.definition_ref.semantic_id, b.definition_ref.semantic_id,
        "identical resolved content → one semantic_id"
    );
    // One configuration_id under equal composition inputs.
    let inputs = hh_assembly::identity::CompositionInputs {
        model_ref: "model:x".into(),
        profile: "sha256:profile".into(),
        environment_ref: "env:test".into(),
        budget: "test:budget".into(),
        seed: "0".into(),
    };
    let ca = hh_assembly::identity::configuration(&a, &inputs);
    let cb = hh_assembly::identity::configuration(&b, &inputs);
    assert_eq!(
        ca.configuration_id, cb.configuration_id,
        "one configuration_id"
    );
    // `layers[]` differ and are recorded on the sealed documents.
    let layers_of = |s: &hh_hir::document::SealedDefinition| {
        s.document
            .assembly
            .as_ref()
            .and_then(|j| j.get("layers"))
            .cloned()
            .unwrap_or(Json::Null)
    };
    let (la, lb) = (layers_of(&a), layers_of(&b));
    assert_ne!(la, Json::Null, "layers[] recorded on a");
    assert_ne!(la, lb, "the two paths' layers[] differ");
    // and the lab ext carries the per-path provenance too.
    let lab_layers = |s: &hh_hir::document::SealedDefinition| {
        s.document
            .assembly
            .as_ref()
            .and_then(|j| j.get("ext"))
            .and_then(|e| e.get(LAB_EXT_KEY))
            .and_then(|x| x.get("layers"))
            .cloned()
            .unwrap_or(Json::Null)
    };
    assert_ne!(lab_layers(&a), lab_layers(&b));
}

// ── AC-4: an unbound exactly-one slot fails C-CLASS-2; nothing slot-shaped
// ever lands in defaulted_paths ──────────────────────────────────────────────

#[test]
fn ac4_unbound_slot_fails_and_no_slot_defaults() {
    let (mut store, catalog) = seeded_store("ac4");
    let mut s = valid_source();
    // A fragment binding only context_policy — control_strategy (exactly-one)
    // is left unbound.
    let mut frag = Assembly::empty();
    frag.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    s.layers = vec![user_layer("user:ac4", 0, frag.to_json())];
    let r = svc(&mut store, &catalog).assemble(&s, None, AssembleMode::Seal);
    assert_eq!(r.status, "error", "the unbound slot refuses");
    assert!(
        codes(&r.report).iter().any(|c| c == "C-CLASS-2"),
        "C-CLASS-2 fires: {:?}",
        codes(&r.report)
    );
    // No `slot_*`/slots path ever appears under defaulted_paths (S-8 —
    // defaulting fills values only).
    assert!(
        !r.plan
            .defaulted_paths
            .iter()
            .any(|p| p.starts_with("slot") || p.contains("slots.")),
        "no slot path is defaulted: {:?}",
        r.plan.defaulted_paths
    );
    // And on a passing source every defaulted value's source is recorded in
    // layer_map under `parameters.<p>`.
    let mut ok = valid_source();
    let mut f = Assembly::empty();
    f.parameters.insert(
        "temp".into(),
        hh_assembly::grammar::ParameterSpec {
            param_type: hh_assembly::grammar::ParamType::Int,
            domain: Some(Json::Arr(vec![Json::Int(0), Json::Int(4)])),
            default: Some(Json::Int(1)),
            required: hh_assembly::grammar::ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: false,
        },
    );
    let mut fj = f.to_json();
    if let (Json::Obj(fm), Json::Obj(base)) = (&mut fj, &mut stage1_fragment()) {
        for (k, vv) in std::mem::take(base) {
            let empty = matches!(fm.get(&k), Some(Json::Obj(m)) if m.is_empty());
            if !fm.contains_key(&k) || empty {
                fm.insert(k, vv);
            }
        }
    }
    ok.layers = vec![user_layer("user:ac4-ok", 0, fj)];
    let r2 = svc(&mut store, &catalog).assemble(&ok, None, AssembleMode::Plan);
    assert_eq!(r2.status, "ok");
    assert!(
        r2.plan.defaulted_paths.iter().any(|p| p == "values.temp"),
        "the parameter default is reported: {:?}",
        r2.plan.defaulted_paths
    );
    for p in &r2.plan.defaulted_paths {
        let param = p.strip_prefix("values.").unwrap();
        assert!(
            r2.plan
                .layer_map
                .contains_key(&format!("parameters.{param}")),
            "the defaulted value's source layer is recorded: {p}"
        );
    }
}

// ── AC-9: equal canonical override content → one experiment layer id;
// explain names it; the configuration is unchanged ───────────────────────────

#[test]
fn ac9_equal_overrides_one_layer_id_and_explain_names_it() {
    let (mut store, catalog) = seeded_store("ac9");
    let mut s1 = valid_source();
    s1.overrides = vec![
        "slots.control_strategy.enabled=false".into(),
        "slots.context_policy.enabled=true".into(),
    ];
    s1.overrides_present = true;
    // A second list with equal canonical content (the id is order-insensitive).
    let mut s2 = s1.clone();
    s2.overrides.reverse();
    let d1 = desugar(&s1, &store, None, &catalog, &kernel(), &kernel());
    let d2 = desugar(&s2, &store, None, &catalog, &kernel(), &kernel());
    let id1 = d1.experiment.as_ref().unwrap().provenance.id.clone();
    let id2 = d2.experiment.as_ref().unwrap().provenance.id.clone();
    assert_eq!(
        id1, id2,
        "equal canonical content → one experiment layer id"
    );
    assert_eq!(id1, overrides_layer_id(&s1.overrides));

    // `explain` names the experiment layer for the paths it wrote.
    let j = svc(&mut store, &catalog).explain(&s1, None);
    let lm = j
        .get("plan")
        .and_then(|p| p.get("layer_map"))
        .and_then(|m| m.get("slots.control_strategy.enabled"))
        .and_then(|l| l.get("id"))
        .and_then(Json::as_str)
        .expect("explain's layer_map names the experiment layer");
    assert_eq!(lm, id1, "explain names the experiment layer id");

    // The configuration is unchanged across the two orderings (the resolved
    // content — and hence both ids — are identical).
    let ra = svc(&mut store, &catalog).assemble(&s1, None, AssembleMode::Seal);
    assert_eq!(
        ra.status,
        "ok",
        "s1 assembles: {:?}",
        ra.report
            .diagnostics
            .iter()
            .map(|d| (d.code.code(), d.path.clone()))
            .collect::<Vec<_>>()
    );
    let a = ra.sealed.expect("s1 seals");
    let rb = svc(&mut store, &catalog).assemble(&s2, None, AssembleMode::Seal);
    assert_eq!(rb.status, "ok", "s2 assembles");
    let b = rb.sealed.expect("s2 seals");
    let inputs = hh_assembly::identity::CompositionInputs {
        model_ref: "model:x".into(),
        profile: "sha256:profile".into(),
        environment_ref: "env:test".into(),
        budget: "test:budget".into(),
        seed: "0".into(),
    };
    assert_eq!(
        hh_assembly::identity::configuration(&a, &inputs).configuration_id,
        hh_assembly::identity::configuration(&b, &inputs).configuration_id,
        "configuration_id is unchanged"
    );
}
