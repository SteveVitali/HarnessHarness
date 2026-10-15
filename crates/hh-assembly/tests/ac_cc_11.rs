//! AC-CC-11 — the diagnostics corpus gate (spec §3.3.8; ticket R2.18;
//! ADR-0148). One committed fixture per *document-reachable* diagnostic code:
//! the fixture is rejected with exactly that code, a resolving path and the
//! correct `source_layer`; a planted multi-error document returns every
//! diagnostic; a fuzzed corpus yields no `C-INT-1`.
//!
//! The INDEX below is the honest code-reachability map over the closed
//! `Code::all()` table:
//! - `F` — a committed fixture (the driver runs the owning stage).
//! - `X` — minted only by a downstream consumer; the row names the owner
//!   crate whose suite covers emission.
//! - `U` — registered but *unemittable* today: the row is the honest account
//!   of why no document can produce it (no silent coverage claim).
//! - `N` — `C-INT-1`, the uncoded-rejection floor: asserted *absent* (minted
//!   only by the service layer's residual mapping — a fixture yielding it is
//!   a defect by definition).
//!
//! `C-KERN-*` rows are `X{hh-hir::validate}`: the assembly mirrors the
//! kernel's `HirError` set 1:1 through `kern_code` (an exhaustive match —
//! the drift control is compile-time); one end-to-end fixture here proves
//! the mirror path emits `C-KERN-*` diagnostics at stage 5.

mod common;

use std::collections::BTreeMap;

use common::*;
use hh_assembly::catalog::ClassCatalog;
use hh_assembly::compose::{compose, Layer};
use hh_assembly::diagnostics::{AssemblyDiagnostic, Code, Severity, Stage};
use hh_assembly::grammar::{
    Assembly, Constraint, ConstraintKind, ExtensionBlock, LayerProvenance, LayerSourceKind,
    MergePolicy, ParamRequirement, ParamType, ParameterSpec, ResolvedInfo,
};
use hh_assembly::Stage1Catalog;
use hh_hir::document::{HirDocument, SealedDefinition};
use hh_hir::records::{SlotBinding, SlotBindings};
use hh_hir::refs::{ComponentVariantRef, RefVersion};
use hh_identity::names::ResolveMode;
use hh_registry::extension::{DeclaredSource, ExtensionKind};
use hh_registry::kinds::Placement;
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

// ── the coverage table ──────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Cov {
    /// A committed fixture (`build` runs the owning stage).
    F,
    /// Minted only by a downstream consumer — the row names the owner.
    X(&'static str),
    /// Registered but unemittable today — the honest reason.
    U(&'static str),
    /// The `C-INT-1` floor — asserted absent, never fixture-emitted.
    N(&'static str),
}

struct Row {
    code: &'static str,
    cov: Cov,
    build: Option<fn() -> Vec<AssemblyDiagnostic>>,
    stage: Option<Stage>,
    severity: Severity,
    /// The resolving-path assertion: the diagnostic's `path` must start here.
    path_prefix: &'static str,
    source_layer: Option<Option<&'static str>>,
    /// `exact` — every *error*-severity diagnostic carries this code (the
    /// AC's "rejected with exactly that code"). Companion warnings/info are
    /// tolerated: they never reject.
    exact: bool,
    /// Named error-severity companions the fixture legitimately carries —
    /// one fault surfacing at two stages is the never-fail-fast contract,
    /// never a masked rejection (e.g. a missing mandatory slot is `C-CLASS-2`
    /// at stage 2 and `C-KERN-SchemaViolation` at stage 5).
    also: &'static [&'static str],
}

#[allow(clippy::too_many_arguments)]
const fn f(
    code: &'static str,
    build: fn() -> Vec<AssemblyDiagnostic>,
    stage: Stage,
    severity: Severity,
    path_prefix: &'static str,
    source_layer: Option<&'static str>,
    exact: bool,
    also: &'static [&'static str],
) -> Row {
    Row {
        code,
        cov: Cov::F,
        build: Some(build),
        stage: Some(stage),
        severity,
        path_prefix,
        source_layer: Some(source_layer),
        exact,
        also,
    }
}

const fn x(code: &'static str, owner: &'static str) -> Row {
    Row {
        code,
        cov: Cov::X(owner),
        build: None,
        stage: None,
        severity: Severity::Error,
        path_prefix: "",
        source_layer: None,
        exact: false,
        also: &[],
    }
}

const fn u(code: &'static str, reason: &'static str) -> Row {
    Row {
        code,
        cov: Cov::U(reason),
        build: None,
        stage: None,
        severity: Severity::Error,
        path_prefix: "",
        source_layer: None,
        exact: false,
        also: &[],
    }
}

const fn n(code: &'static str, owner: &'static str) -> Row {
    Row {
        code,
        cov: Cov::N(owner),
        build: None,
        stage: None,
        severity: Severity::Error,
        path_prefix: "",
        source_layer: None,
        exact: false,
        also: &[],
    }
}

/// The corpus INDEX — one row per code in the closed table.
fn index() -> BTreeMap<&'static str, Row> {
    let mut m = BTreeMap::new();
    let mut put = |r: Row| {
        assert!(m.insert(r.code, r).is_none(), "duplicate INDEX row");
    };
    // ── load (the member-wise decode — Stage::Desugar at `load`, the raw
    // codec at `Assembly::from_json`). ────────────────────────────────────
    put(f(
        "C-LOAD-1",
        f_load_parse,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(f(
        "C-LOAD-2",
        f_load_dialect,
        Stage::Desugar,
        Severity::Error,
        "/assembly",
        None,
        true,
        &[],
    ));
    put(f(
        "C-LOAD-3",
        f_load_unknown_key,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/surprise",
        None,
        true,
        &[],
    ));
    // ── compose. ────────────────────────────────────────────────────────────
    put(f(
        "C-COMP-1",
        f_comp_authority,
        Stage::Compose,
        Severity::Error,
        "/assembly/constraints",
        Some("exp-1"),
        true,
        &[],
    ));
    put(f(
        "C-COMP-2",
        f_comp_conflict,
        Stage::Compose,
        Severity::Error,
        "/values",
        Some("project"),
        true,
        &[],
    ));
    put(f(
        "C-COMP-3",
        f_comp_provenance,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/constraints",
        None,
        true,
        &[],
    ));
    put(f(
        "C-COMP-4",
        f_comp_forbidden,
        Stage::Compose,
        Severity::Error,
        "/assembly",
        Some("user"),
        true,
        &[],
    ));
    // ── ref resolution (resolve). ───────────────────────────────────────────
    put(f(
        "C-REF-1",
        f_ref_unresolved,
        Stage::Resolve,
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(x("C-REF-2", "ResolveOutcome::Ambiguous has no producer in the current NameIndex — the mapping waits on the resolver's arrival"));
    put(x(
        "C-REF-3",
        "RegistryError::NameCollision is a publish-time error — assembly resolve never publishes",
    ));
    put(f(
        "C-REF-4",
        f_ref_stale,
        Stage::Resolve,
        Severity::Error,
        "/assembly/slots",
        None,
        false,
        &[],
    ));
    put(f(
        "C-REF-5",
        f_ref_deny_noop,
        Stage::Resolve,
        Severity::Info,
        "/assembly/slots",
        None,
        false,
        &[],
    ));
    put(x(
        "C-REF-6",
        "hh-lab::assembly::drift — minted on snapshot-drift reads at the service layer",
    ));
    // ── class conformance (stage 2 / resolve). ─────────────────────────────
    put(f(
        "C-CLASS-1",
        f_class_unknown,
        Stage::Validate(2),
        Severity::Error,
        "/assembly/slots",
        None,
        false,
        &[],
    ));
    put(f(
        "C-CLASS-2",
        f_class_slot_unbound,
        Stage::Validate(2),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &["C-KERN-SchemaViolation"],
    ));
    put(f(
        "C-CLASS-3",
        f_class_cardinality,
        Stage::Validate(2),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(f(
        "C-CLASS-4",
        f_class_contract,
        Stage::Validate(2),
        Severity::Error,
        "/assembly",
        None,
        true,
        &[],
    ));
    put(f(
        "C-CLASS-5",
        f_class_inputs,
        Stage::Validate(2),
        Severity::Error,
        "/assembly",
        None,
        true,
        &[],
    ));
    put(f(
        "C-CLASS-6",
        f_class_hosted,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/slots",
        None,
        false,
        &[],
    ));
    put(u("C-CLASS-7", "register-time `dialect_covers` refuses the incompatible record before a document stage can mint the code"));
    put(f(
        "C-CLASS-8",
        f_class_mismatch,
        Stage::Validate(2),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    // ── the parameter space (stage 3). ─────────────────────────────────────
    put(f(
        "C-PARAM-1",
        f_param_unknown,
        Stage::Validate(3),
        Severity::Error,
        "/assembly/values",
        None,
        true,
        &[],
    ));
    put(f(
        "C-PARAM-2",
        f_param_domain,
        Stage::Validate(3),
        Severity::Error,
        "/assembly/parameters",
        None,
        true,
        &[],
    ));
    put(f(
        "C-PARAM-3",
        f_param_undeclared,
        Stage::Validate(3),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(f(
        "C-PARAM-4",
        f_param_unused,
        Stage::Validate(3),
        Severity::Warning,
        "/assembly/parameters",
        None,
        false,
        &[],
    ));
    put(f(
        "C-PARAM-5",
        f_param_defaulted,
        Stage::Validate(3),
        Severity::Warning,
        "/assembly/parameters",
        None,
        false,
        &[],
    ));
    put(f(
        "C-PARAM-6",
        f_param_hosted,
        Stage::Validate(3),
        Severity::Error,
        "/assembly/parameters",
        None,
        false,
        &[],
    ));
    // ── constraints / profiles / LCD. ──────────────────────────────────────
    put(f(
        "C-CONS-1",
        f_cons,
        Stage::Validate(4),
        Severity::Error,
        "/assembly/constraints",
        None,
        true,
        &[],
    ));
    put(f(
        "C-PROF-1",
        f_prof_incompatible,
        Stage::Validate(6),
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(x("C-PROF-2", "hh-compiler::lower + hh-lab::assembly::service — the expressibility refusal is minted downstream"));
    put(u("C-PROF-3", "the §3.3.2 pin-across-profile-factors rule has no landed check — a registered code awaiting its emitter"));
    put(u("C-LCD-1", "no C0 `EdgeKind` spelling contains `host` — the 7-L4 check binds when a hosting edge kind lands"));
    put(f(
        "C-LCD-2",
        f_lcd_surface,
        Stage::Validate(7),
        Severity::Error,
        "/",
        None,
        false,
        &[],
    ));
    put(f(
        "C-LCD-3",
        f_lcd_inherit,
        Stage::Validate(7),
        Severity::Error,
        "/",
        None,
        true,
        &[],
    ));
    put(f(
        "C-LCD-4",
        f_lcd_bench,
        Stage::Validate(7),
        Severity::Error,
        "/assembly/ext",
        None,
        true,
        &[],
    ));
    put(f(
        "C-LCD-5",
        f_lcd_registry_op,
        Stage::Validate(7),
        Severity::Error,
        "/entities",
        None,
        true,
        &[],
    ));
    // ── locator / trust / secrets / extensions. ────────────────────────────
    put(u(
        "C-LOC-1",
        "no emitter — registered for the locator-class refusal (C1+ source locators)",
    ));
    put(u("C-TRUST-1", "no emitter — `instantiate`'s `BindReason::TrustDenied` is a bind-failure spelling, not this diagnostic"));
    put(f(
        "C-SEC-1",
        f_sec_inline,
        Stage::Resolve,
        Severity::Error,
        "/assembly/values",
        None,
        true,
        &[],
    ));
    put(f(
        "C-EXT-1",
        f_ext_unpinned,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/extensions",
        None,
        true,
        &["C-LOAD-1"],
    ));
    put(f(
        "C-EXT-2",
        f_ext_undeclared,
        Stage::Resolve,
        Severity::Error,
        "/assembly/extensions",
        None,
        true,
        &[],
    ));
    put(f(
        "C-EXT-3",
        f_ext_collision,
        Stage::Validate(1),
        Severity::Error,
        "/assembly/extensions",
        None,
        true,
        &[],
    ));
    put(f(
        "C-EXT-4",
        f_ext_unresolved,
        Stage::Resolve,
        Severity::Error,
        "/assembly/extensions",
        None,
        true,
        &[],
    ));
    // ── link / lower / seal / resume — the consumers' stages. ─────────────
    put(f(
        "C-LINK-1",
        f_link_unbound,
        Stage::Link,
        Severity::Error,
        "/assembly/slots",
        None,
        true,
        &[],
    ));
    put(x(
        "C-LINK-2",
        "hh-compiler::link + hh-lab::assembly::service",
    ));
    put(x(
        "C-LINK-3",
        "hh-compiler::link + hh-lab::assembly::service",
    ));
    put(x(
        "C-LINK-4",
        "hh-compiler::link + hh-lab::assembly::service",
    ));
    put(x(
        "C-LINK-5",
        "hh-compiler::link + hh-lab::assembly::service",
    ));
    put(x("C-LINK-6", "hh-compiler::link"));
    put(x("C-LINK-7", "hh-compiler::link"));
    put(u("C-LINK-8", "`CompileError::AmbiguousSelector` exists; the diagnostic code is never minted — the refusal reports via the error kind"));
    put(u(
        "C-LINK-9",
        "`CompileError::DialectNarrowingUndeclared` exists; the diagnostic code is never minted",
    ));
    put(x(
        "C-PLAN-1",
        "hh-lab::assembly::service — the plan-stage refusal is minted at the service boundary",
    ));
    put(x("C-PLAN-2", "hh-lab::assembly::service"));
    put(u(
        "C-SEAL-1",
        "`CompileError::NonCanonicalInput` exists; the diagnostic code is never minted",
    ));
    put(u("C-RES-1", "a results-plane status spelling; no emitter"));
    put(n(
        "C-INT-1",
        "hh-lab::assembly::service — the residual floor; a fixture yielding it is a defect",
    ));
    // ── C-KERN-* — the 1:1 `HirError` mirror (ADR-0148). `kern_code`'s
    // exhaustive match is the compile-time drift control; per-variant
    // document fixtures belong to the kernel's own validate corpus. One
    // end-to-end fixture proves the mirror path (stage 5 → `C-KERN-*`). ────
    for v in hh_assembly::diagnostics::KERN_VARIANTS {
        let code: &'static str = Box::leak(format!("C-KERN-{v}").into_boxed_str());
        if *v == "UnresolvedRef" {
            put(f(
                code,
                f_kern_e2e,
                Stage::Validate(5),
                Severity::Error,
                "/",
                None,
                true,
                &[],
            ));
        } else {
            put(x(code, "hh-hir::validate — emitted through stage 5 via `kern_code`; per-variant fixtures live with the kernel corpus"));
        }
    }
    m
}

// ── shared drivers ──────────────────────────────────────────────────────────

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

fn validate_authored(doc: &HirDocument, catalog: &dyn ClassCatalog) -> Vec<AssemblyDiagnostic> {
    hh_assembly::validate_assembly(
        hh_assembly::Subject::Authored(doc),
        catalog,
        None,
        &kernel(),
    )
    .diagnostics
}

fn resolve_env<'a>(
    store: &'a mut hh_registry::store::RegistryStore,
    catalog: &'a dyn ClassCatalog,
    mode: ResolveMode,
    notices: Option<&'a mut Vec<AssemblyDiagnostic>>,
) -> hh_assembly::resolve::ResolveEnv<'a> {
    hh_assembly::resolve::ResolveEnv {
        registry: store,
        catalog,
        snapshot_id: None,
        mode,
        registrar: kernel(),
        resolved_at: 7,
        notices,
    }
}

fn resolve_errs(
    doc: &HirDocument,
    store: &mut hh_registry::store::RegistryStore,
) -> Vec<AssemblyDiagnostic> {
    let cat = Stage1Catalog::stage1();
    let mut env = resolve_env(store, &cat, ResolveMode::Audit, None);
    hh_assembly::resolve(doc, &mut env).expect_err("the fixture must refuse")
}

fn git_source() -> DeclaredSource {
    DeclaredSource::Git {
        url: "https://example.com".into(),
        ref_: "main".into(),
    }
}

fn ext_block(
    sources: Vec<DeclaredSource>,
    refs: Vec<hh_registry::extension::ExtensionRef>,
) -> ExtensionBlock {
    ExtensionBlock {
        sources,
        refs,
        merge_policy: MergePolicy::ExactOnly,
    }
}

fn resolved_sealed(tag: &str) -> (SealedDefinition, hh_registry::store::RegistryStore) {
    let (mut store, _vids) = seeded_store(tag);
    let cat = Stage1Catalog::stage1();
    let doc = doc_with(&stage1_assembly());
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit, None);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    (sealed, store)
}

// ── fixture builders — one per document-reachable code ──────────────────────

fn f_load_parse() -> Vec<AssemblyDiagnostic> {
    let mut j = stage1_assembly().to_json();
    if let Json::Obj(m) = &mut j {
        m.insert("slots".into(), Json::Int(7));
    }
    let mut diags = Vec::new();
    Assembly::from_json(&j, "/assembly", &kernel(), &mut diags);
    diags
}

fn f_load_dialect() -> Vec<AssemblyDiagnostic> {
    hh_assembly::schema::load(
        &hh_assembly::schema::encode(&stage1_assembly()),
        Some("hir/2"),
    )
    .expect_err("wrong dialect refuses")
}

fn f_load_unknown_key() -> Vec<AssemblyDiagnostic> {
    let mut j = stage1_assembly().to_json();
    if let Json::Obj(m) = &mut j {
        m.insert("surprise".into(), Json::Bool(true));
    }
    let mut diags = Vec::new();
    Assembly::from_json(&j, "/assembly", &kernel(), &mut diags);
    diags
}

fn f_comp_authority() -> Vec<AssemblyDiagnostic> {
    let mut caps = Assembly::empty();
    caps.constraints.push(cap("budget", "definition"));
    let mut experiment = Assembly::empty();
    experiment.constraints.push(cap("budget", "kernel"));
    compose(
        &[
            layer(LayerSourceKind::Organisation, "secops", 50, caps),
            layer(LayerSourceKind::Experiment, "exp-1", 1, experiment),
        ],
        &kernel(),
    )
    .expect_err("widening refuses")
}

fn f_comp_conflict() -> Vec<AssemblyDiagnostic> {
    let mut a = Assembly::empty();
    a.values.insert("temperature".into(), Json::Int(0));
    let mut b = Assembly::empty();
    b.values.insert("temperature".into(), Json::Int(1));
    compose(
        &[
            layer(LayerSourceKind::User, "user", 5, a),
            layer(LayerSourceKind::Project, "project", 5, b),
        ],
        &kernel(),
    )
    .expect_err("equal-precedence disagreement refuses")
}

fn f_comp_forbidden() -> Vec<AssemblyDiagnostic> {
    let mut authored = Assembly::empty();
    authored.resolved = Some(ResolvedInfo {
        registry_snapshot_id: "sha256:snap".into(),
        resolved_at: 7,
    });
    compose(
        &[layer(LayerSourceKind::User, "user", 5, authored)],
        &kernel(),
    )
    .expect_err("authoring `resolved` refuses")
}

fn f_comp_provenance() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.layers = Some(vec![LayerProvenance {
        source_kind: LayerSourceKind::User,
        id: "user".into(),
        version: "1".into(),
        precedence: 5,
    }]);
    a.constraints.push(cap("budget", "definition"));
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_ref_unresolved() -> Vec<AssemblyDiagnostic> {
    let (mut store, _vids) = seeded_store("cc11-ref-unresolved");
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.variant = ComponentVariantRef::selected("control_strategy", "hh/ghost", "latest");
    }
    resolve_errs(&doc_with(&a), &mut store)
}

fn f_ref_stale() -> Vec<AssemblyDiagnostic> {
    let (mut store, vids) = seeded_store("cc11-ref-stale");
    let cat = Stage1Catalog::stage1();
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
            hh_identity::supersede::SupersedeReason::Revocation,
            &kernel(),
            None,
        )
        .unwrap();
    let doc = doc_with(&a);
    let mut env = resolve_env(&mut store, &cat, ResolveMode::Execute, None);
    hh_assembly::resolve(&doc, &mut env).expect_err("a revoked head refuses")
}

fn f_ref_deny_noop() -> Vec<AssemblyDiagnostic> {
    let (mut store, _vids) = seeded_store("cc11-ref-deny");
    let cat = Stage1Catalog::stage1();
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.variant = ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "deny:sha256:never-minted",
        );
    }
    let doc = doc_with(&a);
    let mut notices = Vec::new();
    {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit, Some(&mut notices));
        hh_assembly::resolve(&doc, &mut env).expect("a no-op deny-list still resolves");
    }
    notices
}

fn f_class_unknown() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.slots.insert(
        "not_a_class".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "not_a_class",
            "hh/nope",
            "latest",
        ))),
    );
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_class_slot_unbound() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.slots.remove("context_policy");
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_class_cardinality() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    let one = SlotBinding::of(ComponentVariantRef::selected(
        "control_strategy",
        "hh/round_robin",
        "latest",
    ));
    let two = SlotBinding::of(ComponentVariantRef::selected(
        "control_strategy",
        "hh/round_robin",
        "latest",
    ));
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::Many(vec![one, two]),
    );
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

/// A catalog returning hand-shaped records — lets a malformed contract reach
/// stage 2 (register-time checks would refuse it at the store boundary).
struct MapCatalog(BTreeMap<String, hh_registry::records::ClassRecord>);

impl ClassCatalog for MapCatalog {
    fn class(&self, class_id: &str) -> Option<hh_registry::records::ClassRecord> {
        self.0.get(class_id).cloned()
    }
    fn class_ids(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
    fn variant(&self, _version_id: &str) -> Option<hh_registry::records::VariantRecord> {
        None
    }
}

fn f_class_contract() -> Vec<AssemblyDiagnostic> {
    let mut broken = class_record("control_strategy");
    broken.contract[0].invariants = vec![];
    let cat = MapCatalog(BTreeMap::from([
        ("control_strategy".to_string(), broken),
        ("context_policy".to_string(), class_record("context_policy")),
    ]));
    validate_authored(&doc_with(&stage1_assembly()), &cat)
}

fn f_class_inputs() -> Vec<AssemblyDiagnostic> {
    let mut broken = class_record("control_strategy");
    broken.required_inputs.remove("ModelProfile");
    let cat = MapCatalog(BTreeMap::from([
        ("control_strategy".to_string(), broken),
        ("context_policy".to_string(), class_record("context_policy")),
    ]));
    validate_authored(&doc_with(&stage1_assembly()), &cat)
}

fn f_class_hosted() -> Vec<AssemblyDiagnostic> {
    let mut a = Assembly::empty();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    validate_authored(&hosted_doc_with(&a), &Stage1Catalog::stage1())
}

fn f_class_mismatch() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.variant = ComponentVariantRef::selected("context_policy", "hh/round_robin", "latest");
    }
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_unknown() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.values.insert("undeclared".into(), Json::Int(1));
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_domain() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.parameters.insert(
        "k".into(),
        param_spec(ParamType::Int, ParamRequirement::Optional, true),
    );
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_undeclared() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.params.insert("p".into(), Json::str("$param:ghost"));
    }
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_unused() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.parameters.insert(
        "orphan".into(),
        param_spec(ParamType::Bool, ParamRequirement::Optional, false),
    );
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_defaulted() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.parameters.insert(
        "k".into(),
        ParameterSpec {
            default: Some(Json::Int(0)),
            budget_relevant: true,
            ..param_spec(ParamType::Int, ParamRequirement::Optional, false)
        },
    );
    // The parameter must be *used* (else `C-PARAM-4` masks the finding).
    if let Some(SlotBindings::One(b)) = a.slots.get_mut("control_strategy") {
        b.params.insert("k".into(), Json::str("$param:k"));
    }
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_param_hosted() -> Vec<AssemblyDiagnostic> {
    let mut a = Assembly::empty();
    a.parameters.insert(
        "k".into(),
        ParameterSpec {
            affects: vec!["control_strategy".to_string()],
            ..param_spec(ParamType::Int, ParamRequirement::Optional, false)
        },
    );
    validate_authored(&hosted_doc_with(&a), &Stage1Catalog::stage1())
}

fn f_cons() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.parameters.insert(
        "mode".into(),
        ParameterSpec {
            domain: Some(Json::Arr(vec![Json::str("a"), Json::str("b")])),
            ..param_spec(ParamType::Enum, ParamRequirement::Optional, false)
        },
    );
    a.constraints.push(Constraint {
        kind: ConstraintKind::Requires,
        subject: Json::obj([
            ("of", Json::str("control_strategy")),
            ("needs", Json::str("mode")),
        ]),
        source: None,
    });
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

/// A `ProfileView` over raw record JSON (stage 6a's read seam).
struct JsonProfiles(BTreeMap<String, Json>);

impl hh_assembly::validate::ProfileView for JsonProfiles {
    fn profile(&self, coordinate: &str) -> Option<Json> {
        self.0.get(coordinate).cloned()
    }
}

fn f_prof_incompatible() -> Vec<AssemblyDiagnostic> {
    // A variant requiring a capability the bound profile does not declare —
    // C-PROF-1 (declaration-level compatibility, stage 6a).
    let (mut store, _vids) = seeded_store("cc11-prof");
    let cat = Stage1Catalog::stage1();
    let c = store
        .register(
            RegistryRecord::Class(class_record("control_strategy")),
            &kernel(),
            None,
        )
        .expect("register class");
    let mut v = variant_record(&c.version_id, "hh/cap_test", Placement::InProcess);
    v.capability_declaration
        .insert("deterministic".into(), Json::str("required"));
    let vv = store
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .expect("register variant");
    store
        .publish("hh", "cap_test", &vv.version_id, None, None, &kernel())
        .expect("publish");

    let mut a = stage1_assembly();
    a.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/cap_test",
            "latest",
        ))),
    );
    a.profile_binding = hh_assembly::grammar::ProfileBinding::Pinned(hh_hir::refs::ProfileRef {
        profile: "sha256:profile".into(),
        pinned: true,
    });
    let doc = doc_with(&a);
    let sealed = {
        let mut env = resolve_env(&mut store, &cat, ResolveMode::Audit, None);
        hh_assembly::resolve(&doc, &mut env).expect("resolve")
    };
    let regcat = hh_assembly::catalog::RegistryCatalog::new(&store, None);
    let mut no_caps = BTreeMap::new();
    no_caps.insert(
        "sha256:profile".to_string(),
        Json::obj([("capabilities", Json::obj([]))]),
    );
    let profiles = JsonProfiles(no_caps);
    hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&sealed),
        &regcat,
        Some(&profiles),
        &kernel(),
    )
    .diagnostics
}

fn f_lcd_surface() -> Vec<AssemblyDiagnostic> {
    // A semantic-projection member literally named `version` — identity
    // content must be free of surface/provenance/version members (7-T10).
    let mut doc = doc_with(&stage1_assembly());
    let mut n = tool_node("test:tool", 5);
    if let hh_hir::records::KindRecord::ToolCapability(t) = &mut n.semantic {
        t.input_schema = Json::obj([("version", Json::Int(1))]);
    }
    doc.nodes.push(n);
    validate_authored(&doc, &Stage1Catalog::stage1())
}

fn f_lcd_inherit() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.ext.insert("inherits".into(), Json::Bool(true));
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_lcd_bench() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.ext.insert("suite_id".into(), Json::str("bench"));
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_lcd_registry_op() -> Vec<AssemblyDiagnostic> {
    let mut doc = doc_with(&stage1_assembly());
    let mut n = tool_node("test:regtool", 5);
    if let hh_hir::records::KindRecord::ToolCapability(t) = &mut n.semantic {
        t.source = Json::obj([
            ("kind", Json::str("registry")),
            ("operation", Json::str("lab.registry.query")),
        ]);
    }
    doc.nodes.push(n);
    validate_authored(&doc, &Stage1Catalog::stage1())
}

fn f_sec_inline() -> Vec<AssemblyDiagnostic> {
    let (mut store, _vids) = seeded_store("cc11-sec");
    let mut a = stage1_assembly();
    a.values.insert("api_key".into(), Json::str("sk-inline"));
    resolve_errs(&doc_with(&a), &mut store)
}

fn f_ext_unpinned() -> Vec<AssemblyDiagnostic> {
    // A fabricated sealed doc carrying an unpinned extension ref — the
    // C-EXT-1 a `validate_assembly(Sealed)` reports. The extensions block is
    // injected into the *resolved* assembly JSON so the pinned slots survive.
    let (sealed, _store) = resolved_sealed("cc11-ext-unpinned");
    let mut bad = sealed.document.clone();
    let ext_j = Json::obj([
        (
            "sources",
            Json::Arr(vec![hh_registry::extension::declared_source_json(
                &git_source(),
            )]),
        ),
        (
            "refs",
            Json::Arr(vec![hh_registry::extension::extension_ref_json(
                &extension_ref("demo", ExtensionKind::Skill, "git", Some("latest")),
            )]),
        ),
        ("merge_policy", Json::str("exact_only")),
    ]);
    if let Some(Json::Obj(m)) = &mut bad.assembly {
        m.insert("extensions".into(), ext_j);
    }
    let fabricated = SealedDefinition {
        document: bad,
        definition_ref: sealed.definition_ref.clone(),
        closed_world_tools: sealed.closed_world_tools.clone(),
    };
    hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&fabricated),
        &Stage1Catalog::stage1(),
        None,
        &kernel(),
    )
    .diagnostics
}

fn f_ext_undeclared() -> Vec<AssemblyDiagnostic> {
    let (mut store, _vids) = seeded_store("cc11-ext-undeclared");
    store
        .register(
            RegistryRecord::Extension(extension_record("demo", ExtensionKind::Skill)),
            &kernel(),
            None,
        )
        .unwrap();
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![extension_ref(
            "demo",
            ExtensionKind::Skill,
            "marketplace",
            Some("latest"),
        )],
    ));
    resolve_errs(&doc_with(&a), &mut store)
}

fn f_ext_collision() -> Vec<AssemblyDiagnostic> {
    let mut a = stage1_assembly();
    a.extensions = Some(ext_block(
        vec![git_source()],
        vec![
            extension_ref("demo", ExtensionKind::Skill, "git", Some("latest")),
            extension_ref("demo", ExtensionKind::Skill, "git", Some("version:x")),
        ],
    ));
    validate_authored(&doc_with(&a), &Stage1Catalog::stage1())
}

fn f_ext_unresolved() -> Vec<AssemblyDiagnostic> {
    let (mut store, _vids) = seeded_store("cc11-ext-unresolved");
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
    resolve_errs(&doc_with(&a), &mut store)
}

fn f_link_unbound() -> Vec<AssemblyDiagnostic> {
    // A fabricated sealed doc carrying a surviving `version_selector` — the
    // DF-S1.9-3 refusal `link_precheck` mints (`link` never re-resolves).
    let (sealed, _store) = resolved_sealed("cc11-link");
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
    hh_assembly::link_precheck(&fabricated).expect_err("selector → C-LINK-1")
}

fn f_kern_e2e() -> Vec<AssemblyDiagnostic> {
    // The C-KERN mirror end-to-end: a document whose `budget.accounting`
    // ref names no node — the kernel's stage-5 `validate` mints the
    // `HirError`, assembly surfaces it as `C-KERN-UnresolvedRef`.
    let mut doc = doc_with(&stage1_assembly());
    for n in doc.nodes.iter_mut() {
        if let hh_hir::records::KindRecord::Budget(b) = &mut n.semantic {
            b.accounting = sel("test:ghost");
        }
    }
    validate_authored(&doc, &Stage1Catalog::stage1())
}

// ── the gate ────────────────────────────────────────────────────────────────

#[test]
fn index_covers_the_closed_code_table() {
    // The drift leg: `Code::all()` and the INDEX describe the same set — a
    // code added to the taxonomy without an INDEX row fails here, and a
    // stale row naming a removed code fails too.
    let index = index();
    let table: std::collections::BTreeSet<String> = Code::all().iter().map(|c| c.code()).collect();
    for code in &table {
        assert!(
            index.contains_key(code.as_str()),
            "no INDEX row for {code} — classify it (fixture / external / unemittable)"
        );
    }
    for (k, row) in &index {
        assert!(
            table.contains(*k),
            "INDEX row {k} names no registered code — stale coverage"
        );
        match row.cov {
            Cov::X(owner) | Cov::U(owner) | Cov::N(owner) => assert!(
                !owner.is_empty(),
                "{k}: an external/unemittable row must name its honest owner"
            ),
            Cov::F => {
                assert!(row.build.is_some(), "{k}: a fixture row needs a builder");
                assert!(row.stage.is_some(), "{k}: a fixture row declares its stage");
            }
        }
    }
}

#[test]
fn each_fixture_rejects_with_exactly_its_code() {
    let index = index();
    let mut fixtures = 0;
    for row in index.values() {
        if !matches!(row.cov, Cov::F) {
            continue;
        }
        fixtures += 1;
        let build = row.build.expect("fixture rows carry a builder");
        let diags = build();
        let hits: Vec<&AssemblyDiagnostic> =
            diags.iter().filter(|d| d.code.code() == row.code).collect();
        assert_eq!(
            hits.len(),
            1,
            "{}: expected exactly one diagnostic, got {:?}",
            row.code,
            diags
                .iter()
                .map(|d| (d.code.code(), d.path.clone()))
                .collect::<Vec<_>>()
        );
        let d = hits[0];
        assert_eq!(
            d.stage,
            row.stage.expect("fixture rows declare a stage"),
            "{}: stage",
            row.code
        );
        assert_eq!(d.severity, row.severity, "{}: severity", row.code);
        // "a resolving path" — the pointer exists and lands where declared.
        assert!(
            d.path.starts_with(row.path_prefix),
            "{}: path `{}` does not start with `{}`",
            row.code,
            d.path,
            row.path_prefix
        );
        assert!(!d.path.is_empty(), "{}: empty path", row.code);
        if let Some(expected) = row.source_layer {
            assert_eq!(
                d.source_layer.as_deref(),
                expected,
                "{}: source_layer",
                row.code
            );
        }
        if row.exact {
            let others: Vec<String> = diags
                .iter()
                .filter(|d| {
                    d.severity == Severity::Error
                        && d.code.code() != row.code
                        && !row.also.contains(&d.code.code().as_str())
                })
                .map(|d| d.code.code())
                .collect();
            assert!(
                others.is_empty(),
                "{}: undeclared companion error codes leaked into the fixture: {others:?}",
                row.code
            );
        }
    }
    assert!(
        fixtures >= 30,
        "the corpus battery must be substantial: {fixtures}"
    );
}

#[test]
fn planted_multi_error_returns_every_diagnostic() {
    // The AC's second leg — never fail-fast: a document carrying three
    // independent faults reports all three (unknown class + undeclared value
    // + benchmark token).
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
    a.ext.insert("suite_id".into(), Json::str("bench"));
    let diags = validate_authored(&doc_with(&a), &Stage1Catalog::stage1());
    for code in ["C-CLASS-1", "C-PARAM-1", "C-LCD-4"] {
        assert!(
            diags.iter().any(|d| d.code.code() == code),
            "{code} missing from the multi-error report: {:?}",
            diags.iter().map(|d| d.code.code()).collect::<Vec<_>>()
        );
    }
}

/// A deterministic xorshift64 PRNG — the fuzz corpus is reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn fuzzed_corpus_yields_no_c_int_1() {
    // The AC's third leg: mutated inputs may refuse, but every refusal lands
    // on a *named* code — `C-INT-1` anywhere is a defect.
    let base = hh_assembly::schema::encode(&stage1_assembly());
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for i in 0..600usize {
        let mut bytes = base.clone();
        match rng.next() % 4 {
            0 => {
                // Truncate.
                let n = (rng.next() as usize) % bytes.len().max(1);
                bytes.truncate(n);
            }
            1 => {
                // Flip a byte to a grammar-significant char.
                if !bytes.is_empty() {
                    let at = (rng.next() as usize) % bytes.len();
                    let pool = [b'"', b'{', b'}', b',', b':', b'[', b']', b'\\', b'x'];
                    bytes[at] = pool[(rng.next() as usize) % pool.len()];
                }
            }
            2 => {
                // Duplicate a slice in place.
                if bytes.len() > 8 {
                    let at = (rng.next() as usize) % (bytes.len() - 4);
                    let take = 4 + (rng.next() as usize) % (bytes.len() - at - 4).min(64);
                    let slice: Vec<u8> = bytes[at..at + take].to_vec();
                    bytes.splice(at..at, slice);
                }
            }
            _ => {
                // Splice in a raw token.
                let at = (rng.next() as usize) % bytes.len().max(1);
                let tok = [b"true".as_slice(), b"null".as_slice(), b"\"\"".as_slice()]
                    [(rng.next() as usize) % 3];
                bytes.splice(at..at, tok.iter().copied());
            }
        }
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut produced: Vec<String> = Vec::new();
            match hh_assembly::schema::load(&bytes, None) {
                Ok(a) => {
                    // A decodable-but-mutated assembly still passes through
                    // validate — the whole pipeline must stay typed.
                    let doc = doc_with(&a);
                    let r = hh_assembly::validate_assembly(
                        hh_assembly::Subject::Authored(&doc),
                        &Stage1Catalog::stage1(),
                        None,
                        &kernel(),
                    );
                    produced.extend(r.diagnostics.iter().map(|d| d.code.code()));
                }
                Err(diags) => produced.extend(diags.iter().map(|d| d.code.code())),
            }
            // The document half too — canonical bytes mutated the same way.
            let mut doc = doc_with(&stage1_assembly());
            doc.assembly = hh_wire::canonical::parse_canonical(&bytes).ok();
            let r = hh_assembly::validate_assembly(
                hh_assembly::Subject::Authored(&doc),
                &Stage1Catalog::stage1(),
                None,
                &kernel(),
            );
            produced.extend(r.diagnostics.iter().map(|d| d.code.code()));
            produced
        }));
        match attempt {
            Ok(codes) => assert!(
                !codes.iter().any(|c| c == "C-INT-1"),
                "mutation {i} yielded C-INT-1 — an uncoded rejection escaped the taxonomy"
            ),
            Err(_) => panic!("mutation {i} panicked the decode/validate path"),
        }
    }
}
