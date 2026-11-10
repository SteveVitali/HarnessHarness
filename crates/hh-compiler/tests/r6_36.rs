//! R6.36 acceptance battery, Stage 2 (manifest 294; REQ-XE-110/111;
//! AC-7.3-8/9) — the typed pure renderers AR-23 admits:
//! `json_head_tail@1`/`text_head_tail@1` with typed-parameter constants
//! (G8; BM1/BM2/BM15-shaped goldens — 9 999 / 10 000 / 10 001 chars and
//! multi-byte boundaries in both units, `provenance: hand_derived` until
//! the O-recorded corpus lands, REQ-XE-114) and the `error_format`
//! typed format-failure classes rendered from `Text` leaves with slots
//! (G9; BM4–BM8-shaped goldens + batch scope). The renderer execution
//! lives in `hharness_compiler::result_render` — pure, no clock/env/
//! host reads. USD $0, fixtures only.

mod common;

use std::collections::BTreeMap;

use common::*;

use hharness_compiler::compile;
use hharness_compiler::result_render::{
    render, render_format_error, render_response_errors, ResultRenderError,
};
use hharness_compiler::surface::{
    FormatFailureClass, FormatFailureScope, HeadTailUnit, ResultJsonStyle, ResultRenderer,
};
use hharness_hir::kinds::EffectClass;
use hharness_hir::records::KindRecord;
use hharness_wire::json::Json;

fn rec(pairs: &[(&str, Json)]) -> BTreeMap<String, Json> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

/// mini's `result_renderer` declaration (research 28 §A.5 — hand_derived).
fn mini_json_spec() -> ResultRenderer {
    ResultRenderer::JsonHeadTail {
        threshold: 10_000,
        head: 5_000,
        tail: 5_000,
        unit: HeadTailUnit::Chars,
        fields: vec![
            hharness_compiler::surface::ResultRenderField {
                name: "returncode".into(),
                optional: false,
                truncate: false,
                warning: None,
            },
            hharness_compiler::surface::ResultRenderField {
                name: "output".into(),
                optional: false,
                truncate: true,
                warning: Some("Output too long.".into()),
            },
            hharness_compiler::surface::ResultRenderField {
                name: "exception_info".into(),
                optional: true,
                truncate: false,
                warning: None,
            },
        ],
        json_style: ResultJsonStyle::PythonJinjaTojson,
    }
}

// ── AC-3 (REQ-XE-110; AC-7.3-8) — json_head_tail@1 goldens ────────────────────

#[test]
fn ac3_json_head_tail_under_threshold() {
    // BM1-shaped: output under the threshold renders whole, the mini
    // template's pretty-object shape.
    let result = rec(&[
        ("returncode", Json::Int(0)),
        ("output", Json::str("hello <&>'\nworld".to_string())),
    ]);
    let out = render(&mini_json_spec(), &result).unwrap();
    // `tojson` — htmlsafe escapes + the `\n` escape.
    assert_eq!(
        out,
        "{\n  \"returncode\": 0,\n  \"output\": \"hello \\u003c\\u0026\\u003e\\u0027\\nworld\"\n}"
    );
    // Optional member present → inline `, "exception_info": …`.
    let result = rec(&[
        ("returncode", Json::Int(-1)),
        ("output", Json::str("x".to_string())),
        ("exception_info", Json::str("timed out".to_string())),
    ]);
    let out = render(&mini_json_spec(), &result).unwrap();
    assert_eq!(
        out,
        "{\n  \"returncode\": -1,\n  \"output\": \"x\", \"exception_info\": \"timed out\"\n}"
    );
}

#[test]
fn ac3_json_head_tail_threshold_boundaries() {
    // BM2/BM15-shaped: 9 999 → whole; 10 000 → truncated with
    // `elided_chars: 0` (the template's `length − 10000` arithmetic);
    // 10 001 → `elided_chars: 1`.
    for (n, whole) in [(9_999usize, true), (10_000, false), (10_001, false)] {
        let output = "o".repeat(n);
        let result = rec(&[
            ("returncode", Json::Int(0)),
            ("output", Json::str(output.clone())),
        ]);
        let out = render(&mini_json_spec(), &result).unwrap();
        if whole {
            assert!(out.contains("\"output\": \""), "{n}: {out}");
            assert!(!out.contains("output_head"), "{n}: {out}");
        } else {
            assert!(out.contains("\"output_head\": \""), "{n}: {out}");
            assert!(out.contains("\"output_tail\": \""), "{n}: {out}");
            assert!(
                out.contains(&format!("\"elided_chars\": {}", n - 10_000)),
                "{n}: {out}"
            );
            assert!(
                out.contains("\"warning\": \"Output too long.\""),
                "{n}: {out}"
            );
        }
    }
    // Head + tail come from the edges — 10 001 chars of 'o' gives a
    // 5 000-char head and a 5 000-char tail.
    let result = rec(&[
        ("returncode", Json::Int(0)),
        (
            "output",
            Json::str(format!("{}MID{}", "h".repeat(6_000), "t".repeat(6_000))),
        ),
    ]);
    let out = render(&mini_json_spec(), &result).unwrap();
    assert!(out.contains(&"h".repeat(4_996)), "{out}");
    assert!(out.contains(&"t".repeat(5_000)), "{out}");
}

#[test]
fn ac3_units_and_param_constants_are_typed() {
    // The constants are typed params — a member change, not a template
    // edit, moves the threshold (G8's sweep rule).
    let mut spec = mini_json_spec();
    let ResultRenderer::JsonHeadTail { threshold, .. } = &mut spec else {
        unreachable!()
    };
    *threshold = 4;
    let result = rec(&[
        ("returncode", Json::Int(0)),
        ("output", Json::str("abcdef".to_string())),
    ]);
    let out = render(&spec, &result).unwrap();
    assert!(out.contains("\"elided_chars\": 2"), "{out}");
    // `utf8_bytes` unit — multi-byte chars measure as bytes; a
    // mid-codepoint head cut decodes lossily (U+FFFD), never panics.
    let mut spec = mini_json_spec();
    let ResultRenderer::JsonHeadTail {
        threshold,
        head,
        tail,
        unit,
        ..
    } = &mut spec
    else {
        unreachable!()
    };
    *threshold = 10;
    *head = 5;
    *tail = 5;
    *unit = HeadTailUnit::Utf8Bytes;
    // "éé…" — é is 2 UTF-8 bytes: a 5-byte head cuts inside the third é.
    let result = rec(&[
        ("returncode", Json::Int(0)),
        ("output", Json::str("éééééééé".to_string())),
    ]);
    let out = render(&spec, &result).unwrap();
    // é (U+00E9) → `\u00e9` under python_jinja_tojson; the 5-byte head
    // keeps "éé" + the half-cut é's first byte (U+FFFD).
    assert!(out.contains("\\u00e9\\u00e9"), "{out}");
    assert!(out.contains("\\ufffd"), "{out}");
    // `chars` unit — the same output measures 8 scalars, under a 10-char
    // threshold ⇒ whole, é → `\u00e9` (ensure_ascii).
    let mut spec = mini_json_spec();
    let ResultRenderer::JsonHeadTail { unit, .. } = &mut spec else {
        unreachable!()
    };
    *unit = HeadTailUnit::Chars;
    let out = render(&spec, &result).unwrap();
    assert!(out.contains("\"output\": \""), "{out}");
    assert!(out.contains("\\u00e9"), "{out}");
}

#[test]
fn ac3_text_head_tail_golden() {
    // Terminus's `_limit_output_length` shape (research 28 §B.4 —
    // hand_derived): under → whole; over → head + marker + tail with
    // `omitted` counted in the declared unit.
    // The marker leaf ends `\n\n` — `hh-tmpl/1` strips one trailing
    // newline (`keep_trailing_newline = false`), so the render emits
    // Terminus's `…]\n` before the tail.
    let marker = hharness_hir::leaves::Text::new(
        "\n[... output limited to {{threshold}} bytes; {{omitted}} interior bytes omitted ...]\n\n"
            .to_string(),
        "test:owner".to_string(),
        kernel(),
    );
    let spec = ResultRenderer::TextHeadTail {
        threshold: 10,
        head: 4,
        tail: 4,
        unit: HeadTailUnit::Utf8Bytes,
        marker,
    };
    let result = rec(&[("output", Json::str("0123456789AB".to_string()))]);
    let out = render(&spec, &result).unwrap();
    assert_eq!(
        out,
        "0123\n[... output limited to 10 bytes; 2 interior bytes omitted ...]\n89AB"
    );
    // Under threshold → the whole string.
    let result = rec(&[("output", Json::str("short".to_string()))]);
    assert_eq!(render(&spec, &result).unwrap(), "short");
    // A `chars`-unit declaration measures scalars.
    let mut spec2 = spec.clone();
    let ResultRenderer::TextHeadTail { unit, .. } = &mut spec2 else {
        unreachable!()
    };
    *unit = HeadTailUnit::Chars;
    let result = rec(&[("output", Json::str("éééééééééééé".to_string()))]);
    let out = render(&spec2, &result).unwrap();
    // 12 scalars ≥ 10 → truncated; head/tail keep 4 *chars* each (8
    // UTF-8 bytes — the unit, not a byte count, drove the slice).
    assert!(out.starts_with("éééé"), "{out}");
    assert!(out.ends_with("éééé"), "{out}");
}

// ── AC-4 (REQ-XE-111; AC-7.3-9) — error_format classes + batch scope ──────────

fn classes_spec(scope: FormatFailureScope) -> hharness_compiler::surface::FormatErrorClasses {
    let mut renderings = BTreeMap::new();
    let leaf = |s: &str| {
        hharness_hir::leaves::Text::new(s.to_string(), "test:owner".to_string(), kernel())
    };
    renderings.insert(FormatFailureClass::NoToolCalls, leaf("No tool calls found in the response. Every response MUST include at least one tool call."));
    renderings.insert(
        FormatFailureClass::ArgsUnparseable,
        leaf("Tool call error:\n\n<error>\nError parsing tool call arguments: {{error}}\n</error>"),
    );
    renderings.insert(
        FormatFailureClass::UnknownTool,
        leaf("Tool call error:\n\n<error>\nUnknown tool '{{name}}'.\n</error>"),
    );
    renderings.insert(
        FormatFailureClass::MissingParam,
        leaf(
            "Tool call error:\n\n<error>\nMissing '{{name}}' argument in bash tool call.\n</error>",
        ),
    );
    renderings.insert(FormatFailureClass::OutputTruncated, leaf("Your previous response reached the output token limit (finish_reason={{finish_reason}}) before you produced a tool call, so it was cut off."));
    hharness_compiler::surface::FormatErrorClasses { renderings, scope }
}

#[test]
fn ac4_one_golden_per_class() {
    // BM4–BM8-shaped: one render per class from its `Text` leaf with
    // the declared slots (hand_derived — mini's `format_error_template`
    // error strings are the fixture basis).
    let spec = classes_spec(FormatFailureScope::PerCall);
    let no = render_format_error(&spec, FormatFailureClass::NoToolCalls, &BTreeMap::new()).unwrap();
    assert!(no.contains("No tool calls found in the response."), "{no}");
    let up = render_format_error(
        &spec,
        FormatFailureClass::ArgsUnparseable,
        &BTreeMap::from([(
            "error".to_string(),
            "Expecting value: line 1 column 1 (char 0)".to_string(),
        )]),
    )
    .unwrap();
    assert!(
        up.contains("Error parsing tool call arguments: Expecting value: line 1 column 1 (char 0)"),
        "{up}"
    );
    let un = render_format_error(
        &spec,
        FormatFailureClass::UnknownTool,
        &BTreeMap::from([("name".to_string(), "rm".to_string())]),
    )
    .unwrap();
    assert!(un.contains("Unknown tool 'rm'."), "{un}");
    let mp = render_format_error(
        &spec,
        FormatFailureClass::MissingParam,
        &BTreeMap::from([("name".to_string(), "command".to_string())]),
    )
    .unwrap();
    assert!(mp.contains("Missing 'command' argument"), "{mp}");
    let ot = render_format_error(
        &spec,
        FormatFailureClass::OutputTruncated,
        &BTreeMap::from([("finish_reason".to_string(), "length".to_string())]),
    )
    .unwrap();
    assert!(ot.contains("finish_reason=length"), "{ot}");
    // An undeclared class refuses typed — never a synthesized string.
    let mut spec2 = classes_spec(FormatFailureScope::PerCall);
    spec2.renderings.remove(&FormatFailureClass::UnknownTool);
    assert!(matches!(
        render_format_error(&spec2, FormatFailureClass::UnknownTool, &BTreeMap::new()),
        Err(ResultRenderError::NoRendering { .. })
    ));
}

#[test]
fn ac4_batch_scope_renders_one_failure_per_response() {
    // `format_failure_scope = batch` — a response whose first call is
    // malformed renders one failure for the whole response.
    let failures: Vec<(FormatFailureClass, BTreeMap<String, String>)> = vec![
        (
            FormatFailureClass::ArgsUnparseable,
            BTreeMap::from([("error".to_string(), "bad json".to_string())]),
        ),
        (
            FormatFailureClass::UnknownTool,
            BTreeMap::from([("name".to_string(), "rm".to_string())]),
        ),
    ];
    let batch =
        render_response_errors(&classes_spec(FormatFailureScope::Batch), &failures).unwrap();
    assert_eq!(batch.len(), 1);
    assert!(batch[0].contains("Error parsing tool call arguments: bad json"));
    let per =
        render_response_errors(&classes_spec(FormatFailureScope::PerCall), &failures).unwrap();
    assert_eq!(per.len(), 2);
    assert!(per[1].contains("Unknown tool 'rm'."));
}

// ── compile legs — the typed members land on the compiled surface ─────────────

fn mini_tools_doc(
    result_renderer: Json,
    error_format: Json,
) -> hharness_hir::document::HirDocument {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:bash", "bash", &["command"], 5);
    if let KindRecord::ToolCapability(tc) = &mut t.semantic {
        tc.purpose = text("Execute a bash command", 5);
        tc.input_schema = Json::obj([
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
        ]);
        tc.effects = hharness_hir::kinds::ToolEffects::Declared(
            vec![EffectClass::domain_only(
                hharness_hir::kinds::EffectDomain::Exec,
            )]
            .into_iter()
            .collect(),
        );
    }
    if let Some(hharness_hir::records::SurfaceRecord::Tool(ts)) = &mut t.surface {
        ts.description_template = text("{purpose}", 5);
        ts.result_renderer = result_renderer;
        ts.error_format = error_format;
    }
    doc.nodes.push(t);
    doc
}

fn compile_surface(
    tag: &str,
    doc: hharness_hir::document::HirDocument,
) -> Result<hharness_compiler::seal::CompiledBundle, hharness_compiler::errors::CompileError> {
    let (store, sealed, _v) = sealed_doc_with(tag, doc, vec![], vec![]);
    compile(
        &hharness_compiler::CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![hharness_compiler::profile::profile_coordinate(
                &test_profile(),
            )],
            fallback_profile: None,
            targets: vec![],
            compile_for_expired: false,
            intent_ref: None,
        },
        &MapProfileView::of(vec![test_profile()]),
        &store,
        &hharness_assembly::Stage1Catalog::stage1(),
        &kernel(),
    )
}

#[test]
fn compile_lands_the_typed_members() {
    // The authored `result_renderer`/`error_format` members lower into
    // the compiled surface verbatim — the authored fallback leg (the
    // profile carries no result_render/error_format rules).
    let rr = Json::obj([
        ("kind", Json::str("json_head_tail@1")),
        ("threshold", Json::Int(10_000)),
        ("head", Json::Int(5_000)),
        ("tail", Json::Int(5_000)),
        ("unit", Json::str("chars")),
        (
            "fields",
            Json::Arr(vec![
                Json::str("returncode"),
                Json::obj([
                    ("name", Json::str("output")),
                    ("truncate", Json::Bool(true)),
                    ("warning", Json::str("Output too long.")),
                ]),
                Json::str("exception_info?"),
            ]),
        ),
        ("json_style", Json::str("python_jinja_tojson")),
    ]);
    let ef = Json::obj([
        (
            "classes",
            Json::obj([
                ("no_tool_calls", Json::str("No tool calls found.")),
                ("args_unparseable", Json::str("Parse error: {{error}}")),
                ("unknown_tool", Json::str("Unknown tool '{{name}}'.")),
                ("missing_param", Json::str("Missing '{{name}}' argument.")),
                (
                    "output_truncated",
                    Json::str("limit (finish_reason={{finish_reason}})"),
                ),
            ]),
        ),
        ("format_failure_scope", Json::str("batch")),
    ]);
    let b = compile_surface("r636a", mini_tools_doc(rr, ef)).expect("compile");
    let hharness_compiler::seal::ModelSurfaceState::Lowered(s) = &b.model_surface else {
        panic!("lowered")
    };
    let t = &s.tools[0];
    let Some(hharness_compiler::surface::ResultRenderSpec {
        renderer:
            Some(ResultRenderer::JsonHeadTail {
                threshold,
                head,
                tail,
                unit,
                fields,
                json_style,
            }),
        ..
    }) = &t.result_render
    else {
        panic!("renderer: {:?}", t.result_render)
    };
    assert_eq!((*threshold, *head, *tail), (10_000, 5_000, 5_000));
    assert_eq!(*unit, HeadTailUnit::Chars);
    assert_eq!(*json_style, ResultJsonStyle::PythonJinjaTojson);
    assert!(fields[1].truncate && fields[2].optional);
    let Some(c) = t.error_format.as_ref().and_then(|e| e.classes.as_ref()) else {
        panic!("classes")
    };
    assert_eq!(c.scope, FormatFailureScope::Batch);
    assert_eq!(c.renderings.len(), 5);
    assert!(c.renderings[&FormatFailureClass::ArgsUnparseable]
        .content
        .as_deref()
        .unwrap_or("")
        .contains("{{error}}"));
}

#[test]
fn compile_refusals_are_typed() {
    let doc = |rr: Json| mini_tools_doc(rr, Json::Null);
    // Unknown `kind`.
    let rr = Json::obj([("kind", Json::str("bogus@1"))]);
    match compile_surface("r636b", doc(rr)) {
        Err(hharness_compiler::errors::CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("bogus@1"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
    // Zero / two `truncate` fields refuse.
    let rr = Json::obj([
        ("kind", Json::str("json_head_tail@1")),
        ("threshold", Json::Int(10)),
        ("head", Json::Int(4)),
        ("tail", Json::Int(4)),
        ("unit", Json::str("chars")),
        ("fields", Json::Arr(vec![Json::str("output")])),
        ("json_style", Json::str("canonical")),
    ]);
    match compile_surface("r636b", doc(rr)) {
        Err(hharness_compiler::errors::CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("truncate"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
    // A marker variable outside the renderer's slots ⇒ UnexpressibleSurface.
    let rr = Json::obj([
        ("kind", Json::str("text_head_tail@1")),
        ("threshold", Json::Int(10)),
        ("head", Json::Int(4)),
        ("tail", Json::Int(4)),
        ("unit", Json::str("utf8_bytes")),
        ("marker", Json::str("{{nope}} omitted")),
    ]);
    match compile_surface("r636b", doc(rr)) {
        Err(hharness_compiler::errors::CompileError::UnexpressibleSurface { reason, .. }) => {
            assert!(reason.contains("nope"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    // An `error_format.classes` key outside the closed set refuses.
    let mut d = mini_tools_doc(
        Json::Null,
        Json::obj([("classes", Json::obj([("bogus_class", Json::str("x"))]))]),
    );
    match compile_surface(
        "r636b",
        std::mem::replace(&mut d, doc_with(&stage1_assembly())),
    ) {
        Err(hharness_compiler::errors::CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("bogus_class"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
}
