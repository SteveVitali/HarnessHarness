//! AC-R-2.5.1-{1,2,3,7,10} — the typed-capability slice (§5d.1; S1.17):
//! V-E1 at `register` + `lifecycle.capability.registered`, the semantic-id
//! projection, `project_risk`/`required_grants`/`estimate`, catalog and
//! snapshot determinism, and the `error_classes`/renderer coverage halves of
//! AC-R-2.5.1-10.

use std::collections::BTreeSet;
use std::path::PathBuf;

use hh_hir::document::Node;
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, EntityKind, Mutability, RepeatSafety,
    Reversibility, ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{KindRecord, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_ontology::risk::{RiskReversibility, RiskScope};
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::capability::{
    capability_at, catalog, estimate, project_risk, required_grants, search_projection,
    CatalogFilter, EstimateBasis,
};
use hh_registry::records::{CapabilityRecord, RegistryRecord};
use hh_registry::store::RegistryStore;
use hh_registry::RegistryError;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-capability-test-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("capability.test", 0)
}

fn text(s: &str, seq: u64) -> Text {
    Text::new(s, "test", kernel_text_prov(seq))
}

fn kernel_text_prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("tester", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn attrs(mutability: Mutability, world: World) -> EffectAttributes {
    EffectAttributes {
        mutability,
        repeat_safety: RepeatSafety::Idempotent,
        world,
        reversibility: Reversibility::Compensable,
    }
}

/// A minimal V-E1-clean capability (`pure`, `scope_bindings_unknown`,
/// `error_classes` declared, native source, `kernel_internal` executor).
fn cap_record(seq: u64, effects: ToolEffects, scope: ScopeBindings) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: text("a test capability", seq),
        input_schema: Json::obj([(
            "properties",
            Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
        )]),
        output_schema: None,
        effects,
        preconditions: vec![],
        scope_bindings: scope,
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![Json::str("io_error"), Json::str("denied")]),
        )]),
        cost_model: None,
        execution_requirement: Json::obj([("environment_class", Json::str("kernel_internal"))]),
        source: Json::obj([("kind", Json::str("native_variant"))]),
        exposure_hint: Json::obj([("default", Json::str("direct"))]),
        postconditions: vec![],
        flow_contract: None,
    }
}

fn cap_node(rec: ToolCapabilityRecord, seq: u64) -> Node {
    Node::new(
        EntityKind::ToolCapability,
        KindRecord::ToolCapability(rec),
        kernel_text_prov(seq),
    )
}

fn open_store(tag: &str) -> RegistryStore {
    RegistryStore::open(dir(tag), &kernel()).expect("open")
}

// ── AC-R-2.5.1-1 — V-E1 at register + lifecycle event ────────────────────────

#[test]
fn ac_e1_1_clean_capability_registers_with_lifecycle_event() {
    let mut store = open_store("e1-1-clean");
    let node = cap_node(cap_record(1, ToolEffects::Pure, ScopeBindings::Unknown), 1);
    let vref = store
        .register(
            RegistryRecord::Capability(CapabilityRecord { node }),
            &kernel(),
            None,
        )
        .expect("register");
    assert_eq!(vref.kind, hh_identity::RecordKind::RegistryRecord);
    // `lifecycle.capability.registered` is emitted (content-free — ids only).
    let events = store.drain_events();
    assert!(
        events
            .iter()
            .any(|e| e.class == "lifecycle.capability.registered"),
        "events: {:?}",
        events.iter().map(|e| &e.class).collect::<Vec<_>>()
    );
}

#[test]
fn ac_e1_1_ve1_violations_refuse_at_register() {
    let mut store = open_store("e1-1-bad");
    // `effects = ∅` without `pure` — V-E1-1.
    let node = cap_node(
        cap_record(
            1,
            ToolEffects::Declared(BTreeSet::new()),
            ScopeBindings::Unknown,
        ),
        1,
    );
    let err = store
        .register(
            RegistryRecord::Capability(CapabilityRecord { node }),
            &kernel(),
            None,
        )
        .expect_err("empty declared effects must refuse");
    assert!(matches!(err, RegistryError::CapabilityValidation { .. }));
    // A scoped domain without a binding and without `unknown` — V-E1-3.
    let mut store2 = open_store("e1-1-scope");
    let effects = ToolEffects::Declared(
        [EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(attrs(Mutability::Additive, World::Closed)),
        }]
        .into_iter()
        .collect(),
    );
    let node = cap_node(
        cap_record(2, effects, ScopeBindings::Bindings(Json::Arr(vec![]))),
        2,
    );
    assert!(matches!(
        store2.register(
            RegistryRecord::Capability(CapabilityRecord { node }),
            &kernel(),
            None
        ),
        Err(RegistryError::CapabilityValidation { .. })
    ));
    // A non-capability node refuses on `kind`.
    let mut store3 = open_store("e1-1-kind");
    let mut node = cap_node(cap_record(3, ToolEffects::Pure, ScopeBindings::Unknown), 3);
    node.kind = EntityKind::HarnessRule;
    assert!(matches!(
        store3.register(
            RegistryRecord::Capability(CapabilityRecord { node }),
            &kernel(),
            None
        ),
        Err(RegistryError::SchemaViolation { .. })
    ));
    // Refusals emit `lifecycle.registry.admission_refused`, never partial writes.
    let ev = store3.drain_events();
    assert!(ev
        .iter()
        .any(|e| e.class == "lifecycle.registry.admission_refused"));
}

// ── AC-R-2.5.1-2 — the semantic projection ───────────────────────────────────

#[test]
fn ac_e1_2_surface_and_hint_changes_keep_semantic_id() {
    let base = cap_node(cap_record(1, ToolEffects::Pure, ScopeBindings::Unknown), 1);
    // `exposure_hint` is runtime-only — outside `semantic_id` (V-E1-9).
    let mut b = base.clone();
    if let KindRecord::ToolCapability(t) = &mut b.semantic {
        t.exposure_hint = Json::obj([("default", Json::str("deferred"))]);
    }
    assert_eq!(
        base.semantic_id(),
        b.semantic_id(),
        "exposure_hint excluded"
    );
    // `cost_model.measured_ref` likewise — within an otherwise identical
    // declared cost model (the `cost_model` member itself is semantic).
    let mut d = base.clone();
    let mut e = base.clone();
    for (n, mr) in [(&mut d, "sha256:m1"), (&mut e, "sha256:m2")] {
        if let KindRecord::ToolCapability(t) = &mut n.semantic {
            t.cost_model = Some(Json::obj([
                (
                    "declared",
                    Json::obj([(
                        "time.wall_ms",
                        Json::obj([
                            ("confidence", Json::str("measured")),
                            ("value", Json::Int(5)),
                        ]),
                    )]),
                ),
                ("measured_ref", Json::str(mr)),
            ]));
        }
    }
    assert_eq!(d.semantic_id(), e.semantic_id(), "measured_ref excluded");
    assert_ne!(
        base.semantic_id(),
        d.semantic_id(),
        "cost_model is semantic"
    );
    // A surface rename/description change is a surface member — not semantic.
    b.surface = Some(hh_hir::records::SurfaceRecord::Tool(Box::new(
        hh_hir::records::ToolSurface {
            name: "renamed_tool".into(),
            namespace: "test".into(),
            description_template: text("different prose", 9),
            argument_order: vec!["path".into()],
            examples: Json::Null,
            error_format: Json::Null,
            result_renderer: Json::Null,
            strictness: Json::Null,
            schema_dialect_narrowing: Json::Null,
            exposure_mode: Json::Null,
            display_title: None,
            icon_ref: None,
        },
    )));
    assert_eq!(base.semantic_id(), b.semantic_id(), "surface excluded");
    // `effects` is semantic — a change re-mints.
    let mut f = base.clone();
    if let KindRecord::ToolCapability(t) = &mut f.semantic {
        t.effects = ToolEffects::Declared(
            [EffectClass {
                domain: EffectDomain::FsWrite,
                attributes: Some(attrs(Mutability::Additive, World::Closed)),
            }]
            .into_iter()
            .collect(),
        );
    }
    assert_ne!(base.semantic_id(), f.semantic_id(), "effects are semantic");
}

// ── AC-R-2.5.1-3 — effect discipline + project_risk ─────────────────────────

#[test]
fn ac_e1_3_project_risk_axes() {
    let mut store = open_store("e1-3");
    // read_only ⇒ read_only reversibility.
    let ro = cap_record(
        1,
        ToolEffects::Declared(
            [EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(EffectAttributes {
                    mutability: Mutability::ReadOnly,
                    repeat_safety: RepeatSafety::Idempotent,
                    world: World::Closed,
                    reversibility: Reversibility::Compensable,
                }),
            }]
            .into_iter()
            .collect(),
        ),
        ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
            ("param_path", Json::str("path")),
            ("scope_kind", Json::str("fs_path")),
        ])])),
    );
    let vref = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(ro, 1),
            }),
            &kernel(),
            None,
        )
        .expect("register");
    let risk = project_risk(&store, &vref.version_id).expect("project_risk");
    assert_eq!(risk.reversibility, RiskReversibility::ReadOnly);

    // Attributes absent ⇒ the `unknown_domain` honesty form registers
    // `quarantined` and projects the most dangerous class.
    let unknown = cap_record(
        2,
        ToolEffects::Declared(
            [EffectClass {
                domain: EffectDomain::NetEgress,
                attributes: None,
            }]
            .into_iter()
            .collect(),
        ),
        ScopeBindings::Unknown,
    );
    let vref = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(unknown, 2),
            }),
            &kernel(),
            None,
        )
        .expect("register");
    let (env, _rec) = store.get(&vref.version_id).expect("stored");
    assert_eq!(env.admission, hh_registry::kinds::Admission::Quarantined);
    let risk = project_risk(&store, &vref.version_id).expect("project_risk");
    assert_eq!(risk.scope, RiskScope::External);
    assert_eq!(risk.reversibility, RiskReversibility::Irreversible);
    assert_eq!(
        risk.repeat_safety,
        hh_ontology::risk::RepeatSafety::NonIdempotent
    );

    // `pure` ⇒ read-only.
    let vref = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(cap_record(3, ToolEffects::Pure, ScopeBindings::Unknown), 3),
            }),
            &kernel(),
            None,
        )
        .expect("register");
    let risk = project_risk(&store, &vref.version_id).expect("project_risk");
    assert_eq!(risk.reversibility, RiskReversibility::ReadOnly);
}

// ── AC-R-2.5.1-7 — snapshot + projection determinism ─────────────────────────

#[test]
fn ac_e1_7_catalog_search_snapshot_are_deterministic() {
    let mut store = open_store("e1-7");
    let v1 = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(cap_record(1, ToolEffects::Pure, ScopeBindings::Unknown), 1),
            }),
            &kernel(),
            None,
        )
        .expect("register 1");
    let v2 = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(cap_record(2, ToolEffects::Pure, ScopeBindings::Unknown), 2),
            }),
            &kernel(),
            None,
        )
        .expect("register 2");

    // `snapshot` over the same closure yields the same id (content-addressed);
    // `catalog` over it is deterministic.
    let snap = store.snapshot(&kernel()).expect("snapshot");
    // The id is content-addressed over the closure — it re-mints to itself.
    assert_eq!(
        hh_registry::identity::snapshot_id_of(&snap),
        snap.snapshot_id
    );
    let a = catalog(&store, &CatalogFilter::default(), &snap).expect("catalog a");
    let b = catalog(&store, &CatalogFilter::default(), &snap).expect("catalog b");
    assert_eq!(a, b, "catalog deterministic");
    assert_eq!(a.len(), 2);

    // `search_projection` is deterministic per version_id.
    let pa = search_projection(&store, &v1.version_id).expect("projection");
    let pb = search_projection(&store, &v1.version_id).expect("projection");
    assert_eq!(pa, pb);

    // `lookup` is the direct read; `capability_at` agrees.
    let l = store.lookup(&v2.version_id).expect("lookup");
    let (c, _r) = capability_at(&store, &v2.version_id).expect("capability_at");
    assert_eq!(l.record, RegistryRecord::Capability(c.clone()));
    assert!(store.lookup("sha256:nothing").is_err(), "fail-closed");
}

// ── AC-R-2.5.1-10 — error_classes + estimate (never 0) ───────────────────────

#[test]
fn ac_e1_10_error_classes_declared_and_estimate_never_zero() {
    let mut store = open_store("e1-10");
    let vref = store
        .register(
            RegistryRecord::Capability(CapabilityRecord {
                node: cap_node(cap_record(1, ToolEffects::Pure, ScopeBindings::Unknown), 1),
            }),
            &kernel(),
            None,
        )
        .expect("register");
    let (c, _) = capability_at(&store, &vref.version_id).expect("capability");
    let hh_hir::records::KindRecord::ToolCapability(t) = &c.node.semantic else {
        panic!("kind");
    };
    // `error_classes` is declared on every record.
    let classes = t
        .observation_contract
        .get("error_classes")
        .expect("error_classes declared");
    assert!(matches!(classes, Json::Arr(v) if !v.is_empty()));

    // `estimate` returns `unknown` for undeclared dimensions — never 0.
    let est = estimate(&store, &vref.version_id, None).expect("estimate");
    assert_eq!(
        est.per_dimension.len(),
        hh_ontology::dimensions::DimensionId::ALL.len()
    );
    assert!(est
        .per_dimension
        .values()
        .all(|d| d.value.is_none() && d.basis == EstimateBasis::None));

    // `required_grants` — `pure` ⇒ no grants; declared effects enumerate.
    assert!(required_grants(&store, &vref.version_id)
        .expect("grants")
        .is_empty());
}
