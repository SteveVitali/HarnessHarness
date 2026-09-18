//! The `OutputValidationPolicy` pipeline at G-INTERPRET (§5e.2; ADR-0108 D2):
//! `parse → surface ∈ compiled ModelSurface → schema conformance →
//! SurfaceArgMap → E1 preconditions.param_domain → truncation/refusal/empty
//! detection`. The **failure sum is closed** — `{unparseable,
//! unknown_surface, schema_violation, constraint_violation,
//! precondition_violation, truncated, refusal, empty}` — and a rejection is
//! form-level: `control.output.rejected` carries `detail_ref` (a
//! redaction-safe content address) and **never the rejected bytes** (the
//! refusal Observation echoes nothing).
//!
//! `mode: strict` (the C0 default) — every failure is a rejection counted
//! into `format_failures_running`; `repair_then_strict` is the declared C1
//! member (`ValidationMode::RepairThenStrict` — a `Strict` policy with a
//! repair configured fails `PolicyError`, and the C0 interpreter refuses the
//! mode at `arm`, never silently repairs). A profile's
//! `structured_output ∈ {none, json_mode, constrained}` changes the expected
//! rate, never whether the pipeline runs.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::policy::OutputValidationPolicy;

/// The closed failure sum (§5e.2 `OutputValidationPolicy` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputFailure {
    /// The tool-call arguments do not parse as JSON.
    Unparseable,
    /// The call names a surface outside the compiled `ModelSurface` set.
    UnknownSurface,
    /// The arguments violate the compiled surface schema.
    SchemaViolation,
    /// A declared constraint (enum/range) is violated.
    ConstraintViolation,
    /// E1 `preconditions.param_domain` rejects the call.
    PreconditionViolation,
    /// The response is truncated (`max_output`).
    Truncated,
    /// The model refused (`refusal`/`content_filter`).
    Refusal,
    /// The response carries neither text nor calls.
    Empty,
}

impl OutputFailure {
    /// The canonical spelling (the `failure_class` member of
    /// `control.output.rejected`).
    pub fn as_str(self) -> &'static str {
        match self {
            OutputFailure::Unparseable => "unparseable",
            OutputFailure::UnknownSurface => "unknown_surface",
            OutputFailure::SchemaViolation => "schema_violation",
            OutputFailure::ConstraintViolation => "constraint_violation",
            OutputFailure::PreconditionViolation => "precondition_violation",
            OutputFailure::Truncated => "truncated",
            OutputFailure::Refusal => "refusal",
            OutputFailure::Empty => "empty",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<OutputFailure> {
        Some(match s {
            "unparseable" => OutputFailure::Unparseable,
            "unknown_surface" => OutputFailure::UnknownSurface,
            "schema_violation" => OutputFailure::SchemaViolation,
            "constraint_violation" => OutputFailure::ConstraintViolation,
            "precondition_violation" => OutputFailure::PreconditionViolation,
            "truncated" => OutputFailure::Truncated,
            "refusal" => OutputFailure::Refusal,
            "empty" => OutputFailure::Empty,
            _ => return None,
        })
    }
}

/// A compiled-surface parameter spec — the Stage-1 schema shape the surface
/// compiler hands the pipeline (requiredness, a closed `kind`, optional
/// enum/range constraints, an optional `param_domain` precondition — E1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParamSpec {
    /// The param must be present.
    pub required: bool,
    /// The value kind (`string` | `int` | `bool` | `arr` | `obj` | `any`).
    pub kind: ParamKind,
    /// `enum` — the value must be a member, when non-empty.
    pub enum_values: Vec<Json>,
    /// `param_domain` — E1 precondition: the value must be a member, when
    /// non-empty (a domain declared on the surface, never `Text`).
    pub domain: Vec<Json>,
}

/// The closed parameter-kind vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamKind {
    /// Any JSON value.
    #[default]
    Any,
    /// `Json::Str`.
    Str,
    /// `Json::Int`.
    Int,
    /// `Json::Bool`.
    Bool,
    /// `Json::Arr`.
    Arr,
    /// `Json::Obj`.
    Obj,
}

/// A compiled surface — `surface_id`, its `semantic_id` (the `loop_key`
/// term), and the declared params (the Stage-1 `ModelSurface` projection —
/// the surface compiler's `PlanMap` row supplies it).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurfaceSpec {
    /// The surface id the model calls by name.
    pub surface_id: String,
    /// `capability.semantic_id` — the `loop_key` head (T-LCD-10).
    pub semantic_id: String,
    /// The declared params.
    pub params: BTreeMap<String, ParamSpec>,
}

/// A parsed tool call out of the gateway's response record — the driver
/// hands the pipeline this record (the raw arg bytes are already offloaded;
/// `args_raw` is the call's canonical-args candidate, never a `Text` leaf a
/// guard reads).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCall {
    /// The tool-call id.
    pub tool_call_id: String,
    /// The surface name the model emitted.
    pub surface: String,
    /// The raw args string (`""` = absent) — the `unparseable` check parses
    /// it; the parsed form is what schema conformance and `loop_key` read.
    pub args_raw: String,
}

/// A call that passed the pipeline — the `SurfaceArgMap` product plus the
/// `loop_key` the driver stamps on `action.tool.proposed`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedCall {
    /// The tool-call id.
    pub tool_call_id: String,
    /// The resolved surface id.
    pub surface_id: String,
    /// The canonical args (`SurfaceArgMap` — canonical-serialised data).
    pub args: Json,
    /// `H(semantic_id ∥ canonical(args))` — T-LCD-10.
    pub loop_key: String,
}

/// The pipeline product — either a rejection (one `OutputFailure`, the first
/// in pipeline order) or the validated calls in response order.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationVerdict {
    /// All calls validated.
    Pass {
        /// The validated calls.
        calls: Vec<ValidatedCall>,
    },
    /// A rejection — `control.output.rejected` records `failure_class` +
    /// `detail_ref`, never the bytes.
    Reject {
        /// The failure.
        failure: OutputFailure,
        /// The offending surface, when the failure is call-scoped.
        surface_id: Option<String>,
        /// The offending call id, when call-scoped.
        tool_call_id: Option<String>,
    },
}

/// `ValidationState` — the materialized view (`validation_state(run)`):
/// `format_failures_running` folded from `control.output.rejected` rows,
/// never a stored counter (ADR-0106 D5).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValidationState {
    /// Rejections so far (`format_failures_running` — `stop{format_failure}`
    /// trips at `max_format_failures`).
    pub format_failures_running: u32,
    /// `stop{format_failure}` has fired (idempotent — the state tracks the
    /// recorded `control.decision{stop{format_failure}}`).
    pub format_stop_fired: bool,
}

/// Fold `validation_state` from the durable prefix.
pub fn fold(events: &[hh_ledger::event::EventEnvelope]) -> ValidationState {
    let mut st = ValidationState::default();
    for ev in events {
        match ev.class.as_str() {
            "control.output.rejected" => {
                st.format_failures_running = ev
                    .payload
                    .get("format_failures_running")
                    .and_then(Json::as_int)
                    .map(|n| n as u32)
                    .unwrap_or(st.format_failures_running + 1);
            }
            "control.decision" => {
                if ev
                    .payload
                    .get("reason")
                    .and_then(|r| r.get("kind"))
                    .and_then(Json::as_str)
                    .map(|k| k == "format_failure")
                    .unwrap_or(false)
                {
                    st.format_stop_fired = true;
                }
            }
            _ => {}
        }
    }
    st
}

/// Run the strict pipeline over a response's parsed calls (a pure function —
/// G-INTERPRET evaluates it against the compiled surface set; `stop_reason`
/// is the gateway's member so `truncated`/`refusal`/`empty` precede the
/// per-call checks in pipeline order).
pub fn validate(
    policy: &OutputValidationPolicy,
    surfaces: &[SurfaceSpec],
    stop_reason: hh_gateway::vocab::StopReason,
    text_empty: bool,
    calls: &[ParsedCall],
) -> ValidationVerdict {
    use hh_gateway::vocab::StopReason as Gw;
    let _ = policy; // `strict` behaviour is unconditional at C0; the mode is
                    // the policy's record, the pipeline is fixed.
    match stop_reason {
        Gw::MaxOutput => {
            return ValidationVerdict::Reject {
                failure: OutputFailure::Truncated,
                surface_id: None,
                tool_call_id: None,
            }
        }
        Gw::Refusal | Gw::ContentFilter => {
            return ValidationVerdict::Reject {
                failure: OutputFailure::Refusal,
                surface_id: None,
                tool_call_id: None,
            }
        }
        _ => {}
    }
    if calls.is_empty() && text_empty {
        return ValidationVerdict::Reject {
            failure: OutputFailure::Empty,
            surface_id: None,
            tool_call_id: None,
        };
    }
    let mut out = vec![];
    for c in calls {
        // parse
        let args = if c.args_raw.is_empty() {
            Json::Obj(BTreeMap::new())
        } else {
            match hh_wire::json::parse(&c.args_raw) {
                Ok(j) => j,
                Err(_) => {
                    return ValidationVerdict::Reject {
                        failure: OutputFailure::Unparseable,
                        surface_id: Some(c.surface.clone()),
                        tool_call_id: Some(c.tool_call_id.clone()),
                    }
                }
            }
        };
        // surface ∈ compiled ModelSurface
        let spec = match surfaces.iter().find(|s| s.surface_id == c.surface) {
            Some(s) => s,
            None => {
                return ValidationVerdict::Reject {
                    failure: OutputFailure::UnknownSurface,
                    surface_id: Some(c.surface.clone()),
                    tool_call_id: Some(c.tool_call_id.clone()),
                }
            }
        };
        // schema conformance + constraints + param_domain (E1)
        if let Err((failure, _)) = check_args(&args, spec) {
            return ValidationVerdict::Reject {
                failure,
                surface_id: Some(spec.surface_id.clone()),
                tool_call_id: Some(c.tool_call_id.clone()),
            };
        }
        out.push(ValidatedCall {
            tool_call_id: c.tool_call_id.clone(),
            surface_id: spec.surface_id.clone(),
            loop_key: crate::loops::loop_key(&spec.semantic_id, &args),
            args,
        });
    }
    ValidationVerdict::Pass { calls: out }
}

/// Schema conformance against the compiled surface spec — required params
/// present (`schema_violation`), declared kinds hold (`schema_violation`),
/// enum/range constraints hold (`constraint_violation`), `param_domain` E1
/// precondition holds (`precondition_violation`). The failure order is the
/// pipeline order — deterministic.
fn check_args(args: &Json, spec: &SurfaceSpec) -> Result<(), (OutputFailure, String)> {
    let obj = match args {
        Json::Obj(m) => m,
        _ => return Err((OutputFailure::SchemaViolation, "args not an object".into())),
    };
    for (name, p) in &spec.params {
        match obj.get(name) {
            None => {
                if p.required {
                    return Err((
                        OutputFailure::SchemaViolation,
                        format!("missing required param {name}"),
                    ));
                }
            }
            Some(v) => {
                let kind_ok = match p.kind {
                    ParamKind::Any => true,
                    ParamKind::Str => matches!(v, Json::Str(_)),
                    ParamKind::Int => matches!(v, Json::Int(_)),
                    ParamKind::Bool => matches!(v, Json::Bool(_)),
                    ParamKind::Arr => matches!(v, Json::Arr(_)),
                    ParamKind::Obj => matches!(v, Json::Obj(_)),
                };
                if !kind_ok {
                    return Err((
                        OutputFailure::SchemaViolation,
                        format!("param {name} kind mismatch"),
                    ));
                }
                if !p.enum_values.is_empty() && !p.enum_values.contains(v) {
                    return Err((
                        OutputFailure::ConstraintViolation,
                        format!("param {name} outside enum"),
                    ));
                }
                if !p.domain.is_empty() && !p.domain.contains(v) {
                    return Err((
                        OutputFailure::PreconditionViolation,
                        format!("param {name} outside param_domain"),
                    ));
                }
            }
        }
    }
    // Undeclared params are a schema violation under `strict` (the compiled
    // surface schema is closed-world).
    for name in obj.keys() {
        if !spec.params.contains_key(name) {
            return Err((
                OutputFailure::SchemaViolation,
                format!("undeclared param {name}"),
            ));
        }
    }
    Ok(())
}

/// Whether `stop{format_failure{count}}` trips at this rejection
/// (`format_failures_running` reaching `max_format_failures` — the
/// `on_exhaustion` row).
pub fn exhaustion_trips(policy: &OutputValidationPolicy, running: u32) -> bool {
    running >= policy.max_format_failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_gateway::vocab::StopReason as Gw;

    fn spec() -> SurfaceSpec {
        SurfaceSpec {
            surface_id: "fs.read".into(),
            semantic_id: "sem/fs.read".into(),
            params: BTreeMap::from([(
                "path".into(),
                ParamSpec {
                    required: true,
                    kind: ParamKind::Str,
                    enum_values: vec![],
                    domain: vec![],
                },
            )]),
        }
    }

    #[test]
    fn strict_accepts_a_conforming_call_and_keys_it_semantically() {
        let v = validate(
            &OutputValidationPolicy::default(),
            &[spec()],
            Gw::ToolUse,
            true,
            &[ParsedCall {
                tool_call_id: "tc-1".into(),
                surface: "fs.read".into(),
                args_raw: r#"{"path":"/a"}"#.into(),
            }],
        );
        match v {
            ValidationVerdict::Pass { calls } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(
                    calls[0].loop_key,
                    crate::loops::loop_key("sem/fs.read", &calls[0].args)
                );
            }
            other => panic!("expected pass, got {other:?}"),
        }
    }

    #[test]
    fn the_failure_sum_is_the_closed_eight() {
        let p = OutputValidationPolicy::default();
        let cases = [
            // unparseable args
            (r#"{"path":"#, Gw::ToolUse, OutputFailure::Unparseable),
            // undeclared param → schema_violation
            (
                r#"{"path":"/a","x":1}"#,
                Gw::ToolUse,
                OutputFailure::SchemaViolation,
            ),
            // missing required → schema_violation
            (r#"{}"#, Gw::ToolUse, OutputFailure::SchemaViolation),
        ];
        for (raw, sr, want) in cases {
            let v = validate(
                &p,
                &[spec()],
                sr,
                true,
                &[ParsedCall {
                    tool_call_id: "t".into(),
                    surface: "fs.read".into(),
                    args_raw: raw.into(),
                }],
            );
            match v {
                ValidationVerdict::Reject { failure, .. } => assert_eq!(failure, want),
                other => panic!("expected reject {want:?}, got {other:?}"),
            }
        }
        // unknown_surface
        match validate(
            &p,
            &[spec()],
            Gw::ToolUse,
            true,
            &[ParsedCall {
                tool_call_id: "t".into(),
                surface: "fs.nuke".into(),
                args_raw: "{}".into(),
            }],
        ) {
            ValidationVerdict::Reject { failure, .. } => {
                assert_eq!(failure, OutputFailure::UnknownSurface)
            }
            other => panic!("{other:?}"),
        }
        // truncated / refusal / empty (pipeline order — response-level first)
        for (sr, want) in [
            (Gw::MaxOutput, OutputFailure::Truncated),
            (Gw::Refusal, OutputFailure::Refusal),
            (Gw::ContentFilter, OutputFailure::Refusal),
        ] {
            match validate(&p, &[spec()], sr, true, &[]) {
                ValidationVerdict::Reject { failure, .. } => assert_eq!(failure, want),
                other => panic!("{other:?}"),
            }
        }
        match validate(&p, &[spec()], Gw::EndTurn, true, &[]) {
            ValidationVerdict::Reject { failure, .. } => {
                assert_eq!(failure, OutputFailure::Empty)
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn enum_and_domain_membership_are_distinct_failures() {
        let mut s = spec();
        s.params.get_mut("path").unwrap().enum_values = vec![Json::str("/ok")];
        let v = validate(
            &OutputValidationPolicy::default(),
            &[s],
            Gw::ToolUse,
            true,
            &[ParsedCall {
                tool_call_id: "t".into(),
                surface: "fs.read".into(),
                args_raw: r#"{"path":"/no"}"#.into(),
            }],
        );
        match v {
            ValidationVerdict::Reject { failure, .. } => {
                assert_eq!(failure, OutputFailure::ConstraintViolation)
            }
            other => panic!("{other:?}"),
        }
        let mut s2 = spec();
        s2.params.get_mut("path").unwrap().domain = vec![Json::str("/ok")];
        let v2 = validate(
            &OutputValidationPolicy::default(),
            &[s2],
            Gw::ToolUse,
            true,
            &[ParsedCall {
                tool_call_id: "t".into(),
                surface: "fs.read".into(),
                args_raw: r#"{"path":"/no"}"#.into(),
            }],
        );
        match v2 {
            ValidationVerdict::Reject { failure, .. } => {
                assert_eq!(failure, OutputFailure::PreconditionViolation)
            }
            other => panic!("{other:?}"),
        }
    }
}
