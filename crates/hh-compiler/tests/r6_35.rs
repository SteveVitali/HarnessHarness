//! R6.35 acceptance battery, Stage 1 (manifest 293; REQ-XE-112/113 +
//! REQ-XE-104's compile half) — `hh-tmpl/1`: unit goldens for
//! substitution/conditionals/whitespace control, refusal tests for the
//! out-of-subset constructs, the subset-parse of the pinned `mini.yaml`
//! templates (sha256 `b539a896…`, E1 A.3 — extracted post-YAML so the
//! block scalars' trailing newline is preserved), the compile purity leg,
//! and `prompt_layout.section_templates`/`roles` in `lower_layout`
//! (definition-authority leaves into `system`/`user` sections).

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::*;

use hharness_compiler::compile;
use hharness_compiler::tmpl::{compile_template, TmplError};
use hharness_hir::records::KindRecord;
use hharness_provenance::AuthorityClass;
use hharness_wire::json::Json;

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ── AC-1 (REQ-XE-112; AC-7.3-10) — unit goldens ──────────────────────────────

#[test]
fn ac1_substitution_golden() {
    let t = compile_template("A {{task}} B {{uname.system}}.").unwrap();
    assert_eq!(
        t.variables().iter().cloned().collect::<Vec<_>>(),
        vec!["task".to_string(), "uname.system".to_string()]
    );
    assert_eq!(
        t.render(&vars(&[("task", "fix it"), ("uname.system", "Linux")]))
            .unwrap(),
        "A fix it B Linux."
    );
}

#[test]
fn ac1_conditionals_golden() {
    let src =
        "{% if system == \"Darwin\" %}MAC{% elif system == \"Linux\" %}LIN{% else %}OTH{% endif %}";
    let t = compile_template(src).unwrap();
    assert_eq!(t.render(&vars(&[("system", "Darwin")])).unwrap(), "MAC");
    assert_eq!(t.render(&vars(&[("system", "Linux")])).unwrap(), "LIN");
    assert_eq!(t.render(&vars(&[("system", "BSD")])).unwrap(), "OTH");
    // No `else` — a non-matching render is empty, never an error.
    let t = compile_template("{% if a == \"x\" %}1{% endif %}").unwrap();
    assert_eq!(t.render(&vars(&[("a", "y")])).unwrap(), "");
}

#[test]
fn ac1_whitespace_control_golden() {
    // `-` on either delimiter strips the adjacent whitespace incl.
    // newlines (the mini `Darwin` block's shape).
    let t = compile_template("a\n  {%- if s == \"x\" -%}\n  b\n  {%- endif -%}\n  c").unwrap();
    assert_eq!(t.render(&vars(&[("s", "x")])).unwrap(), "abc");
    let t = compile_template("a {{- v -}} c").unwrap();
    assert_eq!(t.render(&vars(&[("v", "B")])).unwrap(), "aBc");
    // `keep_trailing_newline = false` — one trailing newline is stripped
    // at compile; interior newlines stay.
    assert_eq!(
        compile_template("x\n").unwrap().render(&vars(&[])).unwrap(),
        "x"
    );
    assert_eq!(
        compile_template("x\n\n")
            .unwrap()
            .render(&vars(&[]))
            .unwrap(),
        "x\n"
    );
}

// ── AC-1 — refusal tests (each yields the unexpressible/undefined leg) ───────

fn unexpressible(src: &str) -> String {
    match compile_template(src) {
        Err(TmplError::Unexpressible { detail, .. }) => detail,
        other => panic!("expected Unexpressible for {src:?}, got {other:?}"),
    }
}

#[test]
fn ac1_out_of_subset_refuses() {
    assert!(unexpressible("{{x | tojson}}").contains("ident"));
    assert!(unexpressible("{% for x in y %}{% endfor %}").contains("for"));
    assert!(unexpressible("{% if a != \"b\" %}{% endif %}").contains("=="));
    assert!(unexpressible("{% if a %}{% endif %}").contains("=="));
    assert!(unexpressible("{% if a is defined %}{% endif %}").contains("=="));
    assert!(unexpressible("{# a comment #}").contains("comments"));
    assert!(unexpressible("{% set a = 1 %}").contains("set"));
    assert!(unexpressible("{% if a == \"x\" %}1").contains("endif"));
    // `+` markers and non-literal comparisons refuse typed too.
    assert!(unexpressible("{%+ if a == \"x\" %}{% endif %}").contains("+"));
    assert!(unexpressible("{% if a == b %}{% endif %}").contains("literal"));
}

#[test]
fn ac1_undefined_render_refuses() {
    let t = compile_template("{{task}}").unwrap();
    match t.render(&vars(&[])) {
        Err(TmplError::Undefined { name }) => assert_eq!(name, "task"),
        other => panic!("expected Undefined, got {other:?}"),
    }
}

// ── AC-1 — the pinned `mini.yaml` templates parse ─────────────────────────────

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_35")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"))
}

#[test]
fn ac1_pinned_templates_parse_and_list_variables() {
    // `system_template` (mini.yaml:2-3, post-YAML block scalar) — no
    // variables, renders to itself minus the trailing newline.
    let sys = fixture("system_template.tmpl");
    let t = compile_template(&sys).unwrap();
    assert!(t.variables().is_empty());
    assert_eq!(
        t.render(&vars(&[])).unwrap(),
        "You are a helpful assistant that can interact with a computer."
    );
    // `instance_template` (mini.yaml:4-100, 97 lines) — the variable set
    // is exactly mini's uname+task family.
    let inst = fixture("instance_template.tmpl");
    let t = compile_template(&inst).unwrap();
    assert_eq!(
        t.variables().iter().cloned().collect::<Vec<_>>(),
        vec![
            "machine".to_string(),
            "release".to_string(),
            "system".to_string(),
            "task".to_string(),
            "version".to_string(),
        ]
    );
    // The Darwin conditional renders its branch only on MacOS.
    let rendered = t
        .render(&vars(&[
            ("task", "T"),
            ("system", "Darwin"),
            ("release", "R"),
            ("version", "V"),
            ("machine", "M"),
        ]))
        .unwrap();
    assert!(rendered.contains("sed -i ''"), "{rendered}");
    assert!(rendered.contains("Darwin R V M"), "{rendered}");
    let rendered = t
        .render(&vars(&[
            ("task", "T"),
            ("system", "Linux"),
            ("release", "R"),
            ("version", "V"),
            ("machine", "M"),
        ]))
        .unwrap();
    assert!(!rendered.contains("sed -i ''"), "{rendered}");
    assert!(rendered.contains("Linux R V M"), "{rendered}");
}

// ── AC-2 (REQ-XE-113; AC-7.3-11) — the purity leg ─────────────────────────────

#[test]
fn ac2_compile_is_pure() {
    // `compile_template` reads no clock/env/host — two invocations are
    // identical and the variable set is data, never probed values.
    let inst = fixture("instance_template.tmpl");
    assert_eq!(
        compile_template(&inst).unwrap(),
        compile_template(&inst).unwrap()
    );
}

// ── AC-4 compile half (REQ-XE-104) — `section_templates`/`roles` lowering ─────

/// A `ToolCapability` node whose `purpose` leaf is a `hh-tmpl/1`
/// `definition`-authority template (the AC-4 placement shape).
fn tmpl_node(id: &str, content: &str, seq: u64) -> hharness_hir::document::Node {
    let mut n = tool_node(id, seq);
    if let KindRecord::ToolCapability(t) = &mut n.semantic {
        t.purpose = text(content, seq);
        t.purpose.authority = AuthorityClass::Definition;
        t.purpose.language = Some("hh-tmpl/1".to_string());
    }
    n
}

fn mini_layout_params() -> Json {
    Json::obj([
        (
            "order",
            Json::Arr(vec![Json::str("system"), Json::str("task")]),
        ),
        (
            "roles",
            Json::obj([("system", Json::str("system")), ("task", Json::str("user"))]),
        ),
        (
            "section_templates",
            Json::obj([
                ("system", Json::str("test:sys_tmpl")),
                ("task", Json::str("test:inst_tmpl")),
            ]),
        ),
        (
            "slots",
            Json::obj([(
                "task",
                Json::Arr(vec![
                    Json::str("task"),
                    Json::str("system"),
                    Json::str("release"),
                    Json::str("version"),
                    Json::str("machine"),
                ]),
            )]),
        ),
    ])
}

fn mini_doc() -> hharness_hir::document::HirDocument {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(tmpl_node(
        "test:sys_tmpl",
        &fixture("system_template.tmpl"),
        7,
    ));
    doc.nodes.push(tmpl_node(
        "test:inst_tmpl",
        &fixture("instance_template.tmpl"),
        8,
    ));
    doc
}

fn layout_profile(params: Json) -> hharness_compiler::profile::ModelProfile {
    profile_with(
        "sha256:profile",
        "1.0",
        vec![rule(
            "r.layout",
            hharness_compiler::profile::ProfileRuleKind::PromptLayout,
            &["prompt_layout"],
            params,
            hharness_compiler::profile::DebtStatus::Active,
        )],
    )
}

fn compile_layout(
    tag: &str,
    doc: hharness_hir::document::HirDocument,
    params: Json,
) -> Result<hharness_compiler::seal::CompiledBundle, hharness_compiler::errors::CompileError> {
    let (store, sealed, _v) = sealed_doc_with(tag, doc, vec![], vec![]);
    let profile = layout_profile(params);
    let inputs = hharness_compiler::CompileInputs {
        sealed: sealed.clone(),
        profile_refs: vec![hharness_compiler::profile::profile_coordinate(&profile)],
        fallback_profile: None,
        targets: vec![],
        compile_for_expired: false,
        intent_ref: None,
    };
    let profiles = MapProfileView::of(vec![profile]);
    compile(
        &inputs,
        &profiles,
        &store,
        &hharness_assembly::Stage1Catalog::stage1(),
        &kernel(),
    )
}

#[test]
fn ac4_section_templates_place_leaves_with_roles() {
    let b = compile_layout("r635a", mini_doc(), mini_layout_params()).expect("compile");
    let hharness_compiler::seal::ModelSurfaceState::Lowered(s) = &b.model_surface else {
        panic!("lowered")
    };
    assert_eq!(s.layout.len(), 2);
    let sys = &s.layout[0];
    assert_eq!(sys.section_id, "system");
    assert_eq!(sys.role.as_deref(), Some("system"));
    assert_eq!(sys.grammar.as_deref(), Some("hh-tmpl/1"));
    assert_eq!(sys.source_node_ids, vec!["test:sys_tmpl".to_string()]);
    let task = &s.layout[1];
    assert_eq!(task.role.as_deref(), Some("user"));
    assert_eq!(
        task.slots.iter().cloned().collect::<Vec<_>>(),
        vec![
            "task".to_string(),
            "system".to_string(),
            "release".to_string(),
            "version".to_string(),
            "machine".to_string(),
        ]
    );
    // The section's text is the template *source* — no uname value was
    // ever referenced at compile (REQ-XE-113's never-at-compile leg).
    assert!(task.text.as_deref().unwrap_or("").contains("{{task}}"));
    assert!(!task.text.as_deref().unwrap_or("").contains("Darwin R V M"));
}

#[test]
fn ac4_unexpressible_placements_refuse() {
    // A variable outside the section's declared slots → UnexpressibleSurface.
    let mut params = mini_layout_params();
    if let Json::Obj(m) = &mut params {
        m.insert(
            "slots".to_string(),
            Json::obj([("task", Json::Arr(vec![Json::str("task")]))]),
        );
    }
    match compile_layout("r635b", mini_doc(), params) {
        Err(hharness_compiler::errors::CompileError::UnexpressibleSurface { reason, .. }) => {
            assert!(reason.contains("not declared slots"), "{reason}")
        }
        other => panic!("expected UnexpressibleSurface, got {other:?}"),
    }
    // An undeclared section / role / node each refuse typed.
    for (member, value) in [
        ("roles", Json::obj([("ghost", Json::str("system"))])),
        ("roles", Json::obj([("system", Json::str("overlord"))])),
        (
            "section_templates",
            Json::obj([("system", Json::str("test:missing"))]),
        ),
    ] {
        let mut params = mini_layout_params();
        if let Json::Obj(m) = &mut params {
            m.insert(member.to_string(), value.clone());
        }
        match compile_layout("r635b", mini_doc(), params) {
            Err(hharness_compiler::errors::CompileError::UnexpressibleSurface { .. }) => {}
            other => panic!("{member}: expected UnexpressibleSurface, got {other:?}"),
        }
    }
}
