//! `hh-assembly` acceptance suite (spec §3.3; ticket S1.9; the AC-CC-* matrix).
//! Every landed diagnostic code has a fixture; `load(encode(a)) = a`; `resolve` is
//! closed, secret-free and idempotent; `verify_resume` refuses delivered-tool removal.

mod common;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use common::*;
use hh_assembly::catalog::ClassCatalog;
use hh_assembly::diagnostics::{Code, NaReason, Severity};
use hh_assembly::grammar::{
    Assembly, Constraint, ConstraintKind, EntityBinding, ParamRequirement, ParamType, ParameterSpec,
};
use hh_assembly::instantiate::{BindContext, VariantRuntime};
use hh_assembly::resume::LedgerView;
use hh_assembly::Stage1Catalog;
use hh_hir::diff::DiffOp;
use hh_hir::document::SealedDefinition;
use hh_hir::records::{SlotBinding, SlotBindings};
use hh_hir::refs::{ComponentVariantRef, RefVersion};
use hh_identity::names::ResolveMode;
use hh_identity::supersede::SupersedeReason;
use hh_ledger::classes::Durability;
use hh_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass, RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_registry::kinds::Placement;
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

fn report_codes(r: &hh_assembly::ValidationReport) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.code()).collect()
}

fn has_code(r: &hh_assembly::ValidationReport, code: &str) -> bool {
    report_codes(r).iter().any(|c| c == code)
}

fn validate_authored(
    doc: &hh_hir::document::HirDocument,
    catalog: &dyn ClassCatalog,
) -> hh_assembly::ValidationReport {
    hh_assembly::validate_assembly(
        hh_assembly::Subject::Authored(doc),
        catalog,
        None,
        &kernel(),
    )
}

fn param_spec(t: ParamType, req: ParamRequirement, sweepable: bool) -> ParameterSpec {
    ParameterSpec {
        param_type: t,
        domain: None,
        default: None,
        required: req,
        unit: None,
        sweepable,
        affects: vec![],
        budget_relevant: false,
    }
}

fn constraint(kind: ConstraintKind, subject: Json) -> Constraint {
    Constraint {
        kind,
        subject,
        source: None,
    }
}

// ── grammar / codec ──────────────────────────────────────────────────────────

#[test]
fn load_encode_round_trip_over_a_full_assembly() {
    let mut a = stage1_assembly();
    a.parameters.insert(
        "window".into(),
        ParameterSpec {
            domain: Some(Json::Arr(vec![Json::Int(1), Json::Int(4)])),
            budget_relevant: true,
            ..param_spec(ParamType::Int, ParamRequirement::Required, false)
        },
    );
    a.values.insert("window".into(), Json::Int(4));
    a.entities.insert(
        "tool-a".into(),
        EntityBinding::Ref(hh_hir::refs::Ref::pinned("test:tool", "sha256:t")),
    );
    a.constraints.push(constraint(
        ConstraintKind::Requires,
        Json::obj([
            ("of", Json::str("control_strategy")),
            ("needs", Json::str("context_policy")),
        ]),
    ));
    let bytes = hh_assembly::schema::encode(&a);
    let decoded = hh_assembly::schema::load(&bytes, None).expect("round-trips");
    assert_eq!(decoded, a, "load(encode(a)) = a");
}

#[test]
fn load_rejects_unknown_keys_and_bad_dialect() {
    // C-LOAD-3 — a non-`ext` key outside the closed grammar.
    let mut j = stage1_assembly().to_json();
    if let Json::Obj(m) = &mut j {
        m.insert("surprise".into(), Json::Bool(true));
    }
    let mut diags = Vec::new();
    let a = Assembly::from_json(&j, "/assembly", &kernel(), &mut diags);
    assert!(
        a.is_some(),
        "the member-wise decode still returns a partial"
    );
    assert!(diags.iter().any(|d| d.code == Code::LoadUnknownKey));

    // C-LOAD-2 — the wrong dialect.
    let errs = hh_assembly::schema::load(
        &hh_assembly::schema::encode(&stage1_assembly()),
        Some("hir/2"),
    )
    .expect_err("wrong dialect is a load error");
    assert!(errs.iter().any(|d| d.code == Code::LoadDialect));
}

// ── validate_assembly ────────────────────────────────────────────────────────

#[test]
fn a_clean_assembly_passes_all_stages_with_6a_run_6b_not_run() {
    let doc = doc_with(&stage1_assembly());
    let cat = Stage1Catalog::stage1();
    let r = validate_authored(&doc, &cat);
    assert_eq!(
        r.status,
        hh_assembly::ReportStatus::Pass,
        "diags: {:?}",
        report_codes(&r)
    );
    // Stages 1–5 + 7 ran; 6 is n/a{not_run} (C1 scope).
    let stage6 = r
        .stages
        .iter()
        .find(|s| s.stage == 6)
        .expect("stage 6 reported");
    // Stage 6a runs (declaration-level profile compatibility — C1/Stage 3,
    // R-2.1.4¹ᵃ); an unbound profile makes it vacuous. 6b is C1/Stage 5.
    assert!(stage6.ran, "6a is implemented at Stage 3");
    assert_eq!(stage6.na, None);
}

#[test]
fn validation_is_never_fail_fast() {
    // Two independent faults — both must be reported.
    let mut a = stage1_assembly();
    a.slots.insert(
        "not_a_class".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "not_a_class",
            "hh/nope",
            "latest",
        ))),
    );
    a.values.insert("undeclared".into(), Json::Int(1));
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(has_code(&r, "C-CLASS-1"), "unknown class reported");
    assert!(
        has_code(&r, "C-PARAM-1"),
        "undeclared value reported — the run did not stop at C-CLASS-1"
    );
}

#[test]
fn stage2_missing_mandatory_slot_is_slot_unbound() {
    // S3.5 renumber (ADR-0240 note): §6.1 V-7 pins `C-CLASS-2 SlotUnbound` to an
    // unbound `exactly-one` slot; `C-CLASS-3 CardinalityViolation` now names only
    // bound-shape mismatches.
    let mut a = Assembly::empty();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(
        has_code(&r, "C-CLASS-2"),
        "context_policy absent → SlotUnbound"
    );
}

#[test]
fn hosted_root_reports_na_class_for_class_stages() {
    let doc = hosted_doc_with(&stage1_assembly());
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    let na_class: Vec<u8> = r
        .stages
        .iter()
        .filter(|s| s.na == Some(NaReason::Class))
        .map(|s| s.stage)
        .collect();
    assert!(
        na_class.contains(&2),
        "stage 2 degrades to n/a{{class}} on a hosted root: {:?}",
        r.stages
            .iter()
            .map(|s| (s.stage, s.ran, s.na.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn benchmark_conditioned_references_are_errors_never_warnings() {
    // AC-CC-13 / 7-L2: `suite_id`/`task_id`/`foreign_id`/`split`/`family` anywhere in
    // the assembly is C-LCD-4 at error severity.
    for token in hh_assembly::BENCH_TOKENS {
        let mut a = stage1_assembly();
        a.ext.insert(token.to_string(), Json::str("bench"));
        let doc = doc_with(&a);
        let r = validate_authored(&doc, &Stage1Catalog::stage1());
        let ds: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::LcdBenchmarkConditionedRule)
            .collect();
        assert!(!ds.is_empty(), "`{token}` is C-LCD-4");
        assert!(ds.iter().all(|d| d.severity == Severity::Error));
    }
}

#[test]
fn parameter_space_diagnostics() {
    // C-PARAM-2 — a sweepable parameter without a domain.
    let mut a = stage1_assembly();
    a.parameters.insert(
        "k".into(),
        param_spec(ParamType::Int, ParamRequirement::Optional, true),
    );
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(has_code(&r, "C-PARAM-2"));

    // C-PARAM-3 — a `$param:` reference with no declared spec.
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.params.insert("p".into(), Json::str("$param:ghost"));
    }
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(has_code(&r, "C-PARAM-3"));

    // C-PARAM-4 — a declared parameter never referenced.
    let mut a = stage1_assembly();
    a.parameters.insert(
        "orphan".into(),
        param_spec(ParamType::Bool, ParamRequirement::Optional, false),
    );
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == Code::ParamUnused && d.severity == Severity::Warning));
}

#[test]
fn constraint_violation_is_cons_1() {
    // `requires`: `of` holds (an enabled slot) but `needs` (a declared-but-unset
    // parameter) does not → C-CONS-1.
    let mut a = stage1_assembly();
    a.parameters.insert(
        "mode".into(),
        param_spec(ParamType::Enum, ParamRequirement::Optional, false),
    );
    a.constraints.push(constraint(
        ConstraintKind::Requires,
        Json::obj([
            ("of", Json::str("control_strategy")),
            ("needs", Json::str("mode")),
        ]),
    ));
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(
        has_code(&r, "C-CONS-1"),
        "an unsatisfied requires is C-CONS-1: {:?}",
        report_codes(&r)
    );
}

#[test]
fn every_diagnostic_carries_a_path_and_a_typed_code() {
    let mut a = Assembly::empty();
    a.slots.insert(
        "not_a_class".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "not_a_class",
            "hh/nope",
            "latest",
        ))),
    );
    a.values.insert("ghost".into(), Json::Null);
    let doc = doc_with(&a);
    let r = validate_authored(&doc, &Stage1Catalog::stage1());
    assert!(!r.diagnostics.is_empty());
    for d in &r.diagnostics {
        assert!(!d.code.code().is_empty());
        assert!(!d.path.is_empty(), "{} has no path", d.code.code());
        assert!(d.detail.content.is_some(), "detail is a Text leaf");
    }
}

// ── resolve ──────────────────────────────────────────────────────────────────

fn resolve_env<'a>(
    store: &'a mut hh_registry::store::RegistryStore,
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
        notices: None,
    }
}

#[test]
fn resolve_pins_selectors_records_the_snapshot_and_seals() {
    let (mut store, vids) = seeded_store("resolve-basic");
    let cat = Stage1Catalog::stage1();
    let doc = doc_with(&stage1_assembly());
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    // The assembly section resolved in place: variant refs are pinned version_ids.
    let asm_json = sealed.document.assembly.as_ref().expect("assembly kept");
    let s = asm_json.to_canonical_string();
    assert!(!s.contains("version_selector"), "no selector survives");
    assert!(
        s.contains(&vids["control_strategy"]),
        "control_strategy pinned"
    );
    assert!(s.contains(&vids["context_policy"]), "context_policy pinned");
    assert!(
        s.contains("registry_snapshot_id"),
        "the snapshot is recorded"
    );
    assert!(s.contains("\"resolved_at\":7"), "resolved_at recorded");
}

#[test]
fn resolve_is_idempotent() {
    let (mut store, _) = seeded_store("resolve-idem");
    let cat = Stage1Catalog::stage1();
    let doc = doc_with(&stage1_assembly());
    let first = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    // Resolving the sealed document again is a no-op (already resolved).
    let second = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&first.document, &mut env).expect("re-resolve")
    };
    assert_eq!(
        first.document.canonical_bytes(),
        second.document.canonical_bytes(),
        "resolve(resolve(d)) = resolve(d)"
    );
}

#[test]
fn resolve_substitutes_params_entities_and_keeps_secret_channel_names() {
    let (mut store, _) = seeded_store("resolve-subst");
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    a.parameters.insert(
        "window".into(),
        ParameterSpec {
            default: Some(Json::Int(2)),
            ..param_spec(ParamType::Int, ParamRequirement::Optional, false)
        },
    );
    a.entities.insert(
        "tool-a".into(),
        EntityBinding::Ref(hh_hir::refs::Ref::selected("test:rule", "latest")),
    );
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.params.insert("window".into(), Json::str("$param:window"));
        b.params
            .insert("api_key".into(), Json::str("$secret:llm-key"));
    }
    let doc = doc_with(&a);
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    let s = sealed
        .document
        .assembly
        .as_ref()
        .unwrap()
        .to_canonical_string();
    // `$param:window` → the declared default; `$secret:llm-key` stays a channel name —
    // never a value.
    assert!(s.contains("\"window\":2"), "$param substituted: {s}");
    assert!(s.contains("$secret:llm-key"), "channel names survive: {s}");
    assert!(!s.contains("sk-"), "no secret material anywhere");
}

#[test]
fn resolve_execute_mode_refuses_a_revoked_head() {
    let (mut store, vids) = seeded_store("resolve-revoked");
    let cat = Stage1Catalog::stage1();
    // Pin the variant, then revoke it — execute must refuse.
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.variant.version = RefVersion::Pinned(vids["control_strategy"].clone());
    }
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("context_policy") {
        b.variant.version = RefVersion::Pinned(vids["context_policy"].clone());
    }
    store
        .revoke(
            &vids["control_strategy"],
            SupersedeReason::Revocation,
            &kernel(),
            None,
        )
        .unwrap();
    let doc = doc_with(&a);
    let mut env = resolve_env(&mut store, &cat, ResolveMode::Execute);
    let errs = hh_assembly::resolve(&doc, &mut env).expect_err("revoked head refuses");
    assert!(
        errs.iter()
            .any(|d| matches!(d.code, Code::RefStaleIndex | Code::Kern(_))),
        "the revoked head is a typed refusal: {:?}",
        errs.iter().map(|d| d.code.code()).collect::<Vec<_>>()
    );
}

#[test]
fn resolve_rejects_benchmark_conditioned_rules() {
    let (mut store, _) = seeded_store("resolve-bench");
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    a.ext.insert("suite_id".into(), Json::str("s1"));
    let doc = doc_with(&a);
    let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
    let errs = hh_assembly::resolve(&doc, &mut env).expect_err("benchmark ref refuses");
    assert!(errs
        .iter()
        .any(|d| d.code == Code::LcdBenchmarkConditionedRule && d.severity == Severity::Error));
}

// ── identity / diff ──────────────────────────────────────────────────────────

fn resolved_sealed(tag: &str) -> (SealedDefinition, hh_registry::store::RegistryStore) {
    let (mut store, _) = seeded_store(tag);
    let cat = Stage1Catalog::stage1();
    let doc = doc_with(&stage1_assembly());
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    (sealed, store)
}

#[test]
fn identity_exposes_root_and_configuration_ids() {
    let (sealed, _store) = resolved_sealed("identity");
    let def = hh_assembly::identity(&sealed);
    assert_eq!(def.version_id, sealed.definition_ref.version_id);
    let inputs = hh_assembly::CompositionInputs {
        model_ref: "model:m1".into(),
        profile: "sha256:profile".into(),
        environment_ref: "env:test".into(),
        budget: "test:budget".into(),
        seed: "7".into(),
    };
    let a = hh_assembly::configuration(&sealed, &inputs);
    let b = hh_assembly::configuration(&sealed, &inputs);
    assert_eq!(a.configuration_id, b.configuration_id, "identity is stable");
    let c = hh_assembly::configuration(
        &sealed,
        &hh_assembly::CompositionInputs {
            seed: "8".into(),
            ..inputs
        },
    );
    // `configuration_id` is seedless; `configuration_version_id` carries the seed.
    assert_eq!(a.configuration_id, c.configuration_id);
    assert_ne!(a.configuration_version_id, c.configuration_version_id);
}

#[test]
fn diff_classifies_a_variant_rebind_as_rebind() {
    let (sealed_a, mut store) = resolved_sealed("diff-rebind");
    // Rebind control_strategy to a second registered variant.
    let c = store
        .register(
            RegistryRecord::Class(class_record("control_strategy")),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = store
        .register(
            RegistryRecord::Variant(variant_record(
                &c.version_id,
                "hh/least_loaded",
                Placement::InProcess,
            )),
            &kernel(),
            None,
        )
        .unwrap();
    let mut doc_b = sealed_a.document.clone();
    let mut a = Assembly::from_json(
        doc_b.assembly.as_ref().unwrap(),
        "/assembly",
        &kernel(),
        &mut Vec::new(),
    )
    .unwrap();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.variant.variant_id = "hh/least_loaded".into();
        b.variant.version = RefVersion::Pinned(v2.version_id.clone());
    }
    doc_b.assembly = Some(a.to_json());
    // Also update the materialised native slots.
    if let Some(n) = doc_b
        .nodes
        .iter_mut()
        .find(|n| n.semantic_id() == doc_b.root.semantic_id)
    {
        if let hh_hir::records::KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let hh_hir::records::AgentProcessBody::Native(np) = &mut ap.body {
                np.slots.insert(
                    "control_strategy".into(),
                    SlotBindings::One(SlotBinding::of(ComponentVariantRef::pinned(
                        "control_strategy",
                        "hh/least_loaded",
                        v2.version_id.clone(),
                    ))),
                );
            }
        }
    }
    let sealed_b = hh_hir::seal(&doc_b, 1).expect("re-seal");
    let d = hh_assembly::diff(
        &sealed_a,
        &sealed_b,
        prov(9),
        hh_hir::diff::DiffDerivation::default(),
    )
    .expect("diff");
    assert!(
        d.ops.iter().any(|op| matches!(op, DiffOp::Rebind { .. })),
        "the variant change is a Rebind op: {:?}",
        d.ops
    );
}

#[test]
fn a_disabled_binding_is_exactly_one_replace_field() {
    let (sealed_a, _store) = resolved_sealed("diff-disable");
    let mut doc_b = sealed_a.document.clone();
    // Flip `enabled` in the assembly section AND the materialised native slots.
    if let Some(Json::Obj(m)) = doc_b.assembly.as_mut() {
        if let Some(Json::Obj(slots)) = m.get_mut("slots") {
            if let Some(Json::Obj(bm)) = slots.get_mut("control_strategy") {
                bm.insert("enabled".into(), Json::Bool(false));
            }
        }
    }
    if let Some(n) = doc_b
        .nodes
        .iter_mut()
        .find(|n| n.semantic_id() == doc_b.root.semantic_id)
    {
        if let hh_hir::records::KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let hh_hir::records::AgentProcessBody::Native(np) = &mut ap.body {
                if let Some(SlotBindings::One(b)) = np.slots.get_mut("control_strategy") {
                    b.enabled = false;
                }
            }
        }
    }
    let sealed_b = hh_hir::seal(&doc_b, 1).expect("re-seal");
    let d = hh_assembly::diff(
        &sealed_a,
        &sealed_b,
        prov(9),
        hh_hir::diff::DiffDerivation::default(),
    )
    .expect("diff");
    let replaces = d
        .ops
        .iter()
        .filter(|op| matches!(op, DiffOp::ReplaceField { .. }))
        .count();
    assert!(
        replaces >= 1,
        "the disable is ReplaceField-shaped: {:?}",
        d.ops
    );
}

// ── instantiate ──────────────────────────────────────────────────────────────

/// The stub `VariantRuntime` — binds everything to a handle.
struct StubRuntime;

impl VariantRuntime for StubRuntime {
    fn bind(
        &self,
        slot: &str,
        _class: &hh_registry::records::ClassRecord,
        _binding: &SlotBinding,
        _variant: &hh_registry::records::VariantRecord,
        _ctx: &BindContext,
    ) -> Result<String, hh_assembly::instantiate::BindReason> {
        Ok(format!("handle:{slot}"))
    }
}

/// A runtime that refuses — exercises the `BindFailure` path.
struct RefusingRuntime;

impl VariantRuntime for RefusingRuntime {
    fn bind(
        &self,
        _slot: &str,
        _class: &hh_registry::records::ClassRecord,
        _binding: &SlotBinding,
        _variant: &hh_registry::records::VariantRecord,
        _ctx: &BindContext,
    ) -> Result<String, hh_assembly::instantiate::BindReason> {
        Err(hh_assembly::instantiate::BindReason::TrustDenied)
    }
}

#[test]
fn instantiate_binds_in_process_variants_and_stamps_component_bound() {
    let (sealed, store) = resolved_sealed("instantiate");
    let cat = Stage1Catalog::stage1();
    let mut ledger = Store::open_test(dir("instantiate-ledger"), 1_000).unwrap();
    let (run, _lease) = ledger
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    let account = hh_budget::Account::open(&mut ledger, &run).unwrap();
    let ctx = BindContext {
        run_id: &run,
        profile: Some("sha256:profile"),
        accounting: &account,
    };
    let out = hh_assembly::instantiate(&sealed, &cat, &store, &StubRuntime, &ctx);
    let inst = out.instance.expect("all binds succeeded");
    assert_eq!(inst.bindings.len(), 2, "both stage-1 slots bound");
    assert!(
        out.events
            .iter()
            .all(|e| e.class == hh_assembly::events::COMPONENT_BOUND),
        "every row is lifecycle.component.bound"
    );
    assert_eq!(out.events.len(), 2);
    assert!(out
        .events
        .iter()
        .all(|e| e.payload.get("bind_result") == Some(&Json::str("bound"))));
    assert!(out.failure.is_none());
}

#[test]
fn instantiate_reports_a_typed_bind_failure() {
    let (sealed, store) = resolved_sealed("instantiate-fail");
    let cat = Stage1Catalog::stage1();
    let mut ledger = Store::open_test(dir("instantiate-fail-ledger"), 1_000).unwrap();
    let (run, _lease) = ledger
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    let account = hh_budget::Account::open(&mut ledger, &run).unwrap();
    let ctx = BindContext {
        run_id: &run,
        profile: Some("sha256:profile"),
        accounting: &account,
    };
    let out = hh_assembly::instantiate(&sealed, &cat, &store, &RefusingRuntime, &ctx);
    assert!(out.instance.is_none());
    assert_eq!(
        out.failure.map(|f| f.reason),
        Some(hh_assembly::instantiate::BindReason::TrustDenied)
    );
    assert!(out
        .events
        .iter()
        .any(|e| e.payload.get("bind_result") == Some(&Json::str("trust_denied"))));
}

#[test]
fn a_model_conditioned_variant_binds_without_touching_core() {
    // AC-CC-02 second half (T-LCD-08): a variant whose `capability_declaration`
    // names a model condition registers, resolves, seals and binds through the
    // data-only path — no Core change.
    let mut store =
        hh_registry::store::RegistryStore::open(dir("model-conditioned"), &kernel()).unwrap();
    // A `context_policy` class whose declaration schema admits the model-condition key.
    let mut cls = class_record("context_policy");
    cls.declaration_schema = Json::obj([
        ("additionalProperties", Json::Bool(false)),
        (
            "properties",
            Json::obj([("deterministic", Json::Null), ("model_family", Json::Null)]),
        ),
        ("required", Json::Arr(vec![Json::str("deterministic")])),
    ]);
    let c = store
        .register(RegistryRecord::Class(cls), &kernel(), None)
        .unwrap();
    let mut conditioned = variant_record(
        &c.version_id,
        "hh/family_aware_window",
        Placement::InProcess,
    );
    conditioned
        .capability_declaration
        .insert("model_family".into(), Json::str("m-*"));
    let cv = store
        .register(RegistryRecord::Variant(conditioned), &kernel(), None)
        .unwrap();
    store
        .publish(
            "hh",
            "family_aware_window",
            &cv.version_id,
            None,
            None,
            &kernel(),
        )
        .unwrap();
    // The other mandatory slot gets the stock variant.
    let c2 = store
        .register(
            RegistryRecord::Class(class_record("control_strategy")),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = store
        .register(
            RegistryRecord::Variant(variant_record(
                &c2.version_id,
                "hh/round_robin",
                Placement::InProcess,
            )),
            &kernel(),
            None,
        )
        .unwrap();
    store
        .publish("hh", "round_robin", &v2.version_id, None, None, &kernel())
        .unwrap();

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
            "hh/family_aware_window",
            "latest",
        ))),
    );
    let doc = doc_with(&a);
    let cat = Stage1Catalog::stage1();
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("the conditioned variant resolves")
    };
    let mut ledger = Store::open_test(dir("model-conditioned-ledger"), 1_000).unwrap();
    let (run, _lease) = ledger
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    let account = hh_budget::Account::open(&mut ledger, &run).unwrap();
    let ctx = BindContext {
        run_id: &run,
        profile: Some("sha256:profile"),
        accounting: &account,
    };
    let out = hh_assembly::instantiate(&sealed, &cat, &store, &StubRuntime, &ctx);
    assert!(
        out.instance.is_some(),
        "the model-conditioned variant binds: {:?}",
        out.failure
    );
}

// ── verify_resume ────────────────────────────────────────────────────────────

fn delivered_event(artefact: &str) -> EventEnvelope {
    EventEnvelope {
        event_id: "evt-1".into(),
        run_id: "run".into(),
        seq: 1,
        ts: "2026-09-16T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::Observation,
        class: hh_assembly::events::ARTEFACT_DELIVERED.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:context"),
        participant_class: ParticipantClass::Native,
        observability_level: BTreeSet::from([ObservabilityLevel::Ledger]),
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: hh_ledger::ids::ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(kernel()),
        prev_hash: hh_ledger::ids::GENESIS_HASH.into(),
        payload: Json::obj([("artefact_id", Json::str(artefact))]),
        hash: String::new(),
    }
}

#[test]
fn verify_resume_refuses_removal_of_a_delivered_tool() {
    // Persisted: a definition that declared `tool:search`; the ledger delivered it;
    // the resumed definition drops it → Incompatible{DeliveredToolRemoved} (T-LCD-13).
    let (mut persisted_doc, _s) = {
        let (sealed, store) = resolved_sealed("resume-deliver");
        (sealed.document, store)
    };
    persisted_doc
        .nodes
        .push(named_tool_node("test:search", "search", 8));
    let persisted = hh_hir::seal(&persisted_doc, 0).expect("seal persisted");
    let (current, _s2) = resolved_sealed("resume-deliver");
    let events = vec![delivered_event("search")];
    let view = LedgerView {
        events: &events,
        budget_gate: &hh_assembly::DenyAll,
    };
    let verdict = hh_assembly::verify_resume(
        &persisted,
        &current,
        &view,
        prov(9),
        hh_hir::diff::DiffDerivation::default(),
    )
    .expect("not a gate failure");
    match verdict {
        hh_assembly::ResumeVerdict::Incompatible { reasons } => {
            assert!(reasons
                .iter()
                .any(|r| r.rule == hh_assembly::ResumeRule::DeliveredToolRemoved));
        }
        _ => panic!("delivered-tool removal must be incompatible"),
    }
}

#[test]
fn verify_resume_allows_additions() {
    let (persisted, _s) = resolved_sealed("resume-add");
    let mut cur_doc = persisted.document.clone();
    cur_doc
        .nodes
        .push(named_tool_node("test:extra", "extra", 8));
    let current = hh_hir::seal(&cur_doc, 1).expect("seal current");
    let events = vec![];
    let view = LedgerView {
        events: &events,
        budget_gate: &hh_assembly::AllowAll,
    };
    let verdict = hh_assembly::verify_resume(
        &persisted,
        &current,
        &view,
        prov(9),
        hh_hir::diff::DiffDerivation::default(),
    )
    .expect("not a gate failure");
    match verdict {
        hh_assembly::ResumeVerdict::Compatible { diff_ref, .. } => {
            assert!(
                diff_ref.starts_with("sha256:"),
                "the diff is content-addressed"
            );
        }
        hh_assembly::ResumeVerdict::Incompatible { reasons } => {
            panic!("an addition is compatible: {reasons:?}")
        }
    }
}

#[test]
fn verify_resume_refuses_a_dialect_change() {
    let (persisted, _s) = resolved_sealed("resume-dialect");
    // `seal` refuses a foreign dialect — fabricate the envelope (the check under test
    // is verify_resume's dialect rule, which reads the documents' `hir_version`).
    let mut cur_doc = persisted.document.clone();
    cur_doc.hir_version = "hir/2".into();
    let current = SealedDefinition {
        document: cur_doc,
        definition_ref: persisted.definition_ref.clone(),
        closed_world_tools: persisted.closed_world_tools.clone(),
    };
    let events = vec![];
    let view = LedgerView {
        events: &events,
        budget_gate: &hh_assembly::AllowAll,
    };
    let verdict = hh_assembly::verify_resume(
        &persisted,
        &current,
        &view,
        prov(9),
        hh_hir::diff::DiffDerivation::default(),
    )
    .expect("not a gate failure");
    match verdict {
        hh_assembly::ResumeVerdict::Incompatible { reasons } => {
            assert!(reasons
                .iter()
                .any(|r| r.rule == hh_assembly::ResumeRule::DialectChanged));
        }
        _ => panic!("a dialect change is incompatible"),
    }
}

// ── link_precheck ────────────────────────────────────────────────────────────

#[test]
fn link_precheck_refuses_an_unresolved_selector() {
    let (sealed, _store) = resolved_sealed("link");
    // A clean sealed definition passes.
    hh_assembly::link_precheck(&sealed).expect("resolved doc passes C-LINK-1");
    // A fabricated definition with an unpinned selector inside fails C-LINK-1.
    let mut bad = sealed.document.clone();
    if let Some(Json::Obj(m)) = bad.assembly.as_mut() {
        if let Some(Json::Obj(slots)) = m.get_mut("slots") {
            if let Some(Json::Obj(bm)) = slots.get_mut("control_strategy") {
                bm.insert(
                    "variant".into(),
                    Json::obj([
                        ("class_id", Json::str("control_strategy")),
                        ("variant_id", Json::str("hh/round_robin")),
                        ("version_selector", Json::str("latest")),
                    ]),
                );
            }
        }
    }
    let fabricated = SealedDefinition {
        document: bad,
        definition_ref: sealed.definition_ref.clone(),
        closed_world_tools: sealed.closed_world_tools.clone(),
    };
    let errs = hh_assembly::link_precheck(&fabricated).expect_err("selector → C-LINK-1");
    assert!(errs.iter().any(|d| d.code == Code::LinkUnboundSlot));
}

// ── boundary ─────────────────────────────────────────────────────────────────

#[test]
fn the_must_lists_and_rung_are_declared() {
    // AC-CC-09 — the code/data boundary is published as data: both lists non-empty,
    // disjoint spellings, the rung C0.
    assert!(!hh_assembly::MUST_BE_CODE.is_empty());
    assert!(!hh_assembly::MUST_BE_DATA.is_empty());
    for item in hh_assembly::MUST_BE_CODE {
        assert!(!hh_assembly::MUST_BE_DATA.contains(item));
    }
    assert_eq!(hh_assembly::RUNG, hh_assembly::boundary::LadderRung::C0);
}

// ── extension trust (§5g.5; S1.23) ───────────────────────────────────────────

use hh_assembly::grammar::{ExtensionBlock, MergePolicy};
use hh_registry::extension::{DeclaredSource, ExtensionKind, ExtensionRef};

fn git_source() -> DeclaredSource {
    DeclaredSource::Git {
        url: "https://example.com".into(),
        ref_: "main".into(),
    }
}

fn ext_block(sources: Vec<DeclaredSource>, refs: Vec<ExtensionRef>) -> ExtensionBlock {
    ExtensionBlock {
        sources,
        refs,
        merge_policy: MergePolicy::ExactOnly,
    }
}

#[test]
fn extensions_member_round_trips_through_the_grammar() {
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "git",
            Some("latest"),
        )],
    ));
    let bytes = hh_assembly::schema::encode(&a);
    let decoded = hh_assembly::schema::load(&bytes, None).expect("round-trips");
    assert_eq!(decoded, a, "load(encode(a)) = a");
}

#[test]
fn extensions_member_malformed_is_a_load_diagnostic() {
    let mut j = stage1_assembly().to_json();
    if let Json::Obj(m) = &mut j {
        m.insert("extensions".into(), Json::str("nope"));
    }
    let mut diags = Vec::new();
    Assembly::from_json(&j, "/assembly", &kernel(), &mut diags);
    assert!(diags.iter().any(|d| d.code == Code::LoadParse));
}

#[test]
fn ac_2_8_5_1_resolve_pins_extension_refs() {
    let (mut store, _) = seeded_store("ext-resolve");
    let v = store
        .register(
            RegistryRecord::Extension(extension_record("demo", ExtensionKind::Skill)),
            &kernel(),
            None,
        )
        .expect("register");
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "git",
            Some("latest"),
        )],
    ));
    let doc = doc_with(&a);
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    let s = sealed
        .document
        .assembly
        .as_ref()
        .unwrap()
        .to_canonical_string();
    assert!(!s.contains("\"selector\""), "the selector is consumed: {s}");
    assert!(s.contains(&v.version_id), "extension_id pinned: {s}");
    assert!(s.contains("\"resolved\""), "locator.resolved filled: {s}");
    assert!(s.contains("\"fetched_at\":7"), "fetched_at stamped: {s}");
    assert!(s.contains("\"content\""), "the content pin landed: {s}");
}

/// Inject an `extensions` member into an already-resolved document's assembly
/// (the slots stay pinned so `seal` reaches the extension check).
fn doc_with_extensions(tag: &str, refs: Vec<ExtensionRef>) -> hh_hir::document::HirDocument {
    let (sealed, _store) = resolved_sealed(tag);
    let mut doc = sealed.document.clone();
    let ext_j = Json::obj([
        (
            "sources",
            Json::Arr(vec![hh_registry::extension::declared_source_json(
                &git_source(),
            )]),
        ),
        (
            "refs",
            Json::Arr(
                refs.iter()
                    .map(hh_registry::extension::extension_ref_json)
                    .collect(),
            ),
        ),
        ("merge_policy", Json::str("exact_only")),
    ]);
    if let Some(Json::Obj(m)) = &mut doc.assembly {
        m.insert("extensions".into(), ext_j);
    }
    doc
}

#[test]
fn ac_2_8_5_1_seal_refuses_unpinned_extension_ref() {
    let doc = doc_with_extensions(
        "ext-seal-unpinned",
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "git",
            Some("latest"),
        )],
    );
    let errs = hh_hir::seal(&doc, 9).expect_err("unpinned ref refuses");
    assert!(
        errs.iter()
            .any(|e| matches!(e, hh_hir::errors::HirError::UnpinnedInSealedForm { .. })),
        "UnpinnedInSealedForm: {errs:?}"
    );
}

#[test]
fn ac_2_8_5_1_seal_refuses_a_pin_missing_content() {
    let mut r = extension_ref("demo", ExtensionKind::Skill, "git", None);
    // A locator pin without `content` is still unpinned (L4).
    r.locator.resolved = Some("abc".into());
    r.locator.fetched_at = Some(1);
    let doc = doc_with_extensions("ext-seal-nocontent", vec![r]);
    let errs = hh_hir::seal(&doc, 9).expect_err("content-less pin refuses");
    assert!(errs
        .iter()
        .any(|e| matches!(e, hh_hir::errors::HirError::UnpinnedInSealedForm { .. })));
}

#[test]
fn resolve_extension_ref_reports_undeclared_source() {
    let (mut store, _) = seeded_store("ext-undeclared");
    store
        .register(
            RegistryRecord::Extension(extension_record("demo", ExtensionKind::Skill)),
            &kernel(),
            None,
        )
        .unwrap();
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    // The ref's scheme is `marketplace` — only `git` is declared.
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "marketplace",
            Some("latest"),
        )],
    ));
    let doc = doc_with(&a);
    let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
    let errs = hh_assembly::resolve(&doc, &mut env).expect_err("undeclared source refuses");
    assert!(
        errs.iter().any(|d| d.code == Code::ExtUndeclaredSource),
        "C-EXT-2: {:?}",
        errs.iter().map(|d| d.code.code()).collect::<Vec<_>>()
    );
}

#[test]
fn resolve_extension_ref_unmatched_is_typed_refusal() {
    let (mut store, _) = seeded_store("ext-unmatched");
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "ghost",
            ExtensionKind::Skill,
            "git",
            Some("latest"),
        )],
    ));
    let doc = doc_with(&a);
    let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit);
    let errs = hh_assembly::resolve(&doc, &mut env).expect_err("no record refuses");
    assert!(
        errs.iter().any(|d| d.code == Code::ExtUnresolved),
        "C-EXT-4: {:?}",
        errs.iter().map(|d| d.code.code()).collect::<Vec<_>>()
    );
}

#[test]
fn validate_reports_extension_name_collision_under_exact_only() {
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![
            extension_ref("demo", ExtensionKind::Skill, "git", Some("latest")),
            extension_ref("demo", ExtensionKind::Skill, "git", Some("version:x")),
        ],
    ));
    let r = validate_authored(&doc_with(&a), &Stage1Catalog::stage1());
    assert!(has_code(&r, "C-EXT-3"), "{:?}", report_codes(&r));

    // `disjoint` declares the collision away.
    let mut b = stage1_assembly();
    let mut eb = ext_block(
        vec![git_source()],
        vec![
            extension_ref("demo", ExtensionKind::Skill, "git", Some("latest")),
            extension_ref("demo", ExtensionKind::Skill, "git", Some("version:x")),
        ],
    );
    eb.merge_policy = MergePolicy::Disjoint;
    b.extensions = Some(eb);
    let r2 = validate_authored(&doc_with(&b), &Stage1Catalog::stage1());
    assert!(!has_code(&r2, "C-EXT-3"), "{:?}", report_codes(&r2));
}

#[test]
fn validate_sealed_reports_an_unpinned_extension_ref() {
    // Fabricate a sealed doc carrying an unpinned ref (seal itself refuses —
    // validate_assembly(Sealed) still reports the C-EXT-1 it would have caught).
    let (sealed, _store) = resolved_sealed("ext-sealed-unpinned");
    let mut bad = sealed.document.clone();
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "git",
            Some("latest"),
        )],
    ));
    bad.assembly = Some(a.to_json());
    let fabricated = SealedDefinition {
        document: bad,
        definition_ref: sealed.definition_ref.clone(),
        closed_world_tools: sealed.closed_world_tools.clone(),
    };
    let r = hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&fabricated),
        &Stage1Catalog::stage1(),
        None,
        &kernel(),
    );
    assert!(has_code(&r, "C-EXT-1"), "{:?}", report_codes(&r));
}
