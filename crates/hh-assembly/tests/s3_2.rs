//! `hh-assembly` S3.2 acceptance suite (spec §3.3.3/§3.3.4; ticket S3.2;
//! R-2.1.4, R-2.1.4¹ᵃ): `compose` with layers + monotone `authority_cap`
//! (AC-CC-05/T-LCD-14 half), `space`/`enumerate` with the ADR-0025 override
//! grammar and unfilled budget placeholders (AC-CC-08/T-LCD-14 half), and
//! `validate_assembly` stage 6a (`C-PROF-1` declaration-level compatibility).

mod common;

use std::collections::BTreeMap;

use common::*;
use hh_assembly::catalog::ClassCatalog;
use hh_assembly::compose::{compose, Layer};
use hh_assembly::diagnostics::{Code, ReportStatus, Stage};
use hh_assembly::grammar::{
    Assembly, Constraint, ConstraintKind, LayerProvenance, LayerSourceKind, ParamRequirement,
    ParamType, ParameterSpec, ProfileBinding,
};
use hh_assembly::space::{enumerate, space, Override, OverrideOp, SweepDesign};
use hh_assembly::Stage1Catalog;
use hh_hir::records::SlotBindings;
use hh_hir::refs::{ComponentVariantRef, ProfileRef};
use hh_identity::names::ResolveMode;
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

fn layer(kind: LayerSourceKind, id: &str, precedence: i64, fragment: Assembly) -> Layer {
    Layer {
        provenance: LayerProvenance {
            source_kind: kind,
            id: id.to_string(),
            version: "1".to_string(),
            precedence,
        },
        fragment,
    }
}

fn cap(of: &str, ceiling: &str) -> Constraint {
    Constraint {
        kind: ConstraintKind::AuthorityCap,
        subject: Json::obj([("of", Json::str(of)), ("ceiling", Json::str(ceiling))]),
        source: None,
    }
}

fn resolve_env<'a>(
    store: &'a mut hh_registry::store::RegistryStore,
    catalog: &'a dyn ClassCatalog,
) -> hh_assembly::resolve::ResolveEnv<'a> {
    hh_assembly::resolve::ResolveEnv {
        registry: store,
        catalog,
        snapshot_id: None,
        mode: ResolveMode::Audit,
        registrar: kernel(),
        resolved_at: 7,
    }
}

// ── compose — layered merge (§3.3.3) ─────────────────────────────────────────

#[test]
fn compose_merges_layers_by_declared_policy_and_records_provenance() {
    // packaged-default sets the values; a project layer overrides one key.
    let mut base = stage1_assembly();
    base.values.insert("temperature".into(), Json::Int(1));
    base.values.insert("retries".into(), Json::Int(3));
    let mut proj = Assembly::empty();
    proj.values.insert("temperature".into(), Json::Int(0));

    let out = compose(
        &[
            layer(LayerSourceKind::PackagedDefault, "default", 0, base),
            layer(LayerSourceKind::Project, "project", 10, proj),
        ],
        &kernel(),
    )
    .expect("composes");
    // `values` is `replace` per key: the project value wins, the untouched key
    // survives.
    assert_eq!(out.values.get("temperature"), Some(&Json::Int(0)));
    assert_eq!(out.values.get("retries"), Some(&Json::Int(3)));
    // The slots the project layer did not touch survive from the default.
    assert!(out.slots.contains_key("control_strategy"));
    // `layers[]` records both participants, highest precedence first.
    let layers = out.layers.expect("layers recorded");
    assert_eq!(layers[0].id, "project");
    assert_eq!(layers[1].id, "default");
    assert_eq!(layers[0].source_kind, LayerSourceKind::Project);
}

#[test]
fn compose_equal_precedence_disagreement_is_layer_conflict() {
    let mut a = Assembly::empty();
    a.values.insert("temperature".into(), Json::Int(0));
    let mut b = Assembly::empty();
    b.values.insert("temperature".into(), Json::Int(1));
    let errs = compose(
        &[
            layer(LayerSourceKind::User, "user", 5, a),
            layer(LayerSourceKind::Project, "project", 5, b),
        ],
        &kernel(),
    )
    .expect_err("equal-precedence disagreement refuses");
    let d = errs
        .iter()
        .find(|d| d.code == Code::CompLayerConflict)
        .expect("C-COMP-2");
    assert_eq!(d.stage, Stage::Compose);
    assert_eq!(d.source_layer.as_deref(), Some("project"));
}

#[test]
fn compose_disablement_from_any_layer_wins() {
    // StricterWins on `/slots/*/enabled`: the lower-precedence layer's
    // `enabled = false` survives the higher layer's `enabled = true`.
    let mut low = Assembly::empty();
    let mut disabled = hh_hir::records::SlotBinding::of(ComponentVariantRef::selected(
        "control_strategy",
        "hh/round_robin",
        "latest",
    ));
    disabled.enabled = false;
    low.slots
        .insert("control_strategy".into(), SlotBindings::One(disabled));
    let high = stage1_assembly(); // binds control_strategy enabled
    let out = compose(
        &[
            layer(LayerSourceKind::User, "user", 1, low),
            layer(LayerSourceKind::PackagedDefault, "default", 9, high),
        ],
        &kernel(),
    )
    .expect("composes");
    match out.slots.get("control_strategy").expect("bound") {
        SlotBindings::One(b) => assert!(!b.enabled, "a disablement from any layer wins"),
        _ => panic!("one binding"),
    }
}

#[test]
fn compose_stamps_constraint_sources() {
    let mut a = Assembly::empty();
    a.constraints.push(cap("budget", "definition"));
    let out = compose(
        &[layer(LayerSourceKind::Organisation, "org", 5, a)],
        &kernel(),
    )
    .expect("composes");
    assert_eq!(out.constraints.len(), 1);
    assert_eq!(
        out.constraints[0].source.as_ref().map(|s| s.id.as_str()),
        Some("org"),
        "the constraint names its authoring layer (OQ-076)"
    );
}

// ── AC-CC-05 — authority_cap monotone (T-LCD-14 half) ────────────────────────

#[test]
fn ac_cc_05_experiment_layer_widening_is_authority_violation() {
    // A security/budget cap from a high-precedence layer bounds the subject;
    // the experiment layer may only narrow it.
    let mut caps = Assembly::empty();
    caps.constraints.push(cap("budget", "definition"));
    let mut experiment = Assembly::empty();
    experiment.constraints.push(cap("budget", "kernel")); // wider
    let errs = compose(
        &[
            layer(LayerSourceKind::Organisation, "secops", 50, caps),
            layer(LayerSourceKind::Experiment, "exp-1", 1, experiment),
        ],
        &kernel(),
    )
    .expect_err("widening refuses");
    let d = errs
        .iter()
        .find(|d| d.code == Code::CompAuthorityViolation)
        .expect("C-COMP-1 AuthorityViolation");
    // …naming the capping layer.
    assert_eq!(d.source_layer.as_deref(), Some("exp-1"));
    assert!(
        format!("{:?}", d.detail).contains("secops"),
        "the diagnostic names the capping layer"
    );
}

#[test]
fn ac_cc_05_a_narrower_cap_from_the_experiment_layer_is_admissible() {
    let mut caps = Assembly::empty();
    caps.constraints.push(cap("budget", "principal"));
    let mut experiment = Assembly::empty();
    experiment.constraints.push(cap("budget", "external")); // narrower
    let out = compose(
        &[
            layer(LayerSourceKind::Organisation, "secops", 50, caps),
            layer(LayerSourceKind::Experiment, "exp-1", 1, experiment),
        ],
        &kernel(),
    )
    .expect("a narrowing cap is admissible");
    assert_eq!(out.constraints.len(), 2, "both caps are recorded");
}

// ── space / enumerate + the override grammar (AC-CC-08; T-LCD-14) ────────────

#[test]
fn override_grammar_parses_all_five_forms() {
    let set = Override::parse("temperature=0.5").unwrap();
    assert_eq!(set.op, OverrideOp::Set);
    assert_eq!(set.path, "temperature");
    let app = Override::parse("+allow=fs.read").unwrap();
    assert_eq!(app.op, OverrideOp::Append);
    let aos = Override::parse("++allow=fs.write").unwrap();
    assert_eq!(aos.op, OverrideOp::AppendOrSet);
    let del = Override::parse("~temperature").unwrap();
    assert_eq!(del.op, OverrideOp::Delete);
    assert!(del.values.is_empty());
    let sweep = Override::parse("temperature=0,0.5,1").unwrap();
    assert_eq!(sweep.values.len(), 3, "choice sweep");
    assert!(Override::parse("=1").is_err());
    assert!(Override::parse("~").is_err());
}

/// A sealed definition carrying one sweepable `temperature` parameter.
fn sealed_with_param(
    tag: &str,
) -> (
    hh_registry::store::RegistryStore,
    hh_hir::document::SealedDefinition,
) {
    let (mut store, _vids) = seeded_store(tag);
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    a.parameters.insert(
        "temperature".into(),
        ParameterSpec {
            param_type: ParamType::Real,
            domain: Some(Json::Arr(vec![Json::Int(0), Json::Int(1)])),
            default: Some(Json::Int(0)),
            required: ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: false,
        },
    );
    a.values.insert("temperature".into(), Json::Int(0));
    // `$param:` form — every declared parameter must be *used* (stage 3).
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.params
            .insert("temperature".into(), Json::str("$param:temperature"));
    }
    let doc = doc_with(&a);
    let sealed = {
        let mut env = resolve_env(&mut store, &cat);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    (store, sealed)
}

#[test]
fn ac_cc_08_enumerate_emits_points_with_unfilled_budget_placeholders() {
    let (_store, sealed) = sealed_with_param("s32-enum");
    let sp = space(&sealed.document, &kernel()).expect("space");
    // The space carries the declared parameters and the bound slot choices.
    assert!(sp.params.contains_key("temperature"));
    assert!(sp.slot_choices.contains_key("control_strategy"));
    assert_eq!(sp.slot_choices["control_strategy"].len(), 1);

    let cat = Stage1Catalog::stage1();
    let out = enumerate(&sp, &SweepDesign::FullFactorial, &cat, None, &kernel());
    // `temperature ∈ {0,1}` — two arms.
    assert_eq!(out.points.len(), 2, "rejected: {:?}", out.rejected.len());
    for p in &out.points {
        // AC-CC-08: unfilled placeholders — no point is runnable until the
        // experiment engine fills them.
        assert!(p.search_budget.is_none(), "search_budget unfilled");
        assert!(p.eval_budget.is_none(), "eval_budget unfilled");
        assert_eq!(p.validation.status, ReportStatus::Pass, "pre-spend gate");
        assert_eq!(p.overrides.len(), 1);
    }
    let values: BTreeMap<String, ()> = out
        .points
        .iter()
        .map(|p| (p.overrides[0].clone(), ()))
        .collect();
    assert!(values.contains_key("temperature=0"));
    assert!(values.contains_key("temperature=1"));
}

#[test]
fn enumerate_override_list_and_group_override() {
    let (_store, sealed) = sealed_with_param("s32-ovr");
    let sp = space(&sealed.document, &kernel()).expect("space");
    let cat = Stage1Catalog::stage1();
    let out = enumerate(
        &sp,
        &SweepDesign::OverrideList(vec![
            Override::parse("temperature=1").unwrap(),
            Override::parse("control_strategy=hh/round_robin").unwrap(), // group override
        ]),
        &cat,
        None,
        &kernel(),
    );
    assert_eq!(out.points.len(), 2, "rejected: {:?}", out.rejected);
    // The group override retargeted the slot's variant (and unpinned it — the
    // point is re-resolved before seal).
    let doc = &out.points[1].document;
    let s = doc.assembly.as_ref().unwrap().to_canonical_string();
    assert!(s.contains("hh/round_robin"));
    // A refused path — undeclared parameter — routes to `rejected`, never silently.
    let out2 = enumerate(
        &sp,
        &SweepDesign::OverrideList(vec![Override::parse("nope=1").unwrap()]),
        &cat,
        None,
        &kernel(),
    );
    assert_eq!(out2.points.len(), 0);
    assert_eq!(out2.rejected.len(), 1);
}

// ── stage 6a — declaration-level profile compatibility (R-2.1.4¹ᵃ; C-PROF-1) ──

/// A `ProfileView` over raw record JSON (stage 6a's read seam).
struct JsonProfiles(BTreeMap<String, Json>);

impl hh_assembly::validate::ProfileView for JsonProfiles {
    fn profile(&self, coordinate: &str) -> Option<Json> {
        self.0.get(coordinate).cloned()
    }
}

#[test]
fn stage_6a_refuses_a_variant_requirement_the_profile_does_not_support() {
    // Register a variant whose capability_declaration *requires* `image_input`.
    let (mut store, _vids) = seeded_store("s32-6a");
    let cat = Stage1Catalog::stage1();
    let c = store
        .register(
            RegistryRecord::Class(class_record("control_strategy")),
            &kernel(),
            None,
        )
        .expect("register class");
    let mut v = variant_record(
        &c.version_id,
        "hh/cap_test",
        hh_registry::kinds::Placement::InProcess,
    );
    // `deterministic` is the class `declaration_schema`'s admitted key — the
    // variant *requires* it.
    v.capability_declaration
        .insert("deterministic".into(), Json::str("required"));
    let vv = store
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .expect("register variant");
    store
        .publish("hh", "cap_test", &vv.version_id, None, None, &kernel())
        .expect("publish");

    // The authored doc pins `profile_ref = sha256:profile` on the root and binds
    // the new variant by selector.
    let mut a = stage1_assembly();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(hh_hir::records::SlotBinding::of(
            ComponentVariantRef::selected("control_strategy", "hh/cap_test", "latest"),
        )),
    );
    a.profile_binding = ProfileBinding::Pinned(ProfileRef {
        profile: "sha256:profile".into(),
        pinned: true,
    });
    let doc = doc_with(&a);
    let sealed = {
        let mut env = resolve_env(&mut store, &cat);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    assert!(sealed
        .document
        .assembly
        .as_ref()
        .unwrap()
        .to_canonical_string()
        .contains(&vv.version_id));

    // A profile record that does NOT declare `deterministic` supported. The
    // catalog is the registry-backed view — stage 6a reads `VariantRecord`s
    // through it.
    let regcat = hh_assembly::catalog::RegistryCatalog::new(&store, None);
    let empty_caps = Json::obj([("capabilities", Json::obj([]))]);
    let mut no_caps = BTreeMap::new();
    no_caps.insert("sha256:profile".to_string(), empty_caps);
    let profiles = JsonProfiles(no_caps);
    let r = hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&sealed),
        &regcat,
        Some(&profiles),
        &kernel(),
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == Code::ProfIncompatible && d.stage == Stage::Validate(6)),
        "C-PROF-1 fires: {:?}",
        r.diagnostics
            .iter()
            .map(|d| d.code.code())
            .collect::<Vec<_>>()
    );

    // A profile that DOES support it passes stage 6a.
    let mut with_caps = BTreeMap::new();
    with_caps.insert(
        "sha256:profile".to_string(),
        Json::obj([(
            "capabilities",
            Json::obj([("deterministic", Json::str("supported"))]),
        )]),
    );
    let ok_profiles = JsonProfiles(with_caps);
    let r2 = hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&sealed),
        &regcat,
        Some(&ok_profiles),
        &kernel(),
    );
    assert!(
        !r2.diagnostics
            .iter()
            .any(|d| d.code == Code::ProfIncompatible),
        "the capable profile is admissible"
    );
}

#[test]
fn stage_6a_compatible_profile_and_declarations_pass() {
    let doc = doc_with(&stage1_assembly());
    let cat = Stage1Catalog::stage1();
    // The fixture's root pins `sha256:profile`; the view supplies it with an
    // empty capability set — no variant declares a requirement, so stage 6a is
    // silent.
    let empty_caps = Json::obj([("capabilities", Json::obj([]))]);
    let mut m = BTreeMap::new();
    m.insert("sha256:profile".to_string(), empty_caps);
    let profiles = JsonProfiles(m);
    let r = hh_assembly::validate_assembly(
        hh_assembly::Subject::Authored(&doc),
        &cat,
        Some(&profiles),
        &kernel(),
    );
    assert!(
        !r.diagnostics.iter().any(|d| d.stage == Stage::Validate(6)),
        "no stage-6 diagnostics: {:?}",
        r.diagnostics
            .iter()
            .map(|d| d.code.code())
            .collect::<Vec<_>>()
    );
}
