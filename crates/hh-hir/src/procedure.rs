//! §5c.5 — the C0/Stage-2 procedure slice (R-2.4.5⁰; ADR-0084/0085/0086).
//!
//! This module owns the kernel-facing procedure machinery that is not already
//! the `Procedure` kind record: the closed `Predicate` sum and its check, the
//! `ProcedureProfile/1` ext record, the `validate_procedure` checks wired into
//! [`crate::validate::validate`], and `select_target` (the C0 target is
//! `instruction`; `workflow_node`/`subagent_task` are typed refusals, never
//! silent degradation).
//!
//! The render-purity boundary (I-RENDER): a `Text` leaf inside a procedure may
//! never carry a declared exec marker — the registered marker set is the
//! OpenHands-style bang-backtick form `` !` ``<cmd>`` ` ``. Detection scans
//! *inline* content only: a hash-addressed `Text` leaf (`content: None`) was
//! scanned when authored/lifted (`lift_skill` refuses the marker before the
//! leaf is ever addressed).

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

use crate::document::Node;
use crate::errors::HirError;
use crate::kinds::{Reversibility, World};
use crate::records::{ProcedureRecord, ProcedureStep, SurfaceRecord};
use crate::refs::Ref;

/// The registered `ProcedureProfile/1` ext key (§5c.5: `ext["<prefix>/procedure"]`
/// — `hh/` is the kernel-registered prefix). Closed schema; nothing in the
/// block decides authority, budget or validity (ADR-0015).
pub const PROCEDURE_PROFILE_EXT_KEY: &str = "hh/procedure";

/// The declared render-time exec marker set (ADR-0084 d6; the S2.8 ADR fixes
/// the C0 spelling): the bang-backtick open `` !` `` of the inline-exec form
/// `` !`cmd` ``. A `Text` leaf containing it executes at render time — forbidden.
pub fn carries_render_exec_marker(content: &str) -> bool {
    content.contains("!`")
}

// ─────────────────────────────────────────────────────────────────────────────
// Predicate — the closed precondition sum (§5c.5 kernel field semantics)
// ─────────────────────────────────────────────────────────────────────────────

/// `Predicate = path_glob | capability_present | artifact_present | env_requires
/// | validator_passed(ref, within) | parameter_bound | budget_remaining`
/// (§5c.5; ADR-0084 d2). Checkable by the kernel from records; every other
/// applicability condition lives in `ProcedureProfile.applicability_notes`
/// (a `Text` leaf, never evaluated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// `path_glob{pattern}` — satisfied when a run-visible path matches.
    PathGlob {
        /// The glob pattern (`*` one segment, `**` any depth, `?` one char).
        pattern: String,
    },
    /// `capability_present{capability}` — the capability's semantic id is in
    /// the run's capability set.
    CapabilityPresent {
        /// The capability's identity coordinate.
        capability: String,
    },
    /// `artifact_present{address}` — the content address resolves in the run's
    /// artifact index.
    ArtifactPresent {
        /// The content address.
        address: String,
    },
    /// `env_requires{key}` — the environment declares the key.
    EnvRequires {
        /// The environment key.
        key: String,
    },
    /// `validator_passed{validator, within}` — a fresh passing verdict on the
    /// validator exists (`within` = max age in ledger seq).
    ValidatorPassed {
        /// The validator ref.
        validator: Ref,
        /// Freshness bound (ledger seq).
        within: u64,
    },
    /// `parameter_bound{name}` — the parameter is bound at invocation.
    ParameterBound {
        /// The parameter name (a `ProcedureProfile.parameters[].name`).
        name: String,
    },
    /// `budget_remaining{dimension, min}` — the run's remaining budget for the
    /// dimension is ≥ `min`.
    BudgetRemaining {
        /// The budget dimension.
        dimension: String,
        /// The minimum remaining.
        min: u64,
    },
}

/// One parsed `preconditions` member: a checkable `Predicate` or a
/// `Ref<Validator>` (whose verdict the run evaluates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// A closed `Predicate`.
    Check(Predicate),
    /// A validator ref — `{validator: Ref}`.
    ValidatorRef(Ref),
}

impl Precondition {
    /// Parse one `preconditions` member. A member is `{kind: <predicate>, …}`
    /// or `{validator: <ref>}`; anything else is a `SchemaViolation` (other
    /// applicability conditions belong in `applicability_notes`, §5c.5).
    pub fn from_json(j: &Json, path: &str) -> Result<Precondition, HirError> {
        if let Some(v) = j.get("validator") {
            if j.get("kind").is_some() {
                return Err(HirError::SchemaViolation {
                    detail: format!("{path}: both kind and validator"),
                });
            }
            return Ok(Precondition::ValidatorRef(Ref::from_json(
                v,
                &format!("{path}.validator"),
            )?));
        }
        let kind =
            j.get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}: precondition needs kind or validator"),
                })?;
        let str_member = |k: &str| -> Result<String, HirError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}.{k} missing"),
                })
        };
        let p = match kind {
            "path_glob" => Predicate::PathGlob {
                pattern: str_member("pattern")?,
            },
            "capability_present" => Predicate::CapabilityPresent {
                capability: str_member("capability")?,
            },
            "artifact_present" => Predicate::ArtifactPresent {
                address: str_member("address")?,
            },
            "env_requires" => Predicate::EnvRequires {
                key: str_member("key")?,
            },
            "validator_passed" => Predicate::ValidatorPassed {
                validator: Ref::from_json(
                    j.get("ref").ok_or_else(|| HirError::SchemaViolation {
                        detail: format!("{path}.ref missing"),
                    })?,
                    &format!("{path}.ref"),
                )?,
                within: j.get("within").and_then(Json::as_int).ok_or_else(|| {
                    HirError::SchemaViolation {
                        detail: format!("{path}.within missing"),
                    }
                })? as u64,
            },
            "parameter_bound" => Predicate::ParameterBound {
                name: str_member("name")?,
            },
            "budget_remaining" => Predicate::BudgetRemaining {
                dimension: str_member("dimension")?,
                min: j.get("min").and_then(Json::as_int).ok_or_else(|| {
                    HirError::SchemaViolation {
                        detail: format!("{path}.min missing"),
                    }
                })? as u64,
            },
            other => {
                return Err(HirError::UnknownKind {
                    kind: format!("predicate {other}"),
                })
            }
        };
        Ok(Precondition::Check(p))
    }
}

/// Parse a `Procedure.preconditions` member (a `Json::Arr` of predicate /
/// validator-ref members).
pub fn parse_preconditions(j: &Json, path: &str) -> Result<Vec<Precondition>, HirError> {
    match j {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, m)| Precondition::from_json(m, &format!("{path}[{i}]")))
            .collect(),
        // `Null`/absent-shaped members carry no preconditions.
        Json::Null => Ok(Vec::new()),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}: preconditions must be an array"),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// check_preconditions — the run-side evaluation (§5c.5 row 7)
// ─────────────────────────────────────────────────────────────────────────────

/// The environment `check_preconditions` evaluates against: the run's records.
/// `None`-valued members mean the run cannot decide that fact class — the
/// predicate reports `unknown` (a `PreconditionUncheckable` note), never a
/// silent pass.
#[derive(Debug, Clone, Default)]
pub struct PreconditionEnv {
    /// The run-visible paths (for `path_glob`).
    pub paths: Option<BTreeSet<String>>,
    /// The in-scope capability semantic ids.
    pub capabilities: Option<BTreeSet<String>>,
    /// The run's artifact content addresses.
    pub artifacts: Option<BTreeSet<String>>,
    /// The declared environment keys.
    pub env_keys: Option<BTreeSet<String>>,
    /// The bound parameter names at this invocation.
    pub bound_parameters: BTreeSet<String>,
    /// `dimension → remaining` for `budget_remaining`.
    pub budget_remaining: BTreeMap<String, u64>,
    /// `validator semantic_id → (passed, verdict_seq)`.
    pub validator_verdicts: BTreeMap<String, (bool, u64)>,
    /// The evaluation point (ledger seq) for `within` freshness.
    pub at_seq: u64,
}

/// The `check_preconditions` report: `satisfied` iff no `violated`; `unknown`
/// entries are `PreconditionUncheckable` applicability notes — counted, never
/// errors (§5c.5 row 3; AC-R-2.4.5-11).
#[derive(Debug, Clone, Default)]
pub struct PreconditionReport {
    /// Every predicate that evaluated true (or a validator ref awaiting its
    /// run verdict — refs are checked by the verifier, not this table).
    pub satisfied: Vec<Predicate>,
    /// Checkable predicates that evaluated false — the procedure is omitted
    /// (`PreconditionFailed`) or lands the `precondition` omission reason.
    pub violated: Vec<Predicate>,
    /// Predicates the environment could not decide — `PreconditionUncheckable`
    /// notes, counted on the selection record.
    pub unknown: Vec<Predicate>,
    /// Validator refs — evaluated by the run's verification plane.
    pub validator_refs: Vec<Ref>,
}

/// The C0 segment-aware glob (`*` = one segment, `**` = any depth, `?` = one
/// char) — the same grammar `hh-context`'s retrieval index compiles.
pub fn path_glob_matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let tgt: Vec<&str> = path.split('/').collect();
    fn m(p: &[&str], t: &[&str]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(&"**"), _) => m(&p[1..], t) || (!t.is_empty() && m(p, &t[1..])),
            (Some(&seg), Some(&tseg)) => seg_match(seg, tseg) && m(&p[1..], &t[1..]),
            _ => false,
        }
    }
    fn seg_match(p: &str, t: &str) -> bool {
        let (pb, tb) = (p.as_bytes(), t.as_bytes());
        let (mut pi, mut ti) = (0usize, 0usize);
        while pi < pb.len() {
            match pb[pi] {
                b'*' => {
                    return (ti..=tb.len()).any(|k| seg_match(&p[pi + 1..], &t[k..]));
                }
                b'?' => {
                    if ti >= tb.len() {
                        return false;
                    }
                    pi += 1;
                    ti += 1;
                }
                c => {
                    if ti >= tb.len() || tb[ti] != c {
                        return false;
                    }
                    pi += 1;
                    ti += 1;
                }
            }
        }
        ti == tb.len()
    }
    m(&pat, &tgt)
}

/// `check_preconditions(preconditions, env)` — the kernel evaluation. Pure and
/// deterministic: no model call, no ledger read beyond the supplied records.
pub fn check_preconditions(pre: &[Precondition], env: &PreconditionEnv) -> PreconditionReport {
    let mut report = PreconditionReport::default();
    for p in pre {
        match p {
            Precondition::ValidatorRef(r) => report.validator_refs.push(r.clone()),
            Precondition::Check(pred) => match eval_predicate(pred, env) {
                Some(true) => report.satisfied.push(pred.clone()),
                Some(false) => report.violated.push(pred.clone()),
                None => report.unknown.push(pred.clone()),
            },
        }
    }
    report
}

/// `Some(bool)` = decided; `None` = uncheckable in this environment.
fn eval_predicate(p: &Predicate, env: &PreconditionEnv) -> Option<bool> {
    match p {
        Predicate::PathGlob { pattern } => env
            .paths
            .as_ref()
            .map(|paths| paths.iter().any(|t| path_glob_matches(pattern, t))),
        Predicate::CapabilityPresent { capability } => {
            env.capabilities.as_ref().map(|c| c.contains(capability))
        }
        Predicate::ArtifactPresent { address } => {
            env.artifacts.as_ref().map(|a| a.contains(address))
        }
        Predicate::EnvRequires { key } => env.env_keys.as_ref().map(|k| k.contains(key)),
        Predicate::ValidatorPassed { validator, within } => env
            .validator_verdicts
            .get(&validator.semantic_id)
            .map(|(passed, seq)| *passed && env.at_seq.saturating_sub(*seq) <= *within),
        Predicate::ParameterBound { name } => Some(env.bound_parameters.contains(name)),
        Predicate::BudgetRemaining { dimension, min } => env
            .budget_remaining
            .get(dimension)
            .map(|remaining| *remaining >= *min),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ProcedureProfile/1 — the registered ext block (§5c.5; ADR-0084 d1)
// ─────────────────────────────────────────────────────────────────────────────

/// `FailureClass = {precondition_failed, step_failed(step), validator_failed(ref),
/// budget_exhausted, permission_denied, effect_unknown, timeout, delegate_failed}`
/// (§5c.5 kernel field semantics). Closed sum.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FailureClass {
    /// A checkable precondition failed.
    PreconditionFailed,
    /// Step `i` failed (`*`/`any` covers every step).
    StepFailed(String),
    /// A `Verify` step's validator failed (`*`/`any` covers every validator).
    ValidatorFailed(String),
    /// The step/loop bound budget was exhausted.
    BudgetExhausted,
    /// The monitor denied the step's capability.
    PermissionDenied,
    /// The step produced an effect outside its declared class.
    EffectUnknown,
    /// A deadline fired.
    Timeout,
    /// A `Delegate` step's child failed.
    DelegateFailed,
}

impl FailureClass {
    /// Parse a `failure_classes[]` / `failure_handlers[].on` spelling.
    pub fn from_json(j: &Json, path: &str) -> Result<FailureClass, HirError> {
        match j {
            Json::Str(s) => match s.as_str() {
                "precondition_failed" => Ok(FailureClass::PreconditionFailed),
                "budget_exhausted" => Ok(FailureClass::BudgetExhausted),
                "permission_denied" => Ok(FailureClass::PermissionDenied),
                "effect_unknown" => Ok(FailureClass::EffectUnknown),
                "timeout" => Ok(FailureClass::Timeout),
                "delegate_failed" => Ok(FailureClass::DelegateFailed),
                other => Err(HirError::UnknownKind {
                    kind: format!("failure_class {other}"),
                }),
            },
            Json::Obj(_) => {
                if let Some(v) = j.get("step_failed") {
                    let which = v
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| v.as_int().map(|i| i.to_string()))
                        .ok_or_else(|| HirError::SchemaViolation {
                            detail: format!("{path}.step_failed: bad step designator"),
                        })?;
                    return Ok(FailureClass::StepFailed(which));
                }
                if let Some(v) = j.get("validator_failed") {
                    let which = v.as_str().map(str::to_string).ok_or_else(|| {
                        HirError::SchemaViolation {
                            detail: format!("{path}.validator_failed: bad validator designator"),
                        }
                    })?;
                    return Ok(FailureClass::ValidatorFailed(which));
                }
                Err(HirError::SchemaViolation {
                    detail: format!("{path}: bad failure_class member"),
                })
            }
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}: bad failure_class"),
            }),
        }
    }

    /// Whether a handler `on` class covers a required class.
    pub fn covers(&self, required: &FailureClass) -> bool {
        match (self, required) {
            (FailureClass::StepFailed(a), FailureClass::StepFailed(b)) => {
                a == "*" || a == "any" || a == b
            }
            (FailureClass::ValidatorFailed(a), FailureClass::ValidatorFailed(b)) => {
                a == "*" || a == "any" || a == b
            }
            (a, b) => a == b,
        }
    }
}

/// `Handler = retry(bound: Ref<Budget>) | fallback(Ref<Procedure>) |
/// compensate(Ref<Procedure>) | escalate | abort` (§5c.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handler {
    /// `retry{bound}` — re-attempt bounded by a `Ref<Budget>`.
    Retry(Ref),
    /// `fallback(procedure)` — switch to a declared procedure.
    Fallback(Ref),
    /// `compensate(procedure)` — run the compensating procedure.
    Compensate(Ref),
    /// `escalate` — hand the failure to the principal.
    Escalate,
    /// `abort` — stop the procedure.
    Abort,
}

impl Handler {
    /// Parse a `failure_handlers[].then` member.
    pub fn from_json(j: &Json, path: &str) -> Result<Handler, HirError> {
        match j {
            Json::Str(s) => match s.as_str() {
                "escalate" => Ok(Handler::Escalate),
                "abort" => Ok(Handler::Abort),
                other => Err(HirError::UnknownKind {
                    kind: format!("handler {other}"),
                }),
            },
            Json::Obj(_) => {
                for (k, mk) in [
                    ("retry", "bound"),
                    ("fallback", "procedure"),
                    ("compensate", "procedure"),
                ] {
                    if let Some(v) = j.get(k) {
                        let r = Ref::from_json(v, &format!("{path}.{k}"))?;
                        let _ = mk;
                        return Ok(match k {
                            "retry" => Handler::Retry(r),
                            "fallback" => Handler::Fallback(r),
                            _ => Handler::Compensate(r),
                        });
                    }
                }
                Err(HirError::SchemaViolation {
                    detail: format!("{path}: bad handler member"),
                })
            }
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}: bad handler"),
            }),
        }
    }
}

/// One `failure_handlers[]` row: `{on: FailureClass, then: Handler}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureHandler {
    /// The covered class.
    pub on: FailureClass,
    /// The action.
    pub then: Handler,
}

/// `ProcedureProfile/1` — the parsed `ext["hh/procedure"]` block (§5c.5;
/// ADR-0084 d1). Closed schema: `index{description, retrieval_hints[],
/// examples[], tags[]}`, `parameters[{name, type, required, description,
/// default?}]`, `outputs?{schema_ref}`, `applicability_notes[]`,
/// `invocation_policy{model, principal, implicit}`, `failure_classes[]`,
/// `target_override?{compile_hint, reason}`, `composition[{callee, binding,
/// inline_at_resolve}]`, `relations[{kind, to}]`, `source{extension_ref?,
/// tree_path?}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcedureProfile {
    /// `parameters[].name` — the declared parameter set `UnboundParameter`
    /// checks `$param:` references against.
    pub parameters: BTreeMap<String, ProcedureParameter>,
    /// The declared `failure_classes[]` the handlers must cover.
    pub failure_classes: Vec<FailureClass>,
    /// `target_override.compile_hint` when present.
    pub target_override: Option<crate::records::CompileHint>,
    /// `composition[].callee` — the static composition graph (the
    /// `CompositionCycle` check's edge set).
    pub composition: Vec<Ref>,
    /// `index.tags` — the retrieval-hint tags the procedure index carries.
    pub index_tags: Vec<String>,
    /// `index.retrieval_hints` — the trigger/discovery hints (a `path_glob`
    /// hint is the `trigger.path_touched` match key).
    pub index_retrieval_hints: Vec<String>,
    /// `applicability_notes` — counted (`Text` leaves, never evaluated).
    pub applicability_notes: usize,
    /// `invocation_policy{model, principal, implicit}` (default all true).
    pub invocation_policy: (bool, bool, bool),
    /// `source{extension_ref?, tree_path?}` — the lifted-skill back-reference.
    pub source_extension_ref: Option<String>,
    /// `source.tree_path`.
    pub source_tree_path: Option<String>,
}

/// A declared `ProcedureProfile.parameters[]` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureParameter {
    /// `type` — a `TypedValueKind` spelling (carried, not interpreted at C0).
    pub ty: String,
    /// `required`.
    pub required: bool,
    /// `default` present.
    pub has_default: bool,
}

impl ProcedureProfile {
    /// Parse the `ext["hh/procedure"]` block. `Ok(None)` when absent; closed
    /// schema — unknown members are `SchemaViolation` (the block's schema is
    /// registered with the ext grammar; extension by dialect bump).
    pub fn from_node(node: &Node) -> Result<Option<ProcedureProfile>, HirError> {
        let Some(j) = node.ext.get(PROCEDURE_PROFILE_EXT_KEY) else {
            return Ok(None);
        };
        const MEMBERS: &[&str] = &[
            "index",
            "parameters",
            "outputs",
            "applicability_notes",
            "invocation_policy",
            "failure_classes",
            "target_override",
            "composition",
            "relations",
            "source",
        ];
        if let Json::Obj(m) = j {
            for k in m.keys() {
                if !MEMBERS.contains(&k.as_str()) {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{PROCEDURE_PROFILE_EXT_KEY}.{k}: unknown member"),
                    });
                }
            }
        } else {
            return Err(HirError::SchemaViolation {
                detail: format!("{PROCEDURE_PROFILE_EXT_KEY}: must be an object"),
            });
        }
        let mut p = ProcedureProfile {
            invocation_policy: (true, true, true),
            ..ProcedureProfile::default()
        };
        if let Some(idx) = j.get("index") {
            for (k, dst) in [
                ("tags", &mut p.index_tags),
                ("retrieval_hints", &mut p.index_retrieval_hints),
            ] {
                if let Some(Json::Arr(items)) = idx.get(k) {
                    for it in items {
                        if let Some(s) = it.as_str() {
                            dst.push(s.to_string());
                        } else if let Some(c) = it.get("content").and_then(Json::as_str) {
                            // A Text leaf form `{content, …}`.
                            dst.push(c.to_string());
                        }
                    }
                }
            }
        }
        if let Some(Json::Arr(params)) = j.get("parameters") {
            for (i, prm) in params.iter().enumerate() {
                let path = format!("{PROCEDURE_PROFILE_EXT_KEY}.parameters[{i}]");
                let name = prm
                    .get("name")
                    .and_then(Json::as_str)
                    .ok_or_else(|| HirError::SchemaViolation {
                        detail: format!("{path}.name missing"),
                    })?
                    .to_string();
                p.parameters.insert(
                    name,
                    ProcedureParameter {
                        ty: prm
                            .get("type")
                            .and_then(Json::as_str)
                            .unwrap_or("text")
                            .to_string(),
                        required: matches!(prm.get("required"), Some(Json::Bool(true))),
                        has_default: prm.get("default").is_some(),
                    },
                );
            }
        }
        if let Some(Json::Arr(notes)) = j.get("applicability_notes") {
            p.applicability_notes = notes.len();
        }
        if let Some(Json::Arr(classes)) = j.get("failure_classes") {
            for (i, c) in classes.iter().enumerate() {
                p.failure_classes.push(FailureClass::from_json(
                    c,
                    &format!("{PROCEDURE_PROFILE_EXT_KEY}.failure_classes[{i}]"),
                )?);
            }
        }
        if let Some(ovr) = j.get("target_override") {
            let hint = ovr
                .get("compile_hint")
                .and_then(Json::as_str)
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!(
                        "{PROCEDURE_PROFILE_EXT_KEY}.target_override.compile_hint missing"
                    ),
                })?;
            p.target_override = Some(match hint {
                "instruction" => crate::records::CompileHint::Instruction,
                "workflow_node" => crate::records::CompileHint::WorkflowNode,
                "subagent_task" => crate::records::CompileHint::SubagentTask,
                other => {
                    return Err(HirError::UnknownKind {
                        kind: format!("compile_hint {other}"),
                    })
                }
            });
        }
        if let Some(Json::Arr(comp)) = j.get("composition") {
            for (i, c) in comp.iter().enumerate() {
                let callee = c.get("callee").ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{PROCEDURE_PROFILE_EXT_KEY}.composition[{i}].callee missing"),
                })?;
                p.composition.push(Ref::from_json(
                    callee,
                    &format!("{PROCEDURE_PROFILE_EXT_KEY}.composition[{i}].callee"),
                )?);
            }
        }
        if let Some(src) = j.get("source") {
            p.source_extension_ref = src
                .get("extension_ref")
                .and_then(Json::as_str)
                .map(str::to_string);
            p.source_tree_path = src
                .get("tree_path")
                .and_then(Json::as_str)
                .map(str::to_string);
        }
        if let Some(ip) = j.get("invocation_policy") {
            let b = |k: &str| {
                ip.get(k)
                    .and_then(|v| {
                        if let Json::Bool(b) = v {
                            Some(*b)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(true)
            };
            p.invocation_policy = (b("model"), b("principal"), b("implicit"));
        }
        Ok(Some(p))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// validate_procedure — inside `validate` (§5c.5 row 3)
// ─────────────────────────────────────────────────────────────────────────────

/// The run-side half of `validate_procedure` (§5c.5 row 3): called from
/// [`crate::validate::validate`] for each `Procedure` node. `uncheckable`
/// accumulates `PreconditionUncheckable` counts (validator-bound preconditions
/// — decided by the verification plane, not statically).
pub(crate) fn validate_procedure(
    node: &Node,
    p: &ProcedureRecord,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
    uncheckable: &mut u64,
) {
    let proc_id = node.semantic_id();

    // Render purity — a `Text` leaf carrying a declared exec marker is
    // `RenderTimeExecution` (I-RENDER; ADR-0084 d6).
    check_render_purity(&p.steps, &proc_id, errs);

    // `preconditions` — closed `Predicate` kinds or a validator ref;
    // `validator_passed`/validator refs are `PreconditionUncheckable` at
    // validate time (the verification plane owns their verdicts) — counted.
    match parse_preconditions(&p.preconditions, "procedure.preconditions") {
        Ok(pre) => {
            for c in &pre {
                match c {
                    Precondition::ValidatorRef(r) => {
                        *uncheckable += 1;
                        check_ref_kind(
                            index,
                            r,
                            &[crate::kinds::EntityKind::Validator],
                            "preconditions[].validator",
                            errs,
                        );
                    }
                    Precondition::Check(Predicate::ValidatorPassed { validator, .. }) => {
                        *uncheckable += 1;
                        check_ref_kind(
                            index,
                            validator,
                            &[crate::kinds::EntityKind::Validator],
                            "preconditions[].ref",
                            errs,
                        );
                    }
                    Precondition::Check(_) => {}
                }
            }
        }
        Err(e) => errs.push(e),
    }

    let profile = match ProcedureProfile::from_node(node) {
        Ok(p) => p,
        Err(e) => {
            errs.push(e);
            None
        }
    };

    // Every `Invoke.capability` ∈ `allowed_capabilities` — the allowlist is
    // the closed set an `Invoke` may reference (§5c.5).
    let mut invokes = Vec::new();
    collect_invokes(&p.steps, &mut invokes);
    let allowed: BTreeSet<&str> = p
        .allowed_capabilities
        .iter()
        .map(|r| r.semantic_id.as_str())
        .collect();
    for tool in &invokes {
        if !allowed.contains(tool.semantic_id.as_str()) {
            errs.push(HirError::CapabilityNotAllowed {
                detail: format!(
                    "procedure {proc_id}: Invoke {} not in allowed_capabilities",
                    tool.semantic_id
                ),
            });
        }
    }

    // `Loop` bounded — the `bound` ref must resolve to a `Budget`.
    check_loop_bounds(&p.steps, index, &proc_id, errs);

    // `parameters referenced are declared` — `$param:<name>` spellings in the
    // steps' structured members must name a `ProcedureProfile.parameters[]` row.
    let mut params = Vec::new();
    collect_param_refs(&p.steps, &mut params);
    for name in params {
        let declared = profile
            .as_ref()
            .is_some_and(|pr| pr.parameters.contains_key(&name));
        if !declared {
            errs.push(HirError::UnboundParameter {
                detail: format!("procedure {proc_id}: $param:{name} not declared"),
            });
        }
    }

    // `handlers total over failure_classes` — the required set is the profile's
    // declared classes ∪ the classes the steps can raise.
    let required = required_failure_classes(p, profile.as_ref());
    let handlers = parse_handlers(&p.failure_handlers, errs);
    for need in &required {
        if !handlers.iter().any(|h| h.on.covers(need)) {
            errs.push(HirError::HandlerIncomplete {
                detail: format!(
                    "procedure {proc_id}: failure class {} has no handler",
                    failure_class_name(need)
                ),
            });
        }
    }
    // Handler targets resolve: retry.bound → Budget; fallback/compensate → Procedure.
    for h in &handlers {
        match &h.then {
            Handler::Retry(r) => check_ref_kind(
                index,
                r,
                &[crate::kinds::EntityKind::Budget],
                "failure_handlers[].then.retry",
                errs,
            ),
            Handler::Fallback(r) | Handler::Compensate(r) => check_ref_kind(
                index,
                r,
                &[crate::kinds::EntityKind::Procedure],
                "failure_handlers[].then",
                errs,
            ),
            _ => {}
        }
    }

    // `composition` acyclic — the profile's callee graph over Procedure refs.
    if let Some(pr) = &profile {
        for callee in &pr.composition {
            check_ref_kind(
                index,
                callee,
                &[crate::kinds::EntityKind::Procedure],
                "composition[].callee",
                errs,
            );
        }
        if composition_cycle(&proc_id, index) {
            errs.push(HirError::CompositionCycle {
                detail: format!("procedure {proc_id}: composition cycle"),
            });
        }
    }

    // The risk floor — `ProcedureUnverifiable` (§5c.5): a derived effect that is
    // `irreversible` or `world = open` (external scope) requires
    // `expected_evidence ≠ ∅`.
    if expected_evidence_empty(&p.expected_evidence) {
        let mut effects = Vec::new();
        derived_effects(&p.steps, index, &mut effects);
        for e in &effects {
            let risky = e.attributes.as_ref().is_some_and(|a| {
                a.reversibility == Reversibility::Irreversible || a.world == World::Open
            });
            if risky {
                errs.push(HirError::ProcedureUnverifiable {
                    detail: format!(
                        "procedure {proc_id}: effect {} is irreversible/open-scope with no expected_evidence",
                        e.domain.name()
                    ),
                });
            }
        }
    }
}

fn failure_class_name(c: &FailureClass) -> String {
    match c {
        FailureClass::PreconditionFailed => "precondition_failed".into(),
        FailureClass::StepFailed(s) => format!("step_failed:{s}"),
        FailureClass::ValidatorFailed(v) => format!("validator_failed:{v}"),
        FailureClass::BudgetExhausted => "budget_exhausted".into(),
        FailureClass::PermissionDenied => "permission_denied".into(),
        FailureClass::EffectUnknown => "effect_unknown".into(),
        FailureClass::Timeout => "timeout".into(),
        FailureClass::DelegateFailed => "delegate_failed".into(),
    }
}

/// The classes a procedure can raise: the profile's declared `failure_classes`
/// ∪ step-derived (`step_failed` for executable steps, `validator_failed` per
/// `Verify`, `delegate_failed` per `Delegate`, `precondition_failed` when
/// preconditions exist, `budget_exhausted` when a `Loop` binds a budget).
fn required_failure_classes(
    p: &ProcedureRecord,
    profile: Option<&ProcedureProfile>,
) -> BTreeSet<FailureClass> {
    let mut set: BTreeSet<FailureClass> = profile
        .map(|pr| pr.failure_classes.iter().cloned().collect())
        .unwrap_or_default();
    let mut steps_executable = false;
    derive_step_classes(&p.steps, &mut set, &mut steps_executable);
    if steps_executable {
        set.insert(FailureClass::StepFailed("any".into()));
    }
    if !matches!(p.preconditions, Json::Null) && p.preconditions != Json::Arr(vec![]) {
        set.insert(FailureClass::PreconditionFailed);
    }
    set
}

fn derive_step_classes(
    steps: &[ProcedureStep],
    set: &mut BTreeSet<FailureClass>,
    executable: &mut bool,
) {
    for s in steps {
        match s {
            ProcedureStep::Instruction(_) => {}
            ProcedureStep::Invoke { .. } => {
                *executable = true;
                set.insert(FailureClass::PermissionDenied);
            }
            ProcedureStep::Delegate { .. } => {
                *executable = true;
                set.insert(FailureClass::DelegateFailed);
                set.insert(FailureClass::PermissionDenied);
            }
            ProcedureStep::Verify { validator } => {
                *executable = true;
                set.insert(FailureClass::ValidatorFailed(validator.semantic_id.clone()));
            }
            ProcedureStep::Opaque(_) => *executable = true,
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                derive_step_classes(then_body, set, executable);
                derive_step_classes(else_body, set, executable);
            }
            ProcedureStep::Loop { body, .. } => {
                set.insert(FailureClass::BudgetExhausted);
                derive_step_classes(body, set, executable);
            }
        }
    }
}

fn parse_handlers(j: &Json, errs: &mut Vec<HirError>) -> Vec<FailureHandler> {
    let items = match j {
        Json::Arr(items) => items.clone(),
        Json::Null => Vec::new(),
        other => {
            if let Json::Obj(m) = other {
                if let Some(Json::Arr(items)) = m.get("handlers") {
                    items.clone()
                } else {
                    errs.push(HirError::SchemaViolation {
                        detail: "failure_handlers: expected an array".into(),
                    });
                    Vec::new()
                }
            } else {
                errs.push(HirError::SchemaViolation {
                    detail: "failure_handlers: expected an array".into(),
                });
                Vec::new()
            }
        }
    };
    let mut out = Vec::new();
    for (i, h) in items.iter().enumerate() {
        let path = format!("failure_handlers[{i}]");
        let on = match h.get("on") {
            Some(v) => match FailureClass::from_json(v, &format!("{path}.on")) {
                Ok(c) => c,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            },
            None => {
                errs.push(HirError::SchemaViolation {
                    detail: format!("{path}.on missing"),
                });
                continue;
            }
        };
        let then = match h.get("then") {
            Some(v) => match Handler::from_json(v, &format!("{path}.then")) {
                Ok(t) => t,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            },
            None => {
                errs.push(HirError::SchemaViolation {
                    detail: format!("{path}.then missing"),
                });
                continue;
            }
        };
        out.push(FailureHandler { on, then });
    }
    out
}

fn expected_evidence_empty(j: &Json) -> bool {
    match j {
        Json::Null => true,
        Json::Arr(a) => a.is_empty(),
        Json::Obj(m) => {
            m.is_empty()
                || m.get("items")
                    .is_some_and(|i| matches!(i, Json::Arr(a) if a.is_empty()))
        }
        _ => false,
    }
}

fn check_render_purity(steps: &[ProcedureStep], proc_id: &str, errs: &mut Vec<HirError>) {
    for s in steps {
        match s {
            ProcedureStep::Instruction(t) => {
                if let Some(c) = &t.content {
                    if carries_render_exec_marker(c) {
                        errs.push(HirError::RenderTimeExecution {
                            detail: format!(
                                "procedure {proc_id}: Instruction step carries a render-time exec marker"
                            ),
                        });
                    }
                }
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                check_render_purity(then_body, proc_id, errs);
                check_render_purity(else_body, proc_id, errs);
            }
            ProcedureStep::Loop { body, .. } => check_render_purity(body, proc_id, errs),
            _ => {}
        }
    }
}

fn collect_invokes<'a>(steps: &'a [ProcedureStep], out: &mut Vec<&'a Ref>) {
    for s in steps {
        match s {
            ProcedureStep::Invoke { tool, .. } => out.push(tool),
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_invokes(then_body, out);
                collect_invokes(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_invokes(body, out),
            _ => {}
        }
    }
}

fn check_loop_bounds(
    steps: &[ProcedureStep],
    index: &BTreeMap<String, &Node>,
    proc_id: &str,
    errs: &mut Vec<HirError>,
) {
    for s in steps {
        match s {
            ProcedureStep::Loop { bound, body } => {
                check_ref_kind(
                    index,
                    bound,
                    &[crate::kinds::EntityKind::Budget],
                    "steps[].bound",
                    errs,
                );
                let _ = proc_id;
                check_loop_bounds(body, index, proc_id, errs);
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                check_loop_bounds(then_body, index, proc_id, errs);
                check_loop_bounds(else_body, index, proc_id, errs);
            }
            _ => {}
        }
    }
}

/// `$param:<name>` references inside the steps' structured members
/// (`Invoke.args`, `Branch.condition`, `Delegate.spec`).
/// The `$param:` refs in one Json value (args/condition/spec).
fn collect_param_refs_json(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Str(s) => {
            if let Some(name) = s.strip_prefix("$param:") {
                out.push(name.to_string());
            }
        }
        Json::Arr(items) => {
            for i in items {
                collect_param_refs_json(i, out);
            }
        }
        Json::Obj(m) => {
            for v in m.values() {
                collect_param_refs_json(v, out);
            }
        }
        _ => {}
    }
}

fn collect_param_refs(steps: &[ProcedureStep], out: &mut Vec<String>) {
    fn walk(j: &Json, out: &mut Vec<String>) {
        match j {
            Json::Str(s) => {
                if let Some(name) = s.strip_prefix("$param:") {
                    out.push(name.to_string());
                }
            }
            Json::Arr(items) => {
                for i in items {
                    walk(i, out);
                }
            }
            Json::Obj(m) => {
                for v in m.values() {
                    walk(v, out);
                }
            }
            _ => {}
        }
    }
    for s in steps {
        match s {
            ProcedureStep::Invoke { args, .. } => walk(args, out),
            ProcedureStep::Branch {
                condition,
                then_body,
                else_body,
            } => {
                walk(condition, out);
                collect_param_refs(then_body, out);
                collect_param_refs(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_param_refs(body, out),
            ProcedureStep::Delegate { spec, .. } => walk(spec, out),
            _ => {}
        }
    }
}

fn check_ref_kind(
    index: &BTreeMap<String, &Node>,
    r: &Ref,
    kinds: &[crate::kinds::EntityKind],
    path: &str,
    errs: &mut Vec<HirError>,
) {
    match index.get(r.semantic_id.as_str()) {
        None => errs.push(HirError::UnresolvedRef {
            detail: format!("{path}: {} unresolved", r.semantic_id),
        }),
        Some(n) if !kinds.contains(&n.kind) => errs.push(HirError::UnresolvedRef {
            detail: format!(
                "{path}: {} is a {}, expected {}",
                r.semantic_id,
                n.kind.name(),
                kinds.iter().map(|k| k.name()).collect::<Vec<_>>().join("|")
            ),
        }),
        _ => {}
    }
}

/// `composition` acyclic — DFS over the callee graph (refs resolve by
/// `semantic_id`; only in-document edges are walked).
fn composition_cycle(root: &str, index: &BTreeMap<String, &Node>) -> bool {
    fn visit(
        sid: &str,
        index: &BTreeMap<String, &Node>,
        stack: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> bool {
        if stack.contains(sid) {
            return true;
        }
        if done.contains(sid) {
            return false;
        }
        stack.insert(sid.to_string());
        if let Some(n) = index.get(sid) {
            if let Ok(Some(profile)) = ProcedureProfile::from_node(n) {
                for callee in &profile.composition {
                    if visit(&callee.semantic_id, index, stack, done) {
                        return true;
                    }
                }
            }
        }
        stack.remove(sid);
        done.insert(sid.to_string());
        false
    }
    visit(root, index, &mut BTreeSet::new(), &mut BTreeSet::new())
}

/// The derived-effects walk lives in `crate::validate` (V-EFF) — this module
/// needs the same walk for the risk floor.
fn derived_effects(
    steps: &[ProcedureStep],
    index: &BTreeMap<String, &Node>,
    out: &mut Vec<crate::kinds::EffectClass>,
) {
    crate::validate::derived_effects(steps, index, out)
}

// ─────────────────────────────────────────────────────────────────────────────
// select_target — the full C1 rule (§5c.5 row 8; AC-R-2.4.5-4/-5)
// ─────────────────────────────────────────────────────────────────────────────

/// `CompilationTarget ∈ {instruction, workflow_node, subagent_task}` — the
/// closed target sum (§5c.5). `subagent_task` is declared for the decision's
/// honesty (`I(P)` may hold while the target itself is refused
/// `DelegationUnavailable` before Stage 4 — the spec's Stage-3 degradation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationTarget {
    /// `instruction` — the procedure body is compiled to instruction text.
    Instruction,
    /// `workflow_node` — the body lowers onto §3.2.4's plan-node kinds.
    WorkflowNode,
    /// `subagent_task` — declared; refused `DelegationUnavailable` < Stage 4.
    SubagentTask,
}

impl CompilationTarget {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CompilationTarget::Instruction => "instruction",
            CompilationTarget::WorkflowNode => "workflow_node",
            CompilationTarget::SubagentTask => "subagent_task",
        }
    }
}

/// `B(P)`'s failure reasons — `UnexpressibleAsWorkflow`'s closed `reason`
/// member (§5c.5 row 8: `{unbound_arg, uncheckable_branch, unbounded_loop,
/// opaque_without_interface}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowReason {
    /// An `Invoke` arg references `$param:` the profile never declares.
    UnboundArg,
    /// A `Branch` condition no validator can check.
    UncheckableBranch,
    /// A `Loop` whose bound does not resolve to a `Budget` node.
    UnboundedLoop,
    /// An `Opaque` step without a declared interface.
    OpaqueWithoutInterface,
}

impl WorkflowReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowReason::UnboundArg => "unbound_arg",
            WorkflowReason::UncheckableBranch => "uncheckable_branch",
            WorkflowReason::UnboundedLoop => "unbounded_loop",
            WorkflowReason::OpaqueWithoutInterface => "opaque_without_interface",
        }
    }
}

/// The `select_target` refusals — typed, never silent (§5c.5 row 8:
/// `TargetInfeasible`, `UnexpressibleAsWorkflow{step, reason}`,
/// `DelegationUnavailable`, `ProcedureUnverifiable`,
/// `UnexpressibleSurface`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// `UnexpressibleAsWorkflow{step, reason}` — `B(P)` failed at `step`.
    UnexpressibleAsWorkflow {
        /// The procedure's semantic id.
        procedure: String,
        /// The step that broke compilability.
        step: String,
        /// The closed reason.
        reason: WorkflowReason,
    },
    /// `DelegationUnavailable` — `subagent_task` is refused before Stage 4
    /// (the honest refusal the degraded rule returns when `I(P)` holds and
    /// the subagent path is bound).
    DelegationUnavailable {
        /// The procedure's semantic id.
        procedure: String,
    },
    /// No target satisfies the procedure (`TargetInfeasible`) — an
    /// infeasible `target_override`, or every target refused.
    TargetInfeasible {
        /// Why.
        detail: String,
    },
    /// `UnexpressibleSurface` — `size(body) > procedure_inline_budget` with
    /// no subagent path (rule iv).
    UnexpressibleSurface {
        /// The procedure's semantic id.
        procedure: String,
        /// The body size.
        size: u64,
        /// The budget.
        budget: u64,
    },
    /// `ProcedureUnverifiable` — `R(P)` contains `irreversible` or
    /// `scope = external` and `expected_evidence` is empty, or a risky
    /// `Invoke` fails Π at the proposer's effective authority (rule v).
    ProcedureUnverifiable {
        /// The procedure's semantic id.
        procedure: String,
        /// Why.
        detail: String,
    },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::UnexpressibleAsWorkflow {
                procedure,
                step,
                reason,
            } => write!(
                f,
                "UnexpressibleAsWorkflow: {procedure} step {step} ({})",
                reason.as_str()
            ),
            SelectError::DelegationUnavailable { procedure } => {
                write!(f, "DelegationUnavailable: {procedure}")
            }
            SelectError::TargetInfeasible { detail } => {
                write!(f, "TargetInfeasible: {detail}")
            }
            SelectError::UnexpressibleSurface {
                procedure,
                size,
                budget,
            } => write!(f, "UnexpressibleSurface: {procedure} ({size} > {budget})"),
            SelectError::ProcedureUnverifiable { procedure, detail } => {
                write!(f, "ProcedureUnverifiable: {procedure} ({detail})")
            }
        }
    }
}
impl std::error::Error for SelectError {}

/// The `def`/`profile` inputs `select_target` reads (§5c.5 row 8 — "the
/// bound control strategy declares `workflow_execution`", "the definition
/// binds subagents", "the profile declares `subagents`",
/// `procedure_inline_budget`, the proposer's effective authority for Π).
#[derive(Debug, Clone)]
pub struct SelectCtx {
    /// The bound control strategy declares `workflow_execution` (§05e).
    pub workflow_execution_declared: bool,
    /// The definition binds subagents (§05e binds them).
    pub subagents_bound: bool,
    /// The profile declares `subagents`.
    pub profile_declares_subagents: bool,
    /// `procedure_inline_budget` — the instruction target's size ceiling.
    pub procedure_inline_budget: u64,
    /// The capability refs the proposer's effective Π admits (rule v's
    /// Π-pass — an `Invoke` on a risky tool must name a granted ref).
    pub granted_capabilities: std::collections::BTreeSet<String>,
}

impl Default for SelectCtx {
    /// The C0/Stage-2 defaults — no workflow execution, no subagents, a
    /// generous inline budget (the C0 target set degenerates to
    /// `{instruction}` with typed refusals for the rest).
    fn default() -> Self {
        SelectCtx {
            workflow_execution_declared: false,
            subagents_bound: false,
            profile_declares_subagents: false,
            procedure_inline_budget: 64 * 1024,
            granted_capabilities: std::collections::BTreeSet::new(),
        }
    }
}

/// The decision record — `{target, predicates{B, R, I}, rule_id?}`
/// (§5c.5 row 8; the `rule_id` names which rule clause produced the target —
/// `override`, `b_workflow`, `i_subagent`, `default_instruction`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDecision {
    /// The selected target.
    pub target: CompilationTarget,
    /// `B(P)` — compilable to `workflow_node`.
    pub b: bool,
    /// `R(P)` — the derived risk set (`irreversible`, `scope:external` …).
    pub r: Vec<String>,
    /// `I(P)` — the isolation predicate (self-contained: no `Delegate` step,
    /// every `Invoke` arg resolves to a declared parameter or a literal).
    pub i: bool,
    /// The rule clause that fired.
    pub rule_id: String,
}

/// `B(P)` — the workflow-compilability predicate (§5c.5 row 8): every step
/// lowers onto a §3.2.4 node kind. Failures carry `(step, reason)`.
pub fn b_procedure(
    steps: &[crate::records::ProcedureStep],
    params: &BTreeMap<String, ProcedureParameter>,
    index: &BTreeMap<String, &crate::document::Node>,
    proc_id: &str,
) -> Result<(), (String, WorkflowReason)> {
    use crate::records::ProcedureStep as S;
    for (i, step) in steps.iter().enumerate() {
        let step_id = format!("{proc_id}.step[{i}]");
        match step {
            S::Instruction(_) | S::Verify { .. } | S::Delegate { .. } => {}
            S::Invoke { args, .. } => {
                // unbound_arg — every `$param:` ref must name a declared
                // parameter (the invocation's own binding counts too: a
                // `required` param with a default is bound).
                let mut refs = Vec::new();
                collect_param_refs_json(args, &mut refs);
                for r in refs {
                    if !params.contains_key(&r) {
                        return Err((step_id, WorkflowReason::UnboundArg));
                    }
                }
            }
            S::Branch {
                condition,
                then_body,
                else_body,
            } => {
                // uncheckable_branch — the condition must be validator-
                // backed: `{validator: Ref}` or `{kind: "validator_passed"}`
                // (a free-form condition cannot become branch-on-validator).
                let checkable = condition.get("validator").is_some()
                    || condition.get("validator_ref").is_some()
                    || condition.get("kind").and_then(Json::as_str) == Some("validator_passed");
                if !checkable {
                    return Err((step_id, WorkflowReason::UncheckableBranch));
                }
                b_procedure(then_body, params, index, proc_id)
                    .map_err(|(s, r)| (format!("{step_id}.then/{s}"), r))?;
                b_procedure(else_body, params, index, proc_id)
                    .map_err(|(s, r)| (format!("{step_id}.else/{s}"), r))?;
            }
            S::Loop { bound, body } => {
                // unbounded_loop — the bound must resolve to a `Budget`
                // node (I4's bounded-by-construction).
                let bounded = index
                    .get(bound.semantic_id.as_str())
                    .map(|n| matches!(n.kind, crate::kinds::EntityKind::Budget))
                    .unwrap_or(false);
                if !bounded {
                    return Err((step_id, WorkflowReason::UnboundedLoop));
                }
                b_procedure(body, params, index, proc_id)
                    .map_err(|(s, r)| (format!("{step_id}.body/{s}"), r))?;
            }
            S::Opaque(payload) => {
                if payload.declared_interface.is_none() {
                    return Err((step_id, WorkflowReason::OpaqueWithoutInterface));
                }
            }
        }
    }
    Ok(())
}

/// `R(P)` — the derived risk set: `irreversible` when a derived effect's
/// reversibility is `Irreversible`, `scope:external` when a derived effect's
/// world is `Open` (§5c.5 row 8's `scope = external` spelling).
pub fn r_procedure(
    steps: &[crate::records::ProcedureStep],
    index: &BTreeMap<String, &crate::document::Node>,
) -> Vec<String> {
    let mut effects = Vec::new();
    derived_effects(steps, index, &mut effects);
    let mut r = Vec::new();
    for e in &effects {
        if let Some(attrs) = &e.attributes {
            if matches!(
                attrs.reversibility,
                crate::kinds::Reversibility::Irreversible
            ) && !r.iter().any(|x| x == "irreversible")
            {
                r.push("irreversible".to_string());
            }
            if matches!(attrs.world, crate::kinds::World::Open)
                && !r.iter().any(|x| x == "scope:external")
            {
                r.push("scope:external".to_string());
            }
        }
    }
    r
}

/// `I(P)` — the Stage-3 isolation predicate: the procedure is
/// self-contained — no `Delegate` step, and every `Invoke`'s `$param:` refs
/// resolve to declared parameters (nothing reaches outside the declared
/// surface). A stricter predicate lands with subagents at Stage 4.
pub fn i_procedure(
    steps: &[crate::records::ProcedureStep],
    params: &BTreeMap<String, ProcedureParameter>,
) -> bool {
    use crate::records::ProcedureStep as S;
    for step in steps {
        match step {
            S::Delegate { .. } => return false,
            S::Invoke { args, .. } => {
                let mut refs = Vec::new();
                collect_param_refs_json(args, &mut refs);
                if refs.iter().any(|r| !params.contains_key(r)) {
                    return false;
                }
            }
            S::Branch {
                then_body,
                else_body,
                ..
            } => {
                if !i_procedure(then_body, params) || !i_procedure(else_body, params) {
                    return false;
                }
            }
            S::Loop { body, .. } => {
                if !i_procedure(body, params) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// The procedure body's serialized size (the `instruction` target's
/// `size(body)` — canonical JSON bytes of the step sequence).
pub fn procedure_body_size(steps: &[crate::records::ProcedureStep]) -> u64 {
    Json::Arr(
        steps
            .iter()
            .map(|s| crate::schema::step_json(s, true))
            .collect(),
    )
    .to_canonical_string()
    .len() as u64
}

/// `select_target(P, profile, def)` — the total, pure rule (§5c.5 row 8;
/// AC-R-2.4.5-5: every outcome is a `TargetDecision` or one of the four
/// typed refusals — never a silent fallback):
///
/// 1. `target_override` honoured when feasible else `TargetInfeasible`.
/// 2. `workflow_node` when `B(P)` and the bound control strategy declares
///    `workflow_execution`.
/// 3. `subagent_task` when `I(P)` and the definition binds subagents and
///    the profile declares `subagents` — refused `DelegationUnavailable`
///    before Stage 4 (the spec's honest degradation).
/// 4. `instruction` unless `size(body) > procedure_inline_budget` with no
///    subagent path ⇒ `UnexpressibleSurface`.
/// 5. Risk floor — evaluated before any choice lands: `R(P) ∋
///    {irreversible, scope:external}` requires `expected_evidence ≠ ∅`
///    (`ProcedureUnverifiable`) and every such `Invoke` names a capability
///    the proposer's effective Π admits.
pub fn select_target(
    node: &crate::document::Node,
    profile: Option<&ProcedureProfile>,
    ctx: &SelectCtx,
    index: &BTreeMap<String, &crate::document::Node>,
) -> Result<TargetDecision, SelectError> {
    let proc_id = node.semantic_id();
    let steps = match &node.semantic {
        crate::records::KindRecord::Procedure(rec) => &rec.steps,
        _ => {
            return Err(SelectError::TargetInfeasible {
                detail: format!("{proc_id}: not a Procedure node"),
            })
        }
    };
    let params: BTreeMap<String, ProcedureParameter> =
        profile.map(|p| p.parameters.clone()).unwrap_or_default();
    let b = b_procedure(steps, &params, index, &proc_id);
    let r = r_procedure(steps, index);
    let i_pred = i_procedure(steps, &params);
    let predicates = |target: CompilationTarget, rule_id: &str| TargetDecision {
        target,
        b: b.is_ok(),
        r: r.clone(),
        i: i_pred,
        rule_id: rule_id.to_string(),
    };

    // (v) — the risk floor binds whichever target wins.
    let risky = r
        .iter()
        .any(|x| x == "irreversible" || x == "scope:external");
    let risk_floor = |expected_evidence: &Json| -> Result<(), SelectError> {
        if !risky {
            return Ok(());
        }
        let has_evidence = !matches!(expected_evidence, Json::Null)
            && !matches!(expected_evidence, Json::Obj(m) if m.is_empty())
            && !matches!(expected_evidence, Json::Arr(a) if a.is_empty());
        if !has_evidence {
            return Err(SelectError::ProcedureUnverifiable {
                procedure: proc_id.clone(),
                detail: format!("R(P) = {r:?} but expected_evidence is empty"),
            });
        }
        // Every Invoke on a risky capability must be Π-admitted at the
        // proposer's effective authority — the ctx's granted set.
        if !ctx.granted_capabilities.is_empty() {
            let mut invokes = Vec::new();
            collect_invokes(steps, &mut invokes);
            for t in invokes {
                let tref = t.semantic_id.as_str();
                if !ctx.granted_capabilities.contains(tref) {
                    return Err(SelectError::ProcedureUnverifiable {
                        procedure: proc_id.clone(),
                        detail: format!("Invoke {tref} fails Π at the proposer's authority"),
                    });
                }
            }
        }
        Ok(())
    };
    let expected_evidence = match &node.semantic {
        crate::records::KindRecord::Procedure(rec) => &rec.expected_evidence,
        _ => &Json::Null,
    };

    // `instruction` is the default spelling — only a non-default surface
    // hint (or a profile `target_override`) counts as an authored override.
    let hint = profile
        .and_then(|p| p.target_override)
        .or(match &node.surface {
            Some(SurfaceRecord::Procedure(s)) => match s.compile_hint {
                crate::records::CompileHint::Instruction => None,
                h => Some(h),
            },
            _ => None,
        });

    // (i) — the authored override.
    if let Some(h) = hint {
        match h {
            crate::records::CompileHint::Instruction => {
                risk_floor(expected_evidence)?;
                return Ok(predicates(CompilationTarget::Instruction, "override"));
            }
            crate::records::CompileHint::WorkflowNode => {
                if !ctx.workflow_execution_declared {
                    return Err(SelectError::TargetInfeasible {
                        detail: format!(
                            "{proc_id}: override workflow_node but no bound strategy declares workflow_execution"
                        ),
                    });
                }
                if let Err((step, reason)) = &b {
                    return Err(SelectError::UnexpressibleAsWorkflow {
                        procedure: proc_id,
                        step: step.clone(),
                        reason: *reason,
                    });
                }
                risk_floor(expected_evidence)?;
                return Ok(predicates(CompilationTarget::WorkflowNode, "override"));
            }
            crate::records::CompileHint::SubagentTask => {
                return Err(SelectError::DelegationUnavailable { procedure: proc_id })
            }
        }
    }
    // (ii) — B(P) ∧ workflow_execution declared.
    if ctx.workflow_execution_declared && b.is_ok() {
        risk_floor(expected_evidence)?;
        return Ok(predicates(CompilationTarget::WorkflowNode, "b_workflow"));
    }
    // (iii) — I(P) ∧ subagents bound ∧ declared → refused at Stage 3.
    if i_pred && ctx.subagents_bound && ctx.profile_declares_subagents {
        return Err(SelectError::DelegationUnavailable { procedure: proc_id });
    }
    // (iv) — instruction, bounded.
    let size = procedure_body_size(steps);
    if size > ctx.procedure_inline_budget
        && !(ctx.subagents_bound && ctx.profile_declares_subagents)
    {
        return Err(SelectError::UnexpressibleSurface {
            procedure: proc_id,
            size,
            budget: ctx.procedure_inline_budget,
        });
    }
    risk_floor(expected_evidence)?;
    Ok(predicates(
        CompilationTarget::Instruction,
        "default_instruction",
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hook bodies — the guard-output vocabulary (§5c.5 hooks; AC-R-2.4.5-12)
// ─────────────────────────────────────────────────────────────────────────────

/// The closed guard-output vocabulary (§5c.5: a `ControlBoundary.guard`
/// bound to a `workflow_node` procedure emits only `{narrow, additional_
/// context?, replacement_proposal?}` — `allow` is never a guard output).
pub const GUARD_OUTPUT_KEYS: &[&str] = &["narrow", "additional_context", "replacement_proposal"];

/// The `narrow` member's closed value set.
pub const GUARD_NARROW_VALUES: &[&str] = &["deny", "ask", "none"];

/// `HookGuardError` — the hook-body validation failures (typed; a guard
/// returning `allow` is refused here at `validate`, AC-R-2.4.5-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookGuardError {
    /// The guard's declared output names a key outside the closed
    /// vocabulary.
    GuardOutputOutOfVocabulary {
        /// The offending key.
        key: String,
    },
    /// The guard emits `allow` — refused at validate.
    GuardReturnsAllow,
    /// `narrow` carries a value outside `{deny, ask, none}`.
    BadNarrow {
        /// The value.
        value: String,
    },
    /// The hook body performs world effects (a guard narrows or proposes —
    /// it never acts): an `Invoke` on a non-`pure`/`read_only` tool.
    GuardHasEffects {
        /// The step.
        step: String,
    },
}

impl std::fmt::Display for HookGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for HookGuardError {}

/// `validate_hook_guard(P, outputs)` — the Stage-3 half of AC-R-2.4.5-12.
/// `outputs` is the procedure's declared guard-output object (the members
/// its return value may carry — §5c.5's closed vocabulary). Checks:
///
/// - every output key ∈ `{narrow, additional_context, replacement_proposal}`;
/// - `narrow` ∈ `{deny, ask, none}` (`allow` is never a guard output — a
///   `narrow: allow` or an output key `allow` is refused);
/// - no `Invoke` step targets a capability outside the proposer's granted
///   set when the capability is effect-bearing (a guard proposes, never
///   acts — the `replacement_proposal` re-enters `authorize`, it does not
///   run).
pub fn validate_hook_guard(
    steps: &[crate::records::ProcedureStep],
    outputs: &Json,
    proposer_grants: &std::collections::BTreeSet<String>,
) -> Result<(), HookGuardError> {
    if let Json::Obj(m) = outputs {
        for (k, v) in m {
            if !GUARD_OUTPUT_KEYS.contains(&k.as_str()) {
                if k == "allow" {
                    return Err(HookGuardError::GuardReturnsAllow);
                }
                return Err(HookGuardError::GuardOutputOutOfVocabulary { key: k.clone() });
            }
            if k == "narrow" {
                // `allow` is never a guard output — refuse it before the
                // vocabulary check so the refusal reads `GuardReturnsAllow`.
                if let Json::Str(sv) = v {
                    if sv == "allow" {
                        return Err(HookGuardError::GuardReturnsAllow);
                    }
                }
                let ok = match v {
                    Json::Str(s) => GUARD_NARROW_VALUES.contains(&s.as_str()),
                    // A `narrow` schema object is fine (the emitted values
                    // are still constrained downstream).
                    Json::Obj(_) => true,
                    _ => false,
                };
                if !ok {
                    return Err(HookGuardError::BadNarrow {
                        value: v.to_canonical_string(),
                    });
                }
            }
        }
    }
    // No effect-bearing Invoke — a guard's `Invoke` must name a granted
    // (read-side) capability.
    let mut invokes = Vec::new();
    collect_invokes(steps, &mut invokes);
    for (i, t) in invokes.iter().enumerate() {
        let tref = t.semantic_id.as_str();
        if !proposer_grants.contains(tref) {
            return Err(HookGuardError::GuardHasEffects {
                step: format!("step[{i}] invoke {tref}"),
            });
        }
    }
    Ok(())
}
