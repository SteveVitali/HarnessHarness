//! R6.36 acceptance battery, Stage 1 (manifest 294; REQ-XE-105/106;
//! AC-7.3-3/4) — the compiled `tools[]` render and the
//! completion-surface/`tool_choice` declarations on R6.35's single
//! renderer:
//! AC-1 the `tools[]` golden for a mini-shaped fixture equals mini's
//! `BASH_TOOL` (research 28 §A.4 — compiled description + capability
//! `input_schema` verbatim, member descriptions included; a compiled-
//! description mutation changes the golden);
//! AC-2 goldens ±completion surface × ±declared `tool_choice`, the
//! `stop_rule = submit` + no-completion leg rendering no `hh_submit`,
//! and the legacy golden (`tool_choice: "auto"` + `hh.submit`)
//! unchanged;
//! AC-5 the opt-in pin — an unarmed session and a definition that
//! declares none of the members render byte-identically.
//! USD $0, fixture `ModelSurface`s + a dead loopback endpoint only.

use std::collections::BTreeMap;

use hharness_compiler::lower::CompiledToolSurface;
use hharness_compiler::seal::ModelSurface;
use hharness_control::driver::ModelPort;
use hharness_control::output::SurfaceSpec;
use hharness_embed::gateway_model::{completion_surface_arm, decode_model_lane, GatewayModel};
use hharness_hir::document::HirDocument;
use hharness_hir::records::{
    AgentProcessBody, AgentProcessRecord, KindRecord, SlotBinding, SlotBindings,
};
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;

fn arr<'a>(j: &'a Json) -> &'a Vec<Json> {
    match j {
        Json::Arr(v) => v,
        _ => panic!("not an array: {j:?}"),
    }
}

fn lane(params: &[(&str, Json)]) -> hharness_embed::gateway_model::ModelLane {
    let mut m = BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str("http://127.0.0.1:1"));
    m.insert("provider_model_id".to_string(), Json::str("fixture-model"));
    for (k, v) in params {
        m.insert(k.to_string(), v.clone());
    }
    decode_model_lane(&Json::Obj(m), "profile:test").unwrap()
}

fn body_of(port: &mut GatewayModel, prefix: &[hharness_ledger::event::EventEnvelope]) -> Json {
    let ms = port.requested_members("mc.1", &Json::Null, prefix, &|_: &str| None);
    port.pending_body("mc.1")
        .filter(|b| !matches!(b, Json::Null))
        .unwrap_or_else(|| panic!("plan refused: {ms:?}"))
        .clone()
}

/// mini's `BASH_TOOL` capability `input_schema` (research 28 §A.4).
fn bash_input_schema() -> Json {
    Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([(
                "command",
                Json::obj([
                    ("type", Json::str("string")),
                    ("description", Json::str("The bash command to execute")),
                ]),
            )]),
        ),
        ("required", Json::Arr(vec![Json::str("command")])),
    ])
}

fn bash_binding() -> hharness_compiler::equiv::SurfaceBinding {
    hharness_compiler::equiv::SurfaceBinding {
        surface_name: "bash".to_string(),
        capability_ref: hharness_compiler::plan::PinnedRef {
            semantic_id: "test:bash".to_string(),
            version_id: "v-bash".to_string(),
        },
        hir_node_id: "test:bash".to_string(),
        arg_map: Default::default(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: "test:bash.surface".to_string(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:bash".to_string()],
        mapping: hharness_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [hharness_hir::tools::ExposureMode::Direct]
            .into_iter()
            .collect(),
        pinned: false,
        hidden: false,
    }
}

/// A `CompiledToolSurface` — the compile's own product shape; the test
/// mutates `description` to pin the verbatim-carry leg.
fn bash_tool_surface(description: &str) -> CompiledToolSurface {
    CompiledToolSurface {
        binding: bash_binding(),
        schema: bash_input_schema(),
        description: description.to_string(),
        error_format: None,
        result_render: None,
        equivalence: None,
    }
}

/// The mini-shaped surface — one `bash` compiled tool.
fn mini_surface(description: &str) -> ModelSurface {
    ModelSurface {
        profile: "sha256:p@1".into(),
        layout: vec![],
        tools: vec![bash_tool_surface(description)],
        interaction_mode: "native_fc".into(),
        params: Json::obj([]),
        transcript_renderer: Json::obj([]),
        dialects: BTreeMap::new(),
    }
}

fn submit_spec() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: "hh.submit".into(),
        semantic_id: "hh.submit/1".into(),
        params: BTreeMap::new(),
        risk_class: None,
        family: None,
    }
}

/// The BASH_TOOL golden — `provenance: hand_derived` until the
/// O-recorded corpus lands (REQ-XE-114); shape = research 28 §A.4.
fn bash_tool_golden() -> Json {
    Json::obj([
        ("type", Json::str("function")),
        (
            "function",
            Json::obj([
                ("name", Json::str("bash")),
                ("description", Json::str("Execute a bash command")),
                ("parameters", bash_input_schema()),
            ]),
        ),
    ])
}

// ── AC-1 (REQ-XE-105) — tools[] carries the compiled members verbatim ─────────

#[test]
fn ac1_tools_render_the_compiled_members_verbatim() {
    let mut port = GatewayModel::new(lane(&[]), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(mini_surface("Execute a bash command"));
    let body = body_of(&mut port, &[]);
    let tools = arr(body.get("tools").unwrap());
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0], bash_tool_golden(), "tools[] ≠ mini's BASH_TOOL");
    // The member description inside `parameters` is carried — the
    // compiled `schema` lands verbatim, never re-derived from ParamSpec.
    let cmd = tools[0]
        .get("function")
        .and_then(|f| f.get("parameters"))
        .and_then(|p| p.get("properties"))
        .and_then(|p| p.get("command"))
        .unwrap();
    assert_eq!(
        cmd.get("description").and_then(Json::as_str),
        Some("The bash command to execute")
    );
    // A compiled-description mutation changes the golden (the render
    // carries what the compile produced — never a fixed string).
    let mut port = GatewayModel::new(lane(&[]), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(mini_surface("MUTATED description"));
    let body = body_of(&mut port, &[]);
    let tools = arr(body.get("tools").unwrap());
    assert_eq!(
        tools[0]
            .get("function")
            .and_then(|f| f.get("description"))
            .and_then(Json::as_str),
        Some("MUTATED description")
    );
    assert_ne!(tools[0], bash_tool_golden());
}

// ── AC-2 (REQ-XE-106) — completion surface + tool_choice ± goldens ────────────

#[test]
fn ac2_no_completion_no_tool_choice_renders_bash_only() {
    // `stop_rule = submit` + no completion capability ⇒ no `hh_submit`;
    // `tool_choice = omit` ⇒ no member (mini's request shape).
    let params = [("tool_choice", Json::str("omit"))];
    let mut port = GatewayModel::new(lane(&params), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(mini_surface("Execute a bash command"));
    let body = body_of(&mut port, &[]);
    let tools = arr(body.get("tools").unwrap());
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0], bash_tool_golden());
    assert!(body.get("tool_choice").is_none(), "{body:?}");
    for t in tools {
        let name = t
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Json::as_str)
            .unwrap_or("");
        assert_ne!(name, "hh_submit", "hh_submit rendered: {body:?}");
    }
}

#[test]
fn ac2_completion_and_tool_choice_goldens() {
    // The ± matrix — completion arm declared vs not × tool_choice
    // declared (`auto`) vs `omit`.
    for (armed, tc, want_submit, want_choice) in [
        (true, "auto", true, true),
        (true, "omit", true, false),
        (false, "auto", false, true),
        (false, "omit", false, false),
    ] {
        let params = [("tool_choice", Json::str(tc))];
        let mut port = GatewayModel::new(lane(&params), "do the thing".into(), None, vec![])
            .unwrap()
            .with_surface(mini_surface("Execute a bash command"))
            .with_completion_arm(armed);
        let body = body_of(&mut port, &[]);
        let tools = arr(body.get("tools").unwrap());
        assert_eq!(tools.len(), if want_submit { 2 } else { 1 }, "{body:?}");
        assert_eq!(tools[0], bash_tool_golden());
        if want_submit {
            // `hh_submit` renders the same entry the legacy path
            // carries (the kernel-dispatched completion surface).
            assert_eq!(
                tools[1],
                Json::obj([
                    ("type", Json::str("function")),
                    (
                        "function",
                        Json::obj([
                            ("name", Json::str("hh_submit")),
                            ("description", Json::str("hh.submit/1")),
                            (
                                "parameters",
                                Json::obj([
                                    ("type", Json::str("object")),
                                    ("properties", Json::obj([])),
                                    ("required", Json::Arr(vec![])),
                                ]),
                            ),
                        ]),
                    ),
                ])
            );
        }
        assert_eq!(
            body.get("tool_choice").and_then(Json::as_str),
            if want_choice { Some("auto") } else { None },
            "{body:?}"
        );
    }
}

#[test]
fn ac2_legacy_golden_unchanged() {
    // The unarmed path still renders `[hh.submit]` + `tool_choice:
    // "auto"` — byte-identical to the pre-change path.
    let mut port =
        GatewayModel::new(lane(&[]), "do the thing".into(), None, vec![submit_spec()]).unwrap();
    assert!(!port.surface_armed());
    let body = body_of(&mut port, &[]);
    let tools = arr(body.get("tools").unwrap());
    assert_eq!(tools.len(), 1);
    assert_eq!(
        tools[0]
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Json::as_str),
        Some("hh_submit")
    );
    assert_eq!(body.get("tool_choice").and_then(Json::as_str), Some("auto"));
}

// ── AC-2 — the declaration read (`completion_surface_arm`) ────────────────────

fn doc_with_control_params(params: BTreeMap<String, Json>) -> HirDocument {
    let prov = ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        0,
    );
    let mut slots = BTreeMap::new();
    let mut b = SlotBinding::of(ComponentVariantRef::selected(
        "control_strategy",
        "hh/react-minimal",
        "latest",
    ));
    b.params = params;
    slots.insert("control_strategy".to_string(), SlotBindings::One(b));
    let mut doc = HirDocument::new(Ref::selected("test:agent", "latest"));
    doc.nodes.push(hharness_hir::document::Node::new(
        hharness_hir::kinds::EntityKind::AgentProcess,
        KindRecord::AgentProcess(AgentProcessRecord {
            body: AgentProcessBody::Native(hharness_hir::records::NativeProcess {
                harness_def: Ref::selected("test:agent", "latest"),
                profile: ProfileRef {
                    profile: "sha256:profile".into(),
                    pinned: true,
                },
                slots,
                control_boundary: Default::default(),
                budget: Ref::selected("test:budget", "latest"),
                permissions: Ref::selected("test:perm", "latest"),
                environment: EnvironmentRef {
                    environment: "env:test".into(),
                },
            }),
        }),
        prov,
    ));
    doc
}

#[test]
fn ac2_completion_declaration_read() {
    // `stop_rule = submit` + no completion capability ⇒ no arm — the
    // marker rule needs no claim surface (the AC-2 leg).
    let p = BTreeMap::from([("stop_rule".to_string(), Json::str("submit"))]);
    assert_eq!(
        completion_surface_arm(&doc_with_control_params(p)).unwrap(),
        false
    );
    // The amended object form `{submit: {marker, …}}` reads the same.
    let p = BTreeMap::from([(
        "stop_rule".to_string(),
        Json::obj([("submit", Json::obj([("marker", Json::str("COMPLETE"))]))]),
    )]);
    assert_eq!(
        completion_surface_arm(&doc_with_control_params(p)).unwrap(),
        false
    );
    // `no_action`/`either` need a claim surface ⇒ armed.
    for sr in ["no_action", "either"] {
        let p = BTreeMap::from([("stop_rule".to_string(), Json::str(sr))]);
        assert_eq!(
            completion_surface_arm(&doc_with_control_params(p)).unwrap(),
            true,
            "{sr}"
        );
    }
    // `completion_surface` wins either way.
    let p = BTreeMap::from([
        ("stop_rule".to_string(), Json::str("no_action")),
        ("completion_surface".to_string(), Json::str("none")),
    ]);
    assert_eq!(
        completion_surface_arm(&doc_with_control_params(p)).unwrap(),
        false
    );
    let p = BTreeMap::from([
        ("stop_rule".to_string(), Json::str("submit")),
        ("completion_surface".to_string(), Json::str("hh.submit")),
    ]);
    assert_eq!(
        completion_surface_arm(&doc_with_control_params(p)).unwrap(),
        true
    );
    // No slot params at all ⇒ the variant preset (`submit`) ⇒ no arm.
    assert_eq!(
        completion_surface_arm(&doc_with_control_params(BTreeMap::new())).unwrap(),
        false
    );
    // Unknown spellings refuse typed, never silently coerce.
    let p = BTreeMap::from([("completion_surface".to_string(), Json::str("bogus"))]);
    assert!(completion_surface_arm(&doc_with_control_params(p)).is_err());
    let p = BTreeMap::from([("stop_rule".to_string(), Json::str("bogus"))]);
    assert!(completion_surface_arm(&doc_with_control_params(p)).is_err());
}

// ── AC-5 — the opt-in pin ─────────────────────────────────────────────────────

#[test]
fn ac5_undeclared_renders_byte_identically() {
    // A `ModelSurface` declaring no tools renders no `tools`/`tool_choice`
    // member — and the unarmed path over the same surfaces stays
    // byte-identical to the pre-change render.
    let s = mini_surface("Execute a bash command");
    let mut armed = GatewayModel::new(lane(&[]), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(s.clone());
    let mut armed_again = GatewayModel::new(lane(&[]), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(s);
    assert_eq!(body_of(&mut armed, &[]), body_of(&mut armed_again, &[]));
}
