//! The R-2.5.2⁰ surface records (§5d.2 §3; ADR-0090 D4/D5, ADR-0092 D1/D5/D6;
//! the ADR-0146 C0/Stage-1 schema slice): the extended `SurfaceBinding` members
//! ([`crate::equiv::SurfaceBinding`]), the `SurfaceFailure` closed sum, the
//! `ResultRenderSpec` (`full | truncate` at C0 — `concise`/`offload` are the C1
//! row) and the `ErrorFormatSpec`.
//!
//! Identity (CC1): `surface_id = idp("tool_surface", H(surface record))` — the
//! `hh_identity::RecordKind::ToolSurface` domain; a rename changes the
//! `surface_id` and the profile hash only, never a `capability_refs` member
//! (E7/AC-R-2.5.2-2).

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_identity::idp::idp_id;
use hh_identity::RecordKind;
use hh_wire::json::Json;

/// The compile-time `exposure_mode` sum of a `SurfaceBinding` (§5d.2 — how a
/// capability set is *rendered*: `primitive | split | composite | freeform |
/// code_mode | shim`; ADR-0090 D7). **Distinct** from the run-time
/// [`hh_hir::tools::ExposureMode`] (`direct | indexed | deferred | code_mode |
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
/// failure_class, raw_call_hash, model_call_id, rendering_ref}` (ADR-0092 D5;
/// ADR-0026 P2 log). Content by `raw_call_hash`, never the call (Rule C).
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

/// `ResultRenderSpec.mode` — the C0 members `{full, truncate{max_lines,
/// max_bytes, max_tokens, direction}}` (ADR-0146 stage note). `concise` and
/// `offload` are the C1/Stage-5 row — declared spellings are refused at C0
/// parse, never silently widened.
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
}

impl RenderMode {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Full => "full",
            RenderMode::Truncate { .. } => "truncate",
        }
    }
}

/// `ResultRenderSpec{mode, declared_loss: truncated{retained_fields[]}?,
/// validator_reads: [field]}` (§5d.2 §3; ADR-0092 D1) — the profile-owned
/// surface record written by `result_render` rules. E6 reads
/// `validator_reads ⊆ retained_fields` for every Validator bound to the
/// capability's results (the check itself is the C0/S3 row; the record lands
/// here).
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRenderSpec {
    /// The mode (`full | truncate` at C0).
    pub mode: RenderMode,
    /// The declared truncation loss — the `retained_fields` the truncated
    /// rendering keeps (mandatory when `mode = truncate` and fields are
    /// dropped; `None` on `full`).
    pub declared_loss: Option<Vec<String>>,
    /// The fields bound validators read — E6's static inclusion set.
    pub validator_reads: Vec<String>,
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
}

/// `ErrorFormatSpec{renderings: map<SurfaceFailure ∪ CapabilityFailureClass,
/// Text>, distinguishability}` (§5d.2 §3; ADR-0092 D6) — the `error_format`
/// rule's record. Renderings are `Text` leaves counted under `profile_prose`;
/// E5 ranges over both failure sets (the `SurfaceFailure` spellings plus the
/// capability `error_classes` — the map key domain is their union).
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorFormatSpec {
    /// `failure-class spelling → Text` rendering (canonical order by key).
    pub renderings: BTreeMap<String, Text>,
    /// The E5 distinguishability verdict.
    pub distinguishability: Distinguishability,
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
    ]);
    idp_id(
        RecordKind::ToolSurface.domain_tag(),
        basis.to_canonical_string().as_bytes(),
    )
}
