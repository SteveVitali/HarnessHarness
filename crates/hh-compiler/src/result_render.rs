//! `result_render` — the typed pure renderers ADR-0442 D2 admits
//! (REQ-XE-110/111; design 38 rows G8/G9). These are *pure*: no clock,
//! env, host, or ledger reads — the caller supplies the result record /
//! failure slots and gets model-facing bytes back. R6.37 wires them
//! into the live observation path; this module is the render contract
//! they execute (the one home for the logic, CC10 — never a second
//! implementation beside `hh-tmpl/1` or the wire canonicalizer).
//!
//! - `json_head_tail@1` renders a result record as a pretty JSON
//!   object — mini's observation shape (research 28 §A.5): under
//!   `threshold` units the field renders whole; at/over it the field
//!   expands to `{name}_head`, `{name}_tail`, `elided_chars`, `warning`.
//! - `text_head_tail@1` renders the output string through head+tail
//!   elision with a `hh-tmpl/1` marker leaf (Terminus's shape — the
//!   marker fills `{threshold, head, tail, unit, omitted}`).
//! - `error_format.classes` render the typed format-failure classes
//!   (REQ-XE-111) — each `Text` leaf is a `hh-tmpl/1` template filled
//!   from the class's declared slots; `format_failure_scope = batch`
//!   renders one failure per response.

use std::collections::BTreeMap;

use hharness_hir::leaves::Text;
use hharness_wire::json::Json;

use crate::surface::{
    FormatErrorClasses, FormatFailureClass, FormatFailureScope, HeadTailUnit, ResultJsonStyle,
    ResultRenderer,
};
use crate::tmpl;

/// The renderer refusal sum — closed, typed (§5d.2's refusal discipline:
/// a spec that cannot serve the record is a typed refusal, never a
/// silently-fabricated render).
#[derive(Debug, Clone, PartialEq)]
pub enum ResultRenderError {
    /// A required `fields[]` member is absent from the result record.
    MissingField {
        /// The field name.
        name: String,
    },
    /// The `truncate` field's value is not a string.
    NonStringField {
        /// The field name.
        name: String,
    },
    /// A `hh-tmpl/1` leaf failed to compile or render — the
    /// `hh-tmpl/1` error is carried verbatim.
    Template {
        /// The failing leaf's role.
        role: String,
        /// The `hh-tmpl/1` error text.
        detail: String,
    },
    /// No `Text` rendering is declared for the failure class.
    NoRendering {
        /// The class spelling.
        class: String,
    },
}

impl std::fmt::Display for ResultRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultRenderError::MissingField { name } => {
                write!(f, "result_render: required field `{name}` absent")
            }
            ResultRenderError::NonStringField { name } => {
                write!(f, "result_render: truncate field `{name}` is not a string")
            }
            ResultRenderError::Template { role, detail } => {
                write!(f, "result_render: {role}: {detail}")
            }
            ResultRenderError::NoRendering { class } => {
                write!(f, "result_render: no rendering declared for `{class}`")
            }
        }
    }
}

impl std::error::Error for ResultRenderError {}

// ── json_style serializers ────────────────────────────────────────────

/// `python_tojson_str(s)` — Python `json.dumps(str)` under Jinja's
/// `tojson` policy: `ensure_ascii` (`\uXXXX` escapes, astral chars as
/// surrogate pairs) plus the htmlsafe escapes `<`→`<`, `>`→`>`,
/// `&`→`&`, `'`→`'`. Only O-recorded bytes
/// decide whether this spelling is faithful (X-08).
fn python_tojson_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\'' => out.push_str("\\u0027"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c if (c as u32) >= 0x80 => {
                let cp = c as u32;
                if cp <= 0xFFFF {
                    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", cp));
                } else {
                    // ensure_ascii renders astral chars as a UTF-16
                    // surrogate pair (Python's json.dumps spelling).
                    let v = cp - 0x1_0000;
                    let _ = std::fmt::Write::write_fmt(
                        &mut out,
                        format_args!(
                            "\\u{:04x}\\u{:04x}",
                            0xD800 + (v >> 10),
                            0xDC00 + (v & 0x3FF)
                        ),
                    );
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `python_dumps(v)` — `json.dumps` under Jinja's policy (`sort_keys`,
/// `", "`/`": "` separators at `indent=None`).
fn python_dumps(v: &Json) -> String {
    match v {
        Json::Str(s) => python_tojson_str(s),
        Json::Int(i) => i.to_string(),
        Json::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Json::Null => "null".to_string(),
        Json::Arr(items) => {
            let inner: Vec<String> = items.iter().map(python_dumps).collect();
            format!("[{}]", inner.join(", "))
        }
        Json::Obj(m) => {
            // `sort_keys: true` — Jinja's default dumps kwargs.
            let inner: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {}", python_tojson_str(k), python_dumps(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

/// `tojson(v, style)` — the member serializer `json_style` names.
pub fn tojson(v: &Json, style: ResultJsonStyle) -> String {
    match style {
        ResultJsonStyle::PythonJinjaTojson => python_dumps(v),
        ResultJsonStyle::Canonical => v.to_canonical_string(),
    }
}

// ── json_head_tail@1 ──────────────────────────────────────────────────

/// `render_json_head_tail(threshold, head, tail, unit, fields,
/// json_style, result)` — the `json_head_tail@1` pure render. The
/// record renders `{\n  "f1": v1` + `,\n  "f": v` for required fields
/// + `, "f": v` inline for optional fields + `\n}` (mini's template
/// shape, research 28 §A.5): `measure(output) >= threshold` expands the
/// `truncate` field to `{name}_head`, `{name}_tail`, `elided_chars`
/// (= `measure − threshold`, the template's own arithmetic), `warning`.
pub fn render_json_head_tail(
    threshold: u64,
    head: u64,
    tail: u64,
    unit: HeadTailUnit,
    fields: &[crate::surface::ResultRenderField],
    json_style: ResultJsonStyle,
    result: &BTreeMap<String, Json>,
) -> Result<String, ResultRenderError> {
    let mut members: Vec<(String, String, bool)> = Vec::new();
    for f in fields {
        let v = result.get(&f.name);
        let Some(v) = v else {
            if f.optional {
                continue;
            }
            return Err(ResultRenderError::MissingField {
                name: f.name.clone(),
            });
        };
        if f.truncate {
            let Json::Str(s) = v else {
                return Err(ResultRenderError::NonStringField {
                    name: f.name.clone(),
                });
            };
            if unit.measure(s) >= threshold {
                members.push((
                    format!("{}_head", f.name),
                    tojson(&Json::str(unit.head(s, head)), json_style),
                    false,
                ));
                members.push((
                    format!("{}_tail", f.name),
                    tojson(&Json::str(unit.tail(s, tail)), json_style),
                    false,
                ));
                members.push((
                    "elided_chars".to_string(),
                    unit.measure(s).saturating_sub(threshold).to_string(),
                    false,
                ));
                if let Some(w) = &f.warning {
                    members.push((
                        "warning".to_string(),
                        tojson(&Json::str(w.clone()), json_style),
                        false,
                    ));
                }
                continue;
            }
        }
        members.push((
            f.name.clone(),
            match v {
                // `{{ output.returncode }}` renders the literal, not a
                // tojson string — ints/bools bare, strings/objects
                // through `json_style`.
                Json::Int(i) => i.to_string(),
                Json::Bool(b) => if *b { "true" } else { "false" }.to_string(),
                Json::Null => "null".to_string(),
                other => tojson(other, json_style),
            },
            f.optional,
        ));
    }
    let mut out = String::from("{");
    let mut first = true;
    for (name, value, optional) in &members {
        if first {
            out.push_str(&format!("\n  \"{name}\": {value}"));
            first = false;
        } else if *optional {
            // Optional members append inline — mini's `{%- if %}
            // , "exception_info": …` shape.
            out.push_str(&format!(", \"{name}\": {value}"));
        } else {
            out.push_str(&format!(",\n  \"{name}\": {value}"));
        }
    }
    out.push_str("\n}");
    Ok(out)
}

// ── text_head_tail@1 ──────────────────────────────────────────────────

/// `render_text_head_tail(threshold, head, tail, unit, marker, output)`
/// — the `text_head_tail@1` pure render (Terminus's `_limit_output_length`
/// shape, research 28 §B.4): `measure(output) >= threshold` renders
/// `head + marker + tail`; the marker's `hh-tmpl/1` leaf fills
/// `{threshold, head, tail, unit, omitted}` where `omitted =
/// measure − threshold` (the template's own arithmetic).
pub fn render_text_head_tail(
    threshold: u64,
    head: u64,
    tail: u64,
    unit: HeadTailUnit,
    marker: &Text,
    output: &str,
) -> Result<String, ResultRenderError> {
    if unit.measure(output) < threshold {
        return Ok(output.to_string());
    }
    let t = tmpl::compile_template(marker.content.as_deref().unwrap_or_default()).map_err(|e| {
        ResultRenderError::Template {
            role: "marker".to_string(),
            detail: format!("{e}"),
        }
    })?;
    let vars: BTreeMap<String, String> = [
        ("threshold", threshold.to_string()),
        ("head", head.to_string()),
        ("tail", tail.to_string()),
        ("unit", unit.as_str().to_string()),
        (
            "omitted",
            unit.measure(output).saturating_sub(threshold).to_string(),
        ),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();
    let mark = t.render(&vars).map_err(|e| ResultRenderError::Template {
        role: "marker".to_string(),
        detail: format!("{e}"),
    })?;
    Ok(format!(
        "{}{}{}",
        unit.head(output, head),
        mark,
        unit.tail(output, tail)
    ))
}

/// `render(spec, result)` — the `ResultRenderer` dispatch: the JSON
/// variant reads the declared `fields[]` from the record; the text
/// variant reads the `output` member (a non-string/absent `output` is
/// the same typed refusal shape).
pub fn render(
    spec: &ResultRenderer,
    result: &BTreeMap<String, Json>,
) -> Result<String, ResultRenderError> {
    match spec {
        ResultRenderer::JsonHeadTail {
            threshold,
            head,
            tail,
            unit,
            fields,
            json_style,
        } => render_json_head_tail(*threshold, *head, *tail, *unit, fields, *json_style, result),
        ResultRenderer::TextHeadTail {
            threshold,
            head,
            tail,
            unit,
            marker,
        } => {
            let s = result.get("output").and_then(Json::as_str).ok_or_else(|| {
                ResultRenderError::NonStringField {
                    name: "output".to_string(),
                }
            })?;
            render_text_head_tail(*threshold, *head, *tail, *unit, marker, s)
        }
    }
}

// ── error_format classes (REQ-XE-111) ─────────────────────────────────

/// `render_format_error(classes, class, slots)` — render one typed
/// format failure from the class's `Text` leaf (`hh-tmpl/1`), filled
/// from `slots` (the class's declared set: `args_unparseable{error}`,
/// `unknown_tool{name}`, `missing_param{name}`,
/// `output_truncated{finish_reason}`; `no_tool_calls` takes none). A
/// class the declaration does not name is `NoRendering` — a typed
/// refusal, never a synthesized string.
pub fn render_format_error(
    classes: &FormatErrorClasses,
    class: FormatFailureClass,
    slots: &BTreeMap<String, String>,
) -> Result<String, ResultRenderError> {
    let leaf = classes
        .renderings
        .get(&class)
        .ok_or_else(|| ResultRenderError::NoRendering {
            class: class.as_str().to_string(),
        })?;
    let t = tmpl::compile_template(leaf.content.as_deref().unwrap_or_default()).map_err(|e| {
        ResultRenderError::Template {
            role: class.as_str().to_string(),
            detail: format!("{e}"),
        }
    })?;
    // Fill only the class's declared slots — the leaf's variable set
    // ⊆ the declared set was checked at compile; extra provided keys
    // never reach the render.
    let vars: BTreeMap<String, String> = class
        .slots()
        .iter()
        .filter_map(|s| slots.get(*s).map(|v| (s.to_string(), v.clone())))
        .collect();
    t.render(&vars).map_err(|e| ResultRenderError::Template {
        role: class.as_str().to_string(),
        detail: format!("{e}"),
    })
}

/// `render_response_errors(classes, failures)` — one failure render per
/// `(class, slots)` pair; `format_failure_scope = batch` renders only
/// the first pair's (mini's shape: the first malformed call raises the
/// response's format error — REQ-XE-111's batch scope).
pub fn render_response_errors(
    classes: &FormatErrorClasses,
    failures: &[(FormatFailureClass, BTreeMap<String, String>)],
) -> Result<Vec<String>, ResultRenderError> {
    let it: &[(FormatFailureClass, BTreeMap<String, String>)] = match classes.scope {
        FormatFailureScope::Batch => &failures[..failures.len().min(1)],
        FormatFailureScope::PerCall => failures,
    };
    it.iter()
        .map(|(c, slots)| render_format_error(classes, *c, slots))
        .collect()
}
