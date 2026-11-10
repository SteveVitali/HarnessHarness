//! The R-2.5.2⁰ surface records (§5d.2 §3; ADR-0090 D4/D5, ADR-0092 D1/D5/D6;
//! the ADR-0146 C0/Stage-1 schema slice): the extended `SurfaceBinding` members
//! ([`crate::equiv::SurfaceBinding`]), the `SurfaceFailure` closed sum, the
//! `ResultRenderSpec` (`full | truncate` at C0 — `concise`/`offload` are the C1
//! row) and the `ErrorFormatSpec`.
//!
//! Identity (CC1): `surface_id = idp("tool_surface", H(surface record))` — the
//! `hharness_identity::RecordKind::ToolSurface` domain; a rename changes the
//! `surface_id` and the profile hash only, never a `capability_refs` member
//! (E7/AC-R-2.5.2-2).

use std::collections::BTreeMap;

use hharness_hir::leaves::Text;
use hharness_identity::idp::idp_id;
use hharness_identity::RecordKind;
use hharness_wire::json::Json;

/// The compile-time `exposure_mode` sum of a `SurfaceBinding` (§5d.2 — how a
/// capability set is *rendered*: `primitive | split | composite | freeform |
/// code_mode | shim`; ADR-0090 D7). **Distinct** from the run-time
/// [`hharness_hir::tools::ExposureMode`] (`direct | indexed | deferred | code_mode |
/// hidden` — how a compiled surface is *delivered* per call). Only `primitive`
/// is produced at C0; the rest are declared so the sum is closed (CC8 —
/// extension members land with their stage, never by open spelling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileExposureMode {
    /// A 1:1 function surface (C0).
    Primitive,
    /// One capability, several surfaces (`const`-fixed splits; C1).
    Split,
    /// A `PlanMap` over several capabilities (C1 static; C2 session_state).
    Composite,
    /// One `parse(grammar_ref)` transform (C2).
    Freeform,
    /// Callable only from an executed program (C2).
    CodeMode,
    /// Model-parsed text → target `SurfaceArgMap` (C2).
    Shim,
}

impl CompileExposureMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CompileExposureMode::Primitive => "primitive",
            CompileExposureMode::Split => "split",
            CompileExposureMode::Composite => "composite",
            CompileExposureMode::Freeform => "freeform",
            CompileExposureMode::CodeMode => "code_mode",
            CompileExposureMode::Shim => "shim",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<CompileExposureMode> {
        match s {
            "primitive" => Some(CompileExposureMode::Primitive),
            "split" => Some(CompileExposureMode::Split),
            "composite" => Some(CompileExposureMode::Composite),
            "freeform" => Some(CompileExposureMode::Freeform),
            "code_mode" => Some(CompileExposureMode::CodeMode),
            "shim" => Some(CompileExposureMode::Shim),
            _ => None,
        }
    }
}

/// `mapping: SurfaceArgMap | PlanMap` (§5d.2 §3) — the kind tag on a
/// `SurfaceBinding`. At C0 every binding is `SurfaceArgMap` (the `arg_map`
/// member carries the map); `PlanMap` names a pinned `Procedure` `version_id`
/// (the C1 composite shape — declared, never produced at C0).
#[derive(Debug, Clone, PartialEq)]
pub enum BindingMapping {
    /// `SurfaceArgMap` — the binding's `arg_map` member holds it.
    SurfaceArgMap,
    /// `PlanMap` — a pinned `Procedure` `version_id` (C1; refused at C0 compile).
    PlanMap(String),
}

impl BindingMapping {
    /// The canonical kind spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            BindingMapping::SurfaceArgMap => "surface_arg_map",
            BindingMapping::PlanMap(_) => "plan_map",
        }
    }
}

/// `SurfaceFailure` — the closed pre-dispatch failure sum (§5d.2 §3; ADR-0092
/// D5): detected by the parser or the reference monitor **before dispatch** —
/// no executor runs, no `Effect` exists; ledgered
/// `action.tool.surface_rejected{surface_id, binding_ref, failure_class,
/// raw_call_hash, model_call_id, rendering_ref}` and counted by
/// `surface_rejection_rate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceFailure {
    /// The call bytes did not parse (`Unparseable` at the gateway).
    Unparseable,
    /// The named surface is not in the plan (`unknown_surface` — a shim
    /// `resolve_name` miss lands here too; never a best-effort match).
    UnknownSurface,
    /// A surface argument absent from the `SurfaceArgMap`.
    UnmappedArgument,
    /// An argument outside the capability's declared domain.
    DomainViolation,
    /// A required argument absent.
    MissingRequired,
    /// The profile admits at most one call per block; the model emitted more.
    MultipleCallsUnsupported,
    /// A `parse(grammar_ref)`-shaped call violated the declared grammar.
    GrammarViolation,
    /// A `session_state` composite call referenced unknown state (C2 shape —
    /// declared for closure).
    SessionStateInvalid,
}

impl SurfaceFailure {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SurfaceFailure::Unparseable => "unparseable",
            SurfaceFailure::UnknownSurface => "unknown_surface",
            SurfaceFailure::UnmappedArgument => "unmapped_argument",
            SurfaceFailure::DomainViolation => "domain_violation",
            SurfaceFailure::MissingRequired => "missing_required",
            SurfaceFailure::MultipleCallsUnsupported => "multiple_calls_unsupported",
            SurfaceFailure::GrammarViolation => "grammar_violation",
            SurfaceFailure::SessionStateInvalid => "session_state_invalid",
        }
    }

    /// Every member, in declaration order (the E5 distinguishability set).
    pub const ALL: [SurfaceFailure; 8] = [
        SurfaceFailure::Unparseable,
        SurfaceFailure::UnknownSurface,
        SurfaceFailure::UnmappedArgument,
        SurfaceFailure::DomainViolation,
        SurfaceFailure::MissingRequired,
        SurfaceFailure::MultipleCallsUnsupported,
        SurfaceFailure::GrammarViolation,
        SurfaceFailure::SessionStateInvalid,
    ];

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<SurfaceFailure> {
        SurfaceFailure::ALL
            .iter()
            .copied()
            .find(|f| f.as_str() == s)
    }
}

/// The `action.tool.surface_rejected` payload — `{surface_id, binding_ref,
/// failure_class, raw_call_hash, model_call_id, rendering_ref, family}`
/// (ADR-0092 D5; ADR-0026 P2 log). Content by `raw_call_hash`, never the
/// call (Rule C).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceRejected {
    /// The surface the call named, when it resolved.
    pub surface_id: Option<String>,
    /// The `SurfaceBinding` coordinate (`surface_id`), when bound.
    pub binding_ref: Option<String>,
    /// The `SurfaceFailure` class.
    pub failure_class: SurfaceFailure,
    /// `H(raw call bytes)` — the call content never enters the row.
    pub raw_call_hash: String,
    /// The producing model call.
    pub model_call_id: String,
    /// The error rendering used (`rendering_ref` names surface + mode).
    pub rendering_ref: Option<String>,
    /// The binding's shape family (`family_id`) — `surface_rejection_rate`'s
    /// per-(profile, family) stratum member (R3.5); `None` on an
    /// unresolved/never-bound call, never guessed.
    pub family: Option<String>,
}

/// The payload JSON of a `surface_rejected` row.
pub fn surface_rejected_payload(r: &SurfaceRejected) -> Json {
    Json::obj([
        (
            "surface_id",
            r.surface_id.clone().map_or(Json::Null, Json::str),
        ),
        (
            "binding_ref",
            r.binding_ref.clone().map_or(Json::Null, Json::str),
        ),
        ("failure_class", Json::str(r.failure_class.as_str())),
        ("raw_call_hash", Json::str(r.raw_call_hash.clone())),
        ("model_call_id", Json::str(r.model_call_id.clone())),
        (
            "rendering_ref",
            r.rendering_ref.clone().map_or(Json::Null, Json::str),
        ),
        ("family", r.family.clone().map_or(Json::Null, Json::str)),
    ])
}

// ── ResultRenderSpec (ADR-0092 D1/D2) ────────────────────────────────────────

/// `direction ∈ {head, tail}` — which end a `truncate` keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TruncateDirection {
    /// Keep the head.
    Head,
    /// Keep the tail.
    Tail,
}

impl TruncateDirection {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TruncateDirection::Head => "head",
            TruncateDirection::Tail => "tail",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<TruncateDirection> {
        match s {
            "head" => Some(TruncateDirection::Head),
            "tail" => Some(TruncateDirection::Tail),
            _ => None,
        }
    }
}

/// `ResultRenderSpec.mode` — `{full, truncate{max_lines, max_bytes,
/// max_tokens, direction}, concise{format_param}, offload{threshold_bytes,
/// preview_lines ≤ threshold_bytes, artifact_kind}}`. `concise`/`offload`
/// are the C1 members (S1.17/R2.8 — §5d.2's conditioned modes; each carries
/// a debt record like every conditioned rule).
#[derive(Debug, Clone, PartialEq)]
pub enum RenderMode {
    /// `full` — the reference mode; every other mode is a conditioned rule
    /// with a debt record (compression is never a default).
    Full,
    /// `truncate{max_lines, max_bytes, max_tokens, direction ∈ {head, tail}}`.
    Truncate {
        /// The line cap.
        max_lines: Option<u64>,
        /// The byte cap.
        max_bytes: Option<u64>,
        /// The token cap.
        max_tokens: Option<u64>,
        /// Which end is kept.
        direction: TruncateDirection,
    },
    /// `concise{format_param}` — the concise rendering; `format_param` names
    /// the *declared* format member the summary follows (closed spellings —
    /// a free-text format is `BadMember`, never a prose slot).
    Concise {
        /// The declared format parameter.
        format_param: String,
    },
    /// `offload{threshold_bytes, preview_lines, artifact_kind}` — results
    /// over `threshold_bytes` land as a declared artefact (read through the
    /// bound `read_artifact` capability — E6 leg); the surface keeps a
    /// `preview_lines` head. `preview_lines ≤ threshold_bytes` is a
    /// parse-time bound ([`RenderSpecParseError::OffloadBound`]).
    Offload {
        /// The offload threshold (result bytes).
        threshold_bytes: u64,
        /// The preview line count the surface retains.
        preview_lines: u64,
        /// The artefact kind the body lands under (a declared kind —
        /// `result_body`, `transcript_segment`, …).
        artifact_kind: String,
    },
}

impl RenderMode {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Full => "full",
            RenderMode::Truncate { .. } => "truncate",
            RenderMode::Concise { .. } => "concise",
            RenderMode::Offload { .. } => "offload",
        }
    }

    /// `parse(kind, members)` — the shared mode parser (CC1 — the profile
    /// params' flat shape and the codec's `mode{kind, …}` shape both read
    /// members through this one grammar). Unknown spellings and malformed
    /// members refuse typed, never silently widened.
    pub fn parse(kind: &str, members: &Json) -> Result<RenderMode, RenderSpecParseError> {
        let u64_member = |k: &str| -> Result<Option<u64>, RenderSpecParseError> {
            match members.get(k) {
                None | Some(Json::Null) => Ok(None),
                Some(v) => v.as_int().map(|i| Some(i.max(0) as u64)).ok_or_else(|| {
                    RenderSpecParseError::BadMember {
                        mode: kind.to_string(),
                        member: k.to_string(),
                        reason: "expected a non-negative integer".to_string(),
                    }
                }),
            }
        };
        let str_member = |k: &str| -> Result<Option<String>, RenderSpecParseError> {
            match members.get(k) {
                None | Some(Json::Null) => Ok(None),
                Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                    RenderSpecParseError::BadMember {
                        mode: kind.to_string(),
                        member: k.to_string(),
                        reason: "expected a string".to_string(),
                    }
                }),
            }
        };
        match kind {
            "full" => Ok(RenderMode::Full),
            "truncate" => Ok(RenderMode::Truncate {
                max_lines: u64_member("max_lines")?,
                max_bytes: u64_member("max_bytes")?,
                max_tokens: u64_member("max_tokens")?,
                direction: match str_member("direction")? {
                    Some(d) => TruncateDirection::parse(&d).ok_or_else(|| {
                        RenderSpecParseError::BadMember {
                            mode: kind.to_string(),
                            member: "direction".to_string(),
                            reason: format!("direction ∈ {{head, tail}}; got {d}"),
                        }
                    })?,
                    None => TruncateDirection::Head,
                },
            }),
            "concise" => Ok(RenderMode::Concise {
                format_param: str_member("format_param")?.ok_or_else(|| {
                    RenderSpecParseError::MissingMember {
                        mode: kind.to_string(),
                        member: "format_param".to_string(),
                    }
                })?,
            }),
            "offload" => {
                let threshold_bytes = u64_member("threshold_bytes")?.ok_or_else(|| {
                    RenderSpecParseError::MissingMember {
                        mode: kind.to_string(),
                        member: "threshold_bytes".to_string(),
                    }
                })?;
                let preview_lines = u64_member("preview_lines")?.ok_or_else(|| {
                    RenderSpecParseError::MissingMember {
                        mode: kind.to_string(),
                        member: "preview_lines".to_string(),
                    }
                })?;
                if preview_lines > threshold_bytes {
                    return Err(RenderSpecParseError::OffloadBound {
                        preview_lines,
                        threshold_bytes,
                    });
                }
                Ok(RenderMode::Offload {
                    threshold_bytes,
                    preview_lines,
                    artifact_kind: str_member("artifact_kind")?.ok_or_else(|| {
                        RenderSpecParseError::MissingMember {
                            mode: kind.to_string(),
                            member: "artifact_kind".to_string(),
                        }
                    })?,
                })
            }
            other => Err(RenderSpecParseError::UnknownMode {
                mode: other.to_string(),
            }),
        }
    }

    /// The codec's `mode{kind, …}` member body (canonical JSON).
    pub fn mode_json(&self) -> Json {
        match self {
            RenderMode::Full => Json::obj([("kind", Json::str("full"))]),
            RenderMode::Truncate {
                max_lines,
                max_bytes,
                max_tokens,
                direction,
            } => {
                let mut p = vec![
                    ("kind", Json::str("truncate")),
                    ("direction", Json::str(direction.as_str())),
                ];
                if let Some(v) = max_lines {
                    p.push(("max_lines", Json::Int(*v as i64)));
                }
                if let Some(v) = max_bytes {
                    p.push(("max_bytes", Json::Int(*v as i64)));
                }
                if let Some(v) = max_tokens {
                    p.push(("max_tokens", Json::Int(*v as i64)));
                }
                Json::obj(p)
            }
            RenderMode::Concise { format_param } => Json::obj([
                ("kind", Json::str("concise")),
                ("format_param", Json::str(format_param.clone())),
            ]),
            RenderMode::Offload {
                threshold_bytes,
                preview_lines,
                artifact_kind,
            } => Json::obj([
                ("kind", Json::str("offload")),
                ("threshold_bytes", Json::Int(*threshold_bytes as i64)),
                ("preview_lines", Json::Int(*preview_lines as i64)),
                ("artifact_kind", Json::str(artifact_kind.clone())),
            ]),
        }
    }
}

/// The `RenderMode::parse` refusal sum — closed, typed (§5d.2's
/// `render_spec_parse` error column).
#[derive(Debug, Clone, PartialEq)]
pub enum RenderSpecParseError {
    /// A spelling outside `{full, truncate, concise, offload}`.
    UnknownMode {
        /// The spelling seen.
        mode: String,
    },
    /// A declared-required member absent (`concise.format_param`,
    /// `offload.{threshold_bytes, preview_lines, artifact_kind}`).
    MissingMember {
        /// The mode.
        mode: String,
        /// The member.
        member: String,
    },
    /// A member present but mistyped/out-of-domain.
    BadMember {
        /// The mode.
        mode: String,
        /// The member.
        member: String,
        /// The reason.
        reason: String,
    },
    /// `offload` with `preview_lines > threshold_bytes` — the declared
    /// bound is violated (§5d.2 `preview_lines ≤ threshold_bytes`).
    OffloadBound {
        /// The declared preview.
        preview_lines: u64,
        /// The declared threshold.
        threshold_bytes: u64,
    },
}

impl std::fmt::Display for RenderSpecParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderSpecParseError::UnknownMode { mode } => {
                write!(f, "render_spec_parse: unknown mode {mode}")
            }
            RenderSpecParseError::MissingMember { mode, member } => {
                write!(f, "render_spec_parse: {mode} needs {member}")
            }
            RenderSpecParseError::BadMember {
                mode,
                member,
                reason,
            } => write!(f, "render_spec_parse: {mode}.{member}: {reason}"),
            RenderSpecParseError::OffloadBound {
                preview_lines,
                threshold_bytes,
            } => write!(
                f,
                "render_spec_parse: offload preview_lines {preview_lines} > threshold_bytes {threshold_bytes}"
            ),
        }
    }
}

impl std::error::Error for RenderSpecParseError {}

/// `ResultRenderSpec{mode, declared_loss: truncated{retained_fields[]}?,
/// validator_reads: [field]}` (§5d.2 §3; ADR-0092 D1) — the profile-owned
/// surface record written by `result_render` rules. E6 reads
/// `validator_reads ⊆ retained_fields` for every Validator bound to the
/// capability's results (the check itself is the C0/S3 row; the record lands
/// here). `renderer` carries the R6.36 typed renderers (REQ-XE-110; AR-23)
/// — absent-not-null: a spec declaring none is byte-identical (CC8).
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRenderSpec {
    /// The mode (`full | truncate | concise | offload` — C1 lands the last
    /// two; S1.17/R2.8).
    pub mode: RenderMode,
    /// The declared truncation loss — the `retained_fields` the conditioned
    /// rendering keeps (mandatory when `mode` drops fields; `None` on
    /// `full`).
    pub declared_loss: Option<Vec<String>>,
    /// The fields bound validators read — E6's static inclusion set.
    pub validator_reads: Vec<String>,
    /// The typed pure renderer (`json_head_tail@1` / `text_head_tail@1`) —
    /// the sweep's typed-parameter constants (G8), never string edits.
    pub renderer: Option<ResultRenderer>,
}

// ── the typed result renderers (ADR-0442 D2; REQ-XE-110; design 38 G8) ──

/// `unit ∈ {chars, utf8_bytes}` — the head/tail measure. `chars` counts
/// Unicode scalar values (mini's Python `str` length — design 38 Part 4);
/// `utf8_bytes` counts UTF-8 encoded bytes (Terminus's `max_bytes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadTailUnit {
    /// Unicode scalar values (Rust `chars` = Python code points for
    /// non-surrogate text).
    Chars,
    /// UTF-8 bytes (`s.len()`).
    Utf8Bytes,
}

impl HeadTailUnit {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HeadTailUnit::Chars => "chars",
            HeadTailUnit::Utf8Bytes => "utf8_bytes",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<HeadTailUnit> {
        match s {
            "chars" => Some(HeadTailUnit::Chars),
            "utf8_bytes" => Some(HeadTailUnit::Utf8Bytes),
            _ => None,
        }
    }

    /// `measure(s)` — the length in this unit.
    pub fn measure(self, s: &str) -> u64 {
        match self {
            HeadTailUnit::Chars => s.chars().count() as u64,
            HeadTailUnit::Utf8Bytes => s.len() as u64,
        }
    }

    /// `head(s, n)` — the first `n` units. Under `utf8_bytes` a
    /// mid-codepoint cut decodes lossily (Python's `errors="replace"`
    /// — each malformed byte renders `U+FFFD`, never a panic and never
    /// silent data loss past the boundary).
    pub fn head(self, s: &str, n: u64) -> String {
        match self {
            HeadTailUnit::Chars => s.chars().take(n as usize).collect(),
            HeadTailUnit::Utf8Bytes => {
                let b = s.as_bytes();
                String::from_utf8_lossy(&b[..(n as usize).min(b.len())]).into_owned()
            }
        }
    }

    /// `tail(s, n)` — the last `n` units (same lossy-boundary rule).
    pub fn tail(self, s: &str, n: u64) -> String {
        match self {
            HeadTailUnit::Chars => {
                let cs: Vec<char> = s.chars().collect();
                cs.iter()
                    .skip(cs.len().saturating_sub(n as usize))
                    .collect()
            }
            HeadTailUnit::Utf8Bytes => {
                let b = s.as_bytes();
                String::from_utf8_lossy(&b[b.len().saturating_sub(n as usize)..]).into_owned()
            }
        }
    }
}

/// `json_style` — the value serializer `json_head_tail@1` applies to
/// non-numeric members (X-08: `python_jinja_tojson` is a declared
/// spelling admitted for goldens that match O's recorded bytes;
/// hand-derived goldens are `provenance: hand_derived`, never evidence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultJsonStyle {
    /// Python `json.dumps` under Jinja's `tojson` policy (`sort_keys`,
    /// `ensure_ascii`, the htmlsafe `<>&'` escapes).
    PythonJinjaTojson,
    /// The workspace canonical JSON (`to_canonical_string`).
    Canonical,
}

impl ResultJsonStyle {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ResultJsonStyle::PythonJinjaTojson => "python_jinja_tojson",
            ResultJsonStyle::Canonical => "canonical",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<ResultJsonStyle> {
        match s {
            "python_jinja_tojson" => Some(ResultJsonStyle::PythonJinjaTojson),
            "canonical" => Some(ResultJsonStyle::Canonical),
            _ => None,
        }
    }
}

/// One `json_head_tail@1.fields[]` member — a result-record field the
/// renderer emits, in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRenderField {
    /// The result member's name.
    pub name: String,
    /// Optional — an absent input member omits the line entirely (never
    /// `null`). Optional members render inline (`, "name": v`), required
    /// members newline-separated (mini's template shape).
    pub optional: bool,
    /// The head/tail truncation subject — exactly one per renderer.
    pub truncate: bool,
    /// The `warning` literal the truncated expansion emits after
    /// `elided_chars` (`None` ⇒ no warning member).
    pub warning: Option<String>,
}

/// `ResultRenderer` — the typed pure renderers AR-23 admits (REQ-XE-110).
/// `json_head_tail@1` renders a result record as a pretty JSON object
/// (mini's observation shape — head/tail under `threshold`, member
/// descriptions of the `elided_chars`/`warning` pair included);
/// `text_head_tail@1` renders the output string through head+tail
/// elision with a `marker` `hh-tmpl/1` `Text` leaf (Terminus's shape).
/// Every constant is a typed parameter — the sweep varies members,
/// never template strings.
#[derive(Debug, Clone, PartialEq)]
pub enum ResultRenderer {
    /// `json_head_tail@1{threshold, head, tail, unit, fields, json_style}`.
    JsonHeadTail {
        /// `measure(output) >= threshold` truncates (mini's `<` test).
        threshold: u64,
        /// Head units kept.
        head: u64,
        /// Tail units kept.
        tail: u64,
        /// The measure.
        unit: HeadTailUnit,
        /// The emitted fields, in order; exactly one `truncate`.
        fields: Vec<ResultRenderField>,
        /// The member serializer.
        json_style: ResultJsonStyle,
    },
    /// `text_head_tail@1{threshold, head, tail, unit, marker}`.
    TextHeadTail {
        /// `measure(output) >= threshold` elides.
        threshold: u64,
        /// Head units kept.
        head: u64,
        /// Tail units kept.
        tail: u64,
        /// The measure.
        unit: HeadTailUnit,
        /// The elision marker — a `hh-tmpl/1` `Text` leaf rendered with
        /// `{threshold, head, tail, unit, omitted}` (the declared slot set).
        marker: Text,
    },
}

impl ResultRenderer {
    /// The `kind` spelling the `result_renderer` member declares.
    pub fn kind(&self) -> &'static str {
        match self {
            ResultRenderer::JsonHeadTail { .. } => "json_head_tail@1",
            ResultRenderer::TextHeadTail { .. } => "text_head_tail@1",
        }
    }

    /// The marker template's admitted slots (`text_head_tail@1`) —
    /// the renderer fills every one at render time; a marker naming
    /// any other variable is `UnexpressibleSurface` at compile.
    pub const MARKER_SLOTS: [&'static str; 5] = ["threshold", "head", "tail", "unit", "omitted"];
}

/// The `check_render_admissible` refusals (§5d.2 E6; R2.8) — closed sum.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderAdmissibilityError {
    /// `validator_reads ⊄ retained_fields` — a bound validator reads a field
    /// the conditioned rendering drops (the spec's E6 inclusion leg).
    ValidatorReadsDropped {
        /// The unretained fields.
        fields: Vec<String>,
    },
    /// `offload` requires a `read_artifact` capability bound to the
    /// surface — the body lands as an artefact a validator/consumer reads
    /// back; without the read leg the offloaded body is unreachable (the
    /// spec's offload admissibility row).
    OffloadWithoutReadArtifact,
}

impl std::fmt::Display for RenderAdmissibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderAdmissibilityError::ValidatorReadsDropped { fields } => write!(
                f,
                "render_admissible: validator_reads {} not in retained_fields",
                fields.join(",")
            ),
            RenderAdmissibilityError::OffloadWithoutReadArtifact => write!(
                f,
                "render_admissible: offload needs a bound read_artifact capability"
            ),
        }
    }
}

impl std::error::Error for RenderAdmissibilityError {}

/// `check_render_admissible(spec, has_read_artifact)` — the §5d.2 E6 legs
/// (R2.8): `offload` needs a bound `read_artifact` capability
/// (`has_read_artifact` is the caller's binding-time fact, never a guess);
/// `validator_reads ⊆ retained_fields` when the spec declares a retention
/// set (`declared_loss`). `full`/`None` retention trivially retains every
/// field.
pub fn check_render_admissible(
    spec: &ResultRenderSpec,
    has_read_artifact: bool,
) -> Result<(), RenderAdmissibilityError> {
    if matches!(spec.mode, RenderMode::Offload { .. }) && !has_read_artifact {
        return Err(RenderAdmissibilityError::OffloadWithoutReadArtifact);
    }
    if let Some(retained) = &spec.declared_loss {
        let dropped: Vec<String> = spec
            .validator_reads
            .iter()
            .filter(|r| !retained.contains(r))
            .cloned()
            .collect();
        if !dropped.is_empty() {
            return Err(RenderAdmissibilityError::ValidatorReadsDropped { fields: dropped });
        }
    }
    Ok(())
}

// ── ErrorFormatSpec (ADR-0092 D6) ────────────────────────────────────────────

/// `distinguishability` — the E5 verdict on the spec's `renderings` map.
/// `verified` is the only passing value; `unchecked`/`failed` are honest
/// non-verdicts (T-LCD-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distinguishability {
    /// E5 passed — every rendering is pairwise distinguishable.
    Verified,
    /// The static check has not run.
    Unchecked,
    /// E5 ran and found an indistinguishable pair.
    Failed,
}

impl Distinguishability {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Distinguishability::Verified => "verified",
            Distinguishability::Unchecked => "unchecked",
            Distinguishability::Failed => "failed",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "verified" => Self::Verified,
            "unchecked" => Self::Unchecked,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// `ErrorFormatSpec{renderings: map<SurfaceFailure ∪ CapabilityFailureClass,
/// Text>, distinguishability}` (§5d.2 §3; ADR-0092 D6) — the `error_format`
/// rule's record. Renderings are `Text` leaves counted under `profile_prose`;
/// E5 ranges over both failure sets (the `SurfaceFailure` spellings plus the
/// capability `error_classes` — the map key domain is their union).
/// `classes` carries the R6.36 typed format-failure renderings (REQ-XE-111;
/// AR-23/G9) — absent-not-null (CC8).
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorFormatSpec {
    /// `failure-class spelling → Text` rendering (canonical order by key).
    pub renderings: BTreeMap<String, Text>,
    /// The E5 distinguishability verdict.
    pub distinguishability: Distinguishability,
    /// The typed format-failure classes — model-facing `Text` renderings
    /// for `model.format_failure` outcomes (the bad assistant turn is
    /// omitted; the class's own leaf renders — G9).
    pub classes: Option<FormatErrorClasses>,
}

/// `FormatFailureClass` — the typed format-failure classes (REQ-XE-111;
/// design 38 G9): `no_tool_calls`, `args_unparseable{error}`,
/// `unknown_tool{name}`, `missing_param{name}`,
/// `output_truncated{finish_reason}`. The `{…}` members are the class's
/// declared slot set — its `Text` leaf's `hh-tmpl/1` variables must be a
/// subset at compile (strict undefined's compile half).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FormatFailureClass {
    /// `no_tool_calls` — the response carried no tool calls.
    NoToolCalls,
    /// `args_unparseable{error}` — the call's argument bytes did not
    /// parse; `{error}` carries the parse error text.
    ArgsUnparseable,
    /// `unknown_tool{name}` — the call named an unbound surface.
    UnknownTool,
    /// `missing_param{name}` — a required argument absent.
    MissingParam,
    /// `output_truncated{finish_reason}` — the response hit the output
    /// limit before a call (`{finish_reason}` = `length`/`tool_calls`
    /// family).
    OutputTruncated,
}

impl FormatFailureClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FormatFailureClass::NoToolCalls => "no_tool_calls",
            FormatFailureClass::ArgsUnparseable => "args_unparseable",
            FormatFailureClass::UnknownTool => "unknown_tool",
            FormatFailureClass::MissingParam => "missing_param",
            FormatFailureClass::OutputTruncated => "output_truncated",
        }
    }

    /// The declared slot set — the leaf's `hh-tmpl/1` variable bound.
    pub fn slots(self) -> &'static [&'static str] {
        match self {
            FormatFailureClass::NoToolCalls => &[],
            FormatFailureClass::ArgsUnparseable => &["error"],
            FormatFailureClass::UnknownTool => &["name"],
            FormatFailureClass::MissingParam => &["name"],
            FormatFailureClass::OutputTruncated => &["finish_reason"],
        }
    }

    /// Every member, in declaration order.
    pub const ALL: [FormatFailureClass; 5] = [
        FormatFailureClass::NoToolCalls,
        FormatFailureClass::ArgsUnparseable,
        FormatFailureClass::UnknownTool,
        FormatFailureClass::MissingParam,
        FormatFailureClass::OutputTruncated,
    ];

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<FormatFailureClass> {
        FormatFailureClass::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
    }
}

/// `format_failure_scope ∈ {per_call, batch}` — `batch` renders one
/// failure for the whole response (mini's shape: the first malformed
/// call raises the format error for the batch — REQ-XE-111's
/// batch-scope declaration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatFailureScope {
    /// One rendering per malformed call.
    #[default]
    PerCall,
    /// One rendering per response — the first malformed call's class.
    Batch,
}

impl FormatFailureScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FormatFailureScope::PerCall => "per_call",
            FormatFailureScope::Batch => "batch",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<FormatFailureScope> {
        match s {
            "per_call" => Some(FormatFailureScope::PerCall),
            "batch" => Some(FormatFailureScope::Batch),
            _ => None,
        }
    }
}

/// `FormatErrorClasses` — the `error_format` classes record (REQ-XE-111):
/// `classes{class → Text}` renderings plus `format_failure_scope`.
/// Each leaf's `hh-tmpl/1` variables are checked ⊆ the class's declared
/// slots at compile.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatErrorClasses {
    /// `FormatFailureClass → Text` rendering (canonical order by class).
    pub renderings: BTreeMap<FormatFailureClass, Text>,
    /// The failure scope (`batch` ⇒ one render per response).
    pub scope: FormatFailureScope,
}

/// The E5 static check (C0 half — AC-R-2.5.2-7's schema face): every
/// rendering's `Text` content is pairwise distinct. `renderings` keyed by a
/// `SurfaceFailure` spelling outside the closed sum is an error (closed
/// worlds only).
pub fn check_distinguishable(spec: &ErrorFormatSpec) -> Distinguishability {
    let mut seen: Vec<&str> = Vec::new();
    for t in spec.renderings.values() {
        match &t.content {
            Some(c) => {
                if seen.contains(&c.as_str()) {
                    return Distinguishability::Failed;
                }
                seen.push(c.as_str());
            }
            // A hash-addressed leaf without inline content cannot be compared —
            // the verdict stays `unchecked` (honest, never guessed).
            None => return Distinguishability::Unchecked,
        }
    }
    Distinguishability::Verified
}

// ── surface_id ───────────────────────────────────────────────────────────────

/// `surface_id(binding)` — `idp("tool_surface", H(canonical surface record))`
/// over `{surface_name, hir_node_id, dialect, arg_map, exposure_mode,
/// capability_refs, admitted_modes, pinned, hidden}` — the members that make
/// the surface *this* surface. The `surface_id` member itself and the
/// evidence/safety refs are excluded (a record's id never covers the id
/// member; evidence attaches after minting). A rename changes the
/// `surface_id` and the profile hash only — `capability_refs` are semantic ids
/// (E7; AC-R-2.5.2-2).
pub fn surface_id(b: &crate::equiv::SurfaceBinding) -> String {
    let basis = Json::obj([
        ("surface_name", Json::str(b.surface_name.clone())),
        ("hir_node_id", Json::str(b.hir_node_id.clone())),
        ("dialect", Json::str(b.dialect.clone())),
        ("exposure_mode", Json::str(b.exposure_mode.as_str())),
        (
            "capability_refs",
            Json::Arr(
                b.capability_refs
                    .iter()
                    .map(|r| Json::str(r.clone()))
                    .collect(),
            ),
        ),
        (
            "admitted_modes",
            Json::Arr(
                b.admitted_modes
                    .iter()
                    .map(|m| Json::str(m.as_str()))
                    .collect(),
            ),
        ),
        ("pinned", Json::Bool(b.pinned)),
        ("hidden", Json::Bool(b.hidden)),
        ("arg_map", crate::schema::arg_map_json_pub(&b.arg_map)),
        // The `mapping` kind + the PlanMap's pin belong in the basis — a
        // PlanMap composite and an ArgMap surface of the same name/args are
        // different surfaces (CC1; ADR-0090 D6).
        ("mapping", Json::str(b.mapping.as_str())),
        (
            "plan_ref",
            match &b.mapping {
                BindingMapping::PlanMap(p) => Json::str(p.clone()),
                BindingMapping::SurfaceArgMap => Json::Null,
            },
        ),
    ]);
    idp_id(
        RecordKind::ToolSurface.domain_tag(),
        basis.to_canonical_string().as_bytes(),
    )
}
