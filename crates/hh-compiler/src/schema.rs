//! The `hh-compiler` canonical schema (CC7 — the one source for both directions of every
//! record this crate owns). Everything is `hh_wire::json::Json`; the canonicalizer is
//! `Json::to_canonical_string` (sorted keys, integers only — no float ambiguity in a
//! content address).

use std::collections::BTreeMap;

use hh_hir::records::ScopeBindings;
use hh_wire::json::Json;

use crate::equiv::{
    ArgTransform, EquivalenceEvidence, EvidenceVerdict, SurfaceArgMap, SurfaceBinding,
};
use crate::errors::CompileError;
use crate::lcd::{
    GranularityCeiling, LcdReport, LossEntry, LossKind, LossSeverity, LoweringLossReport,
};
use crate::link::{ConditionedRule, ConditionedRuleHome, TargetSpec};
use crate::plan::{
    BoundSlot, BranchOnValidatorNode, BudgetEnvelope, BudgetRow, DelegateNode, EffectRow, LoopNode,
    PermissionRow, PinnedRef, PlanIds, PlanNode, PlanNodePayload, PolicyTables, ReplanOn,
    RuntimePlan, StepAction, StepMode, StepNode, StopRuleNode, ToolBinding, ValidatorBinding,
};
use crate::profile::{
    CapabilityState, Compliance, ComplianceDetector, DebtStatus, ExtBlock, ModelProfile, ModelRole,
    ProfileCapabilities, ProfileCompatibility, ProfileDebtRecord, ProfileRule, ProfileRuleKind,
    ProfileSelector, VersionPattern,
};
use crate::seal::{CompiledBundle, ModelSurface, ModelSurfaceState};
use crate::trace::{TraceEntry, TraceMap};
use hh_ontology::control::StopKind;

fn schema_err(path: &str, what: impl Into<String>) -> CompileError {
    CompileError::InvalidModelProfile {
        detail: format!("{path}: {}", what.into()),
    }
}

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, CompileError> {
    j.get(k)
        .ok_or_else(|| schema_err(path, format!("{k} missing")))
}

fn str_at(j: &Json, k: &str, path: &str) -> Result<String, CompileError> {
    req(j, k, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| schema_err(path, format!("{k} must be a string")))
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn str_vec(j: &Json, k: &str, path: &str) -> Result<Vec<String>, CompileError> {
    match req(j, k, path)? {
        Json::Arr(items) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| schema_err(path, format!("{k} items must be strings")))
            })
            .collect(),
        _ => Err(schema_err(path, format!("{k} must be an array"))),
    }
}

fn int_at(j: &Json, k: &str, path: &str) -> Result<i64, CompileError> {
    req(j, k, path)?
        .as_int()
        .ok_or_else(|| schema_err(path, format!("{k} must be an integer")))
}

fn str_arr(j: &Json, path: &str) -> Result<Vec<String>, CompileError> {
    match j {
        Json::Arr(items) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| schema_err(path, "items must be strings"))
            })
            .collect(),
        _ => Err(schema_err(path, "expected an array")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ModelProfile/1
// ─────────────────────────────────────────────────────────────────────────────

fn debt_json(d: &ProfileDebtRecord) -> Json {
    let mut expiry = vec![("kind", Json::str(d.expiry_condition.kind.name()))];
    if let Some(v) = &d.expiry_condition.value {
        expiry.push(("value", Json::str(v.clone())));
    }
    let mut m = std::collections::BTreeMap::new();
    // `evidence_refs` — a pure-legacy ref (`{kind: source}` bare) emits the
    // landed `ModelProfile/1` string form; a richer ref emits the typed object
    // (the /1 additive member shape — decode accepts both).
    m.insert(
        "evidence_refs".into(),
        Json::Arr(
            d.evidence_refs
                .iter()
                .map(|r| {
                    if r.kind == hh_ontology::debt::EvidenceKind::Source
                        && r.observed_at.is_none()
                        && r.tier.is_none()
                        && !r.provisional
                    {
                        Json::str(r.reference.clone())
                    } else {
                        r.to_json()
                    }
                })
                .collect(),
        ),
    );
    m.insert("expiry_condition".into(), Json::obj(expiry));
    m.insert("hypothesis".into(), Json::str(d.hypothesis.clone()));
    m.insert("owner".into(), Json::str(d.owner.clone()));
    m.insert(
        "removal_test_ref".into(),
        Json::str(d.removal_test_ref.clone()),
    );
    m.insert("rule_id".into(), Json::str(d.rule_id.clone()));
    m.insert("status".into(), Json::str(d.status.name()));
    // The /1 additive members — emitted only when present (CC8).
    if !d.reach_via.is_empty() {
        m.insert(
            "reach_via".into(),
            Json::Arr(d.reach_via.iter().map(|s| Json::str(s.clone())).collect()),
        );
    }
    if let Some(t) = &d.removal_test {
        m.insert("removal_test".into(), t.to_json());
    }
    if let Some(c) = &d.debt_class {
        m.insert("debt_class".into(), Json::str(c.name()));
    }
    if let Some(h) = &d.hypothesis_typed {
        m.insert("hypothesis_typed".into(), h.to_json());
    }
    if let Some(s) = &d.scope {
        m.insert("scope".into(), s.to_json());
    }
    if let Some(e) = &d.expiry {
        m.insert("expiry".into(), e.to_json());
    }
    if let Some(r) = d.runway_ms {
        m.insert("runway_ms".into(), Json::Int(r as i64));
    }
    if let Some(r) = &d.revalidation {
        m.insert("revalidation".into(), r.to_json());
    }
    if let Some(c) = d.created_at {
        m.insert("created_at".into(), Json::Int(c as i64));
    }
    if let Some(s) = &d.supersedes {
        m.insert("supersedes".into(), Json::str(s.clone()));
    }
    Json::Obj(m)
}

fn debt_member_err(path: &str, e: hh_ontology::debt::DebtSchemaError) -> CompileError {
    schema_err(path, e.detail)
}

fn debt_from_json(j: &Json, path: &str) -> Result<ProfileDebtRecord, CompileError> {
    let expiry = req(j, "expiry_condition", path)?;
    let expiry_condition =
        hh_ontology::debt::ExpiryCondition::from_json(expiry, &format!("{path}.expiry_condition"))
            .map_err(|e| debt_member_err(path, e))?;
    let mut evidence_refs = Vec::new();
    if let Some(Json::Arr(items)) = j.get("evidence_refs") {
        for (i, v) in items.iter().enumerate() {
            evidence_refs.push(
                hh_ontology::debt::EvidenceRef::from_json(v, &format!("{path}.evidence_refs[{i}]"))
                    .map_err(|e| debt_member_err(path, e))?,
            );
        }
    }
    let debt_class = match opt_str(j, "debt_class") {
        None => None,
        Some(s) => Some(
            hh_ontology::debt::DebtClass::parse(&s)
                .ok_or_else(|| schema_err(path, "debt_class: bad DebtClass"))?,
        ),
    };
    Ok(ProfileDebtRecord {
        rule_id: str_at(j, "rule_id", path)?,
        hypothesis: str_at(j, "hypothesis", path)?,
        evidence_refs,
        owner: str_at(j, "owner", path)?,
        reach_via: str_vec(j, "reach_via", path).unwrap_or_default(),
        expiry_condition,
        removal_test_ref: str_at(j, "removal_test_ref", path)?,
        removal_test: match j.get("removal_test") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                hh_ontology::debt::RemovalTest::from_json(v, &format!("{path}.removal_test"))
                    .map_err(|e| debt_member_err(path, e))?,
            ),
        },
        status: DebtStatus::parse(&str_at(j, "status", path)?)
            .ok_or_else(|| schema_err(path, "status: bad DebtStatus"))?,
        debt_class,
        hypothesis_typed: match j.get("hypothesis_typed") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                hh_ontology::debt::HypothesisTyped::from_json(
                    v,
                    &format!("{path}.hypothesis_typed"),
                )
                .map_err(|e| debt_member_err(path, e))?,
            ),
        },
        scope: match j.get("scope") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                hh_ontology::debt::DebtScope::from_json(v, &format!("{path}.scope"))
                    .map_err(|e| debt_member_err(path, e))?,
            ),
        },
        expiry: match j.get("expiry") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                hh_ontology::debt::DebtExpiry::from_json(v, &format!("{path}.expiry"))
                    .map_err(|e| debt_member_err(path, e))?,
            ),
        },
        runway_ms: match j.get("runway_ms") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                v.as_int()
                    .map(|i| i.max(0) as u64)
                    .ok_or_else(|| schema_err(path, "runway_ms: not an integer"))?,
            ),
        },
        revalidation: match j.get("revalidation") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                hh_ontology::debt::Revalidation::from_json(v, &format!("{path}.revalidation"))
                    .map_err(|e| debt_member_err(path, e))?,
            ),
        },
        created_at: match j.get("created_at") {
            None | Some(Json::Null) => None,
            Some(v) => Some(
                v.as_int()
                    .map(|i| i.max(0) as u64)
                    .ok_or_else(|| schema_err(path, "created_at: not an integer"))?,
            ),
        },
        supersedes: opt_str(j, "supersedes"),
    })
}

fn selector_json(s: &ProfileSelector) -> Json {
    let mut pairs = vec![
        ("model_family", Json::str(s.model_family.clone())),
        ("precedence", Json::Int(s.precedence)),
        (
            "provider_api_family",
            Json::str(s.provider_api_family.clone()),
        ),
        (
            "roles_admitted",
            Json::Arr(
                s.roles_admitted
                    .iter()
                    .map(|r| Json::str(r.name()))
                    .collect(),
            ),
        ),
        ("version_pattern", version_pattern_json(&s.version_pattern)),
    ];
    if let Some(r) = &s.successor_ref {
        pairs.push(("successor_ref", Json::str(r.clone())));
    }
    if let Some(r) = &s.retirement_at {
        pairs.push(("retirement_at", Json::str(r.clone())));
    }
    Json::obj(pairs)
}

fn version_pattern_json(p: &VersionPattern) -> Json {
    match p {
        VersionPattern::Exact(v) => Json::obj([("exact", Json::str(v.clone()))]),
        VersionPattern::Prefix(v) => Json::obj([("prefix", Json::str(v.clone()))]),
        VersionPattern::Range { family, lo, hi } => {
            let mut m = std::collections::BTreeMap::new();
            m.insert("family".to_string(), Json::str(family.clone()));
            if let Some(lo) = lo {
                m.insert("lo".to_string(), Json::str(lo.clone()));
            }
            if let Some(hi) = hi {
                m.insert("hi".to_string(), Json::str(hi.clone()));
            }
            Json::Obj(m)
        }
        VersionPattern::Any => Json::str("any"),
    }
}

fn version_pattern_from_json(j: &Json, path: &str) -> Result<VersionPattern, CompileError> {
    match j {
        Json::Str(s) if s == "any" => Ok(VersionPattern::Any),
        Json::Obj(_) => {
            if let Some(v) = j.get("exact").and_then(Json::as_str) {
                return Ok(VersionPattern::Exact(v.to_string()));
            }
            if let Some(v) = j.get("prefix").and_then(Json::as_str) {
                return Ok(VersionPattern::Prefix(v.to_string()));
            }
            // `range{family, lo?, hi?}` — either bound may be absent
            // (unbounded); `{lo, hi}` without `family` reads with the
            // selector's own `model_family` (legacy spelling).
            if j.get("family").and_then(Json::as_str).is_some()
                || j.get("lo").and_then(Json::as_str).is_some()
                || j.get("hi").and_then(Json::as_str).is_some()
            {
                return Ok(VersionPattern::Range {
                    family: j
                        .get("family")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    lo: j.get("lo").and_then(Json::as_str).map(str::to_string),
                    hi: j.get("hi").and_then(Json::as_str).map(str::to_string),
                });
            }
            Err(schema_err(path, "version_pattern: not a closed-set member"))
        }
        _ => Err(schema_err(
            path,
            "version_pattern must be an object or \"any\"",
        )),
    }
}

fn selector_from_json(j: &Json, path: &str) -> Result<ProfileSelector, CompileError> {
    let roles: Vec<ModelRole> = str_vec(j, "roles_admitted", path)?
        .iter()
        .map(|s| {
            ModelRole::parse(s)
                .ok_or_else(|| schema_err(path, format!("roles_admitted: bad role {s}")))
        })
        .collect::<Result<_, _>>()?;
    Ok(ProfileSelector {
        provider_api_family: str_at(j, "provider_api_family", path)?,
        model_family: str_at(j, "model_family", path)?,
        version_pattern: version_pattern_from_json(
            req(j, "version_pattern", path)?,
            &format!("{path}.version_pattern"),
        )?,
        precedence: int_at(j, "precedence", path)?,
        successor_ref: opt_str(j, "successor_ref"),
        retirement_at: opt_str(j, "retirement_at"),
        roles_admitted: roles,
    })
}

fn capabilities_json(c: &ProfileCapabilities) -> Json {
    let state = |s: CapabilityState| Json::str(s.name());
    let mut pairs = vec![
        (
            "assistant_required_after_tool_result",
            state(c.assistant_required_after_tool_result),
        ),
        (
            "cache_control_convention",
            state(c.cache_control_convention),
        ),
        ("deferred_tools", state(c.deferred_tools)),
        ("developer_role", state(c.developer_role)),
        ("grammar_tools", state(c.grammar_tools)),
        ("image_input", state(c.image_input)),
        ("interleaved_reasoning", state(c.interleaved_reasoning)),
        ("native_function_calling", state(c.native_function_calling)),
        ("parallel_tool_calls", state(c.parallel_tool_calls)),
        (
            "reasoning_levels",
            Json::Arr(c.reasoning_levels.iter().map(Json::str).collect()),
        ),
        (
            "reasoning_replay",
            Json::obj([
                ("field", Json::str(c.reasoning_replay.field.clone())),
                ("opaque", state(c.reasoning_replay.opaque)),
            ]),
        ),
        ("seed_honoured", state(c.seed_honoured)),
        ("strict_schema_dialect", state(c.strict_schema_dialect)),
        ("structured_output", state(c.structured_output)),
        ("substitution_allowed", state(c.substitution_allowed)),
        ("temperature_supported", state(c.temperature_supported)),
        (
            "tool_result_name_required",
            state(c.tool_result_name_required),
        ),
        ("tool_result_role", Json::str(c.tool_result_role.clone())),
        ("tool_search", state(c.tool_search)),
    ];
    if let Some(v) = c.context_window {
        pairs.push(("context_window", Json::Int(v as i64)));
    }
    if let Some(v) = c.max_output {
        pairs.push(("max_output", Json::Int(v as i64)));
    }
    if let Some(v) = &c.usage_mapping {
        pairs.push(("usage_mapping", v.clone()));
    }
    if let Some(v) = &c.compatibility_token {
        pairs.push(("compatibility_token", v.clone()));
    }
    Json::obj(pairs)
}

fn capabilities_from_json(j: &Json, path: &str) -> Result<ProfileCapabilities, CompileError> {
    let state = |k: &str| -> Result<CapabilityState, CompileError> {
        CapabilityState::parse(&str_at(j, k, path)?)
            .ok_or_else(|| schema_err(path, format!("{k}: bad CapabilityState")))
    };
    Ok(ProfileCapabilities {
        native_function_calling: state("native_function_calling")?,
        parallel_tool_calls: state("parallel_tool_calls")?,
        strict_schema_dialect: state("strict_schema_dialect")?,
        structured_output: state("structured_output")?,
        grammar_tools: state("grammar_tools")?,
        reasoning_replay: {
            let r = req(j, "reasoning_replay", path)?;
            crate::profile::ReasoningReplayDecl {
                field: str_at(r, "field", &format!("{path}.reasoning_replay"))?,
                opaque: CapabilityState::parse(&str_at(
                    r,
                    "opaque",
                    &format!("{path}.reasoning_replay"),
                )?)
                .ok_or_else(|| {
                    schema_err(
                        &format!("{path}.reasoning_replay"),
                        "opaque: bad CapabilityState",
                    )
                })?,
            }
        },
        interleaved_reasoning: state("interleaved_reasoning")?,
        image_input: state("image_input")?,
        context_window: j
            .get("context_window")
            .and_then(Json::as_int)
            .map(|v| v as u64),
        max_output: j.get("max_output").and_then(Json::as_int).map(|v| v as u64),
        developer_role: state("developer_role")?,
        cache_control_convention: state("cache_control_convention")?,
        tool_result_name_required: state("tool_result_name_required")?,
        assistant_required_after_tool_result: state("assistant_required_after_tool_result")?,
        temperature_supported: state("temperature_supported")?,
        deferred_tools: state("deferred_tools")?,
        tool_search: state("tool_search")?,
        seed_honoured: state("seed_honoured")?,
        substitution_allowed: state("substitution_allowed")?,
        compatibility_token: j.get("compatibility_token").cloned(),
        usage_mapping: j.get("usage_mapping").cloned(),
        tool_result_role: str_at(j, "tool_result_role", path)?,
        reasoning_levels: str_vec(j, "reasoning_levels", path)?,
    })
}

fn rule_json(r: &ProfileRule) -> Json {
    let mut pairs = vec![
        ("compliance", compliance_json(&r.compliance)),
        ("debt", debt_json(&r.debt)),
        ("kind", Json::str(r.kind.name())),
        (
            "owned_fields",
            Json::Arr(r.owned_fields.iter().map(Json::str).collect()),
        ),
        ("params", r.params.clone()),
        ("rule_id", Json::str(r.rule_id.clone())),
    ];
    if let Some(s) = &r.scope {
        pairs.push(("scope", s.clone()));
    }
    if let Some(s) = &r.supersedes {
        pairs.push(("supersedes", Json::str(s.clone())));
    }
    Json::obj(pairs)
}

fn compliance_json(c: &Compliance) -> Json {
    let mut pairs = vec![(
        "detector_class",
        Json::str(match c.detector_class {
            ComplianceDetector::Deterministic => "deterministic",
            ComplianceDetector::Judged => "judged",
            ComplianceDetector::None => "none",
        }),
    )];
    if let Some(r) = &c.followed_predicate_ref {
        pairs.push(("followed_predicate_ref", Json::str(r.clone())));
    }
    Json::obj(pairs)
}

fn compliance_from_json(j: &Json, path: &str) -> Result<Compliance, CompileError> {
    let detector = match str_at(j, "detector_class", path)?.as_str() {
        "deterministic" => ComplianceDetector::Deterministic,
        "judged" => ComplianceDetector::Judged,
        "none" => ComplianceDetector::None,
        other => return Err(schema_err(path, format!("detector_class: {other}"))),
    };
    let followed = opt_str(j, "followed_predicate_ref");
    if detector == ComplianceDetector::Judged && followed.is_none() {
        return Err(schema_err(
            path,
            "judged compliance requires followed_predicate_ref",
        ));
    }
    Ok(Compliance {
        detector_class: detector,
        followed_predicate_ref: followed,
    })
}

fn rule_from_json(j: &Json, path: &str) -> Result<ProfileRule, CompileError> {
    Ok(ProfileRule {
        rule_id: str_at(j, "rule_id", path)?,
        kind: ProfileRuleKind::parse(&str_at(j, "kind", path)?)
            .ok_or_else(|| schema_err(path, "kind: bad ProfileRuleKind"))?,
        owned_fields: str_vec(j, "owned_fields", path)?,
        params: req(j, "params", path)?.clone(),
        debt: debt_from_json(req(j, "debt", path)?, &format!("{path}.debt"))?,
        scope: j.get("scope").cloned(),
        supersedes: opt_str(j, "supersedes"),
        compliance: compliance_from_json(
            req(j, "compliance", path)?,
            &format!("{path}.compliance"),
        )?,
    })
}

/// The canonical JSON of a `ModelProfile/1`.
pub fn profile_to_json(p: &ModelProfile) -> Json {
    let mut pairs = vec![
        ("capabilities", capabilities_json(&p.capabilities)),
        ("compatibility", {
            let c = &p.compatibility;
            Json::obj([
                ("inventory_version", Json::str(c.inventory_version.clone())),
                (
                    "min_compiler_version",
                    Json::str(c.min_compiler_version.clone()),
                ),
            ])
        }),
        ("content_hash", Json::str(p.content_hash.clone())),
        ("expiry", debt_json(&p.expiry)),
        (
            "ext",
            Json::Obj(
                p.ext
                    .iter()
                    .map(|(k, e)| {
                        (
                            k.clone(),
                            Json::obj([("block", e.block.clone()), ("debt", debt_json(&e.debt))]),
                        )
                    })
                    .collect(),
            ),
        ),
        ("profile_id", Json::str(p.profile_id.clone())),
        ("rules", Json::Arr(p.rules.iter().map(rule_json).collect())),
        ("schema", Json::str("ModelProfile/1")),
        ("selector", selector_json(&p.selector)),
        ("tests", p.tests.clone()),
        ("version", Json::str(p.version.clone())),
    ];
    if let Some(e) = &p.extends {
        pairs.push(("extends", Json::str(e.clone())));
    }
    Json::obj(pairs)
}

/// Parse a `ModelProfile/1`.
pub fn profile_from_json(j: &Json, path: &str) -> Result<ModelProfile, CompileError> {
    if str_at(j, "schema", path)? != "ModelProfile/1" {
        return Err(schema_err(path, "schema must be ModelProfile/1"));
    }
    let ext = match j.get("ext") {
        Some(Json::Obj(m)) => m
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    ExtBlock {
                        block: v
                            .get("block")
                            .cloned()
                            .ok_or_else(|| schema_err(path, format!("ext.{k}.block missing")))?,
                        debt: debt_from_json(
                            v.get("debt")
                                .ok_or_else(|| schema_err(path, format!("ext.{k}.debt missing")))?,
                            &format!("{path}.ext.{k}.debt"),
                        )?,
                    },
                ))
            })
            .collect::<Result<BTreeMap<String, ExtBlock>, CompileError>>()?,
        _ => BTreeMap::new(),
    };
    let rules = match req(j, "rules", path)? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, r)| rule_from_json(r, &format!("{path}.rules[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(schema_err(path, "rules must be an array")),
    };
    let compat = req(j, "compatibility", path)?;
    let mut p = ModelProfile {
        profile_id: str_at(j, "profile_id", path)?,
        version: str_at(j, "version", path)?,
        content_hash: str_at(j, "content_hash", path)?,
        selector: selector_from_json(req(j, "selector", path)?, &format!("{path}.selector"))?,
        extends: opt_str(j, "extends"),
        capabilities: capabilities_from_json(
            req(j, "capabilities", path)?,
            &format!("{path}.capabilities"),
        )?,
        rules,
        ext,
        expiry: debt_from_json(req(j, "expiry", path)?, &format!("{path}.expiry"))?,
        compatibility: ProfileCompatibility {
            inventory_version: str_at(
                compat,
                "inventory_version",
                &format!("{path}.compatibility"),
            )?,
            min_compiler_version: str_at(
                compat,
                "min_compiler_version",
                &format!("{path}.compatibility"),
            )?,
        },
        tests: req(j, "tests", path)?.clone(),
    };
    // The content_hash member must equal identity(record minus content_hash) — a claim
    // that doesn't recompute is a schema violation (CC3: nothing unpinned).
    let claimed = p.content_hash.clone();
    p.content_hash = String::new();
    let recomputed = crate::profile::profile_identity(&p);
    p.content_hash = claimed.clone();
    if claimed != recomputed {
        return Err(schema_err(
            path,
            format!("content_hash {claimed} ≠ identity {recomputed}"),
        ));
    }
    Ok(p)
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared leaf codecs
// ─────────────────────────────────────────────────────────────────────────────

fn pinned_ref_json(r: &PinnedRef) -> Json {
    Json::obj([
        ("semantic_id", Json::str(r.semantic_id.clone())),
        ("version_id", Json::str(r.version_id.clone())),
    ])
}

fn pinned_ref_from_json(j: &Json, path: &str) -> Result<PinnedRef, CompileError> {
    Ok(PinnedRef {
        semantic_id: str_at(j, "semantic_id", path)?,
        version_id: str_at(j, "version_id", path)?,
    })
}

/// The canonical JSON of a `Grant` (the plan's policy-row payload — the record's own
/// shape, encoded here because no other crate encodes a bare `Grant` yet).
pub fn grant_json(g: &hh_hir::records::Grant) -> Json {
    let mut pairs = vec![
        ("delegable", Json::Bool(g.delegable)),
        ("effect", g.effect.to_json()),
        ("scope", Json::str(g.scope.clone())),
    ];
    let c = &g.constraints;
    pairs.push((
        "constraints",
        Json::obj([
            ("budget", opt_or_null(c.budget.clone())),
            ("count", opt_or_null(c.count.map(|v| Json::Int(v as i64)))),
            ("time", opt_or_null(c.time.map(|v| Json::Int(v as i64)))),
        ]),
    ));
    Json::obj(pairs)
}

fn opt_or_null(v: Option<Json>) -> Json {
    v.unwrap_or(Json::Null)
}

/// The canonical JSON of a `ScopeBindings` (`scope_bindings` or `{"unknown": true}`).
pub fn scope_bindings_json(s: &ScopeBindings) -> Json {
    match s {
        ScopeBindings::Bindings(b) => Json::obj([("scope_bindings", b.clone())]),
        ScopeBindings::Unknown => Json::obj([("scope_bindings_unknown", Json::Bool(true))]),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RuntimePlan/1
// ─────────────────────────────────────────────────────────────────────────────

fn plan_node_json(n: &PlanNode) -> Json {
    let mut pairs = vec![
        ("hir_node_id", Json::str(n.hir_node_id.clone())),
        ("hir_version_id", Json::str(n.hir_version_id.clone())),
        ("node_id", Json::str(n.node_id.clone())),
    ];
    let (kind, payload) = match &n.payload {
        PlanNodePayload::Loop(l) => (
            "loop",
            Json::obj([
                (
                    "body",
                    Json::Arr(l.body.iter().map(plan_node_json).collect()),
                ),
                ("bound_budget", pinned_ref_json(&l.bound_budget)),
                ("replan_on", Json::str(l.replan_on.name())),
            ]),
        ),
        PlanNodePayload::Step(s) => (
            "step",
            Json::obj({
                let mut p = vec![
                    (
                        "action",
                        match &s.action {
                            StepAction::Instruction {
                                content_hash,
                                owner,
                            } => Json::obj([
                                ("content_hash", Json::str(content_hash.clone())),
                                ("kind", Json::str("instruction")),
                                ("owner", Json::str(owner.clone())),
                            ]),
                            StepAction::Invoke { capability, args } => Json::obj([
                                ("args", args.clone()),
                                ("capability", pinned_ref_json(capability)),
                                ("kind", Json::str("invoke")),
                            ]),
                        },
                    ),
                    (
                        "mode",
                        Json::str(match s.mode {
                            StepMode::Sequential => "sequential",
                            StepMode::Parallel => "parallel",
                        }),
                    ),
                ];
                if let Some(o) = &s.output_schema {
                    p.push(("output_schema", pinned_ref_json(o)));
                }
                p
            }),
        ),
        PlanNodePayload::BranchOnValidator(b) => (
            "branch-on-validator",
            Json::obj([
                (
                    "else",
                    Json::Arr(b.else_body.iter().map(plan_node_json).collect()),
                ),
                (
                    "then",
                    Json::Arr(b.then_body.iter().map(plan_node_json).collect()),
                ),
                ("validator", pinned_ref_json(&b.validator)),
            ]),
        ),
        PlanNodePayload::Delegate(d) => (
            "delegate",
            Json::obj([
                ("budget", pinned_ref_json(&d.budget)),
                ("permission", pinned_ref_json(&d.permission)),
                ("spec", d.spec.clone()),
            ]),
        ),
        PlanNodePayload::StopRule(s) => (
            "stop-rule",
            Json::obj({
                let mut p = vec![("reason", Json::str(s.reason.as_str()))];
                if let Some(b) = &s.bound {
                    p.push(("bound", pinned_ref_json(b)));
                }
                if let Some(c) = &s.condition {
                    p.push(("condition", c.clone()));
                }
                p
            }),
        ),
    };
    pairs.push(("kind", Json::str(kind)));
    pairs.push(("payload", payload));
    Json::obj(pairs)
}

fn plan_node_from_json(j: &Json, path: &str) -> Result<PlanNode, CompileError> {
    let kind = str_at(j, "kind", path)?;
    let payload = req(j, "payload", path)?;
    let pp = format!("{path}.payload");
    let body_nodes = |key: &str| -> Result<Vec<PlanNode>, CompileError> {
        match payload.get(key) {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, n)| plan_node_from_json(n, &format!("{pp}.{key}[{i}]")))
                .collect(),
            _ => Ok(Vec::new()),
        }
    };
    let payload = match kind.as_str() {
        "loop" => PlanNodePayload::Loop(LoopNode {
            bound_budget: pinned_ref_from_json(
                req(payload, "bound_budget", &pp)?,
                &format!("{pp}.bound_budget"),
            )?,
            replan_on: ReplanOn::parse(&str_at(payload, "replan_on", &pp)?)
                .ok_or_else(|| schema_err(&pp, "replan_on: bad value"))?,
            body: body_nodes("body")?,
        }),
        "step" => {
            let action = req(payload, "action", &pp)?;
            let ap = format!("{pp}.action");
            let action = match str_at(action, "kind", &ap)?.as_str() {
                "instruction" => StepAction::Instruction {
                    content_hash: str_at(action, "content_hash", &ap)?,
                    owner: str_at(action, "owner", &ap)?,
                },
                "invoke" => StepAction::Invoke {
                    capability: pinned_ref_from_json(
                        req(action, "capability", &ap)?,
                        &format!("{ap}.capability"),
                    )?,
                    args: req(action, "args", &ap)?.clone(),
                },
                other => return Err(schema_err(&ap, format!("kind: {other}"))),
            };
            PlanNodePayload::Step(StepNode {
                mode: match str_at(payload, "mode", &pp)?.as_str() {
                    "sequential" => StepMode::Sequential,
                    "parallel" => StepMode::Parallel,
                    other => return Err(schema_err(&pp, format!("mode: {other}"))),
                },
                output_schema: match payload.get("output_schema") {
                    Some(v) => Some(pinned_ref_from_json(v, &format!("{pp}.output_schema"))?),
                    None => None,
                },
                action,
            })
        }
        "branch-on-validator" => PlanNodePayload::BranchOnValidator(BranchOnValidatorNode {
            validator: pinned_ref_from_json(
                req(payload, "validator", &pp)?,
                &format!("{pp}.validator"),
            )?,
            then_body: body_nodes("then")?,
            else_body: body_nodes("else")?,
        }),
        "delegate" => PlanNodePayload::Delegate(DelegateNode {
            spec: req(payload, "spec", &pp)?.clone(),
            budget: pinned_ref_from_json(req(payload, "budget", &pp)?, &format!("{pp}.budget"))?,
            permission: pinned_ref_from_json(
                req(payload, "permission", &pp)?,
                &format!("{pp}.permission"),
            )?,
        }),
        "stop-rule" => PlanNodePayload::StopRule(StopRuleNode {
            reason: StopKind::parse(&str_at(payload, "reason", &pp)?)
                .ok_or_else(|| schema_err(&pp, "reason: bad StopKind"))?,
            bound: payload
                .get("bound")
                .map(|b| pinned_ref_from_json(b, &format!("{pp}.bound")))
                .transpose()?,
            condition: payload.get("condition").cloned(),
        }),
        other => return Err(schema_err(path, format!("kind: {other}"))),
    };
    Ok(PlanNode {
        node_id: str_at(j, "node_id", path)?,
        hir_node_id: str_at(j, "hir_node_id", path)?,
        hir_version_id: str_at(j, "hir_version_id", path)?,
        payload,
    })
}

fn transform_json(t: &ArgTransform) -> Json {
    match t {
        ArgTransform::Identity => Json::obj([("kind", Json::str("identity"))]),
        ArgTransform::Rename => Json::obj([("kind", Json::str("rename"))]),
        ArgTransform::Project => Json::obj([("kind", Json::str("project"))]),
        ArgTransform::Const(v) => Json::obj([("kind", Json::str("const")), ("value", v.clone())]),
        ArgTransform::Parse { grammar_ref } => Json::obj([
            ("grammar_ref", Json::str(grammar_ref.clone())),
            ("kind", Json::str("parse")),
        ]),
        ArgTransform::Coerce { declared_loss } => Json::obj({
            let mut p = vec![("kind", Json::str("coerce"))];
            if let Some(l) = declared_loss {
                p.push(("declared_loss", Json::str(l.clone())));
            }
            p
        }),
        ArgTransform::ResolveName { table_ref } => Json::obj([
            ("kind", Json::str("resolve_name")),
            ("table_ref", Json::str(table_ref.clone())),
        ]),
        ArgTransform::Unescape { policy_ref } => Json::obj([
            ("kind", Json::str("unescape")),
            ("policy_ref", Json::str(policy_ref.clone())),
        ]),
    }
}

fn transform_from_json(j: &Json, path: &str) -> Result<ArgTransform, CompileError> {
    match str_at(j, "kind", path)?.as_str() {
        "identity" => Ok(ArgTransform::Identity),
        "rename" => Ok(ArgTransform::Rename),
        "project" => Ok(ArgTransform::Project),
        "const" => Ok(ArgTransform::Const(req(j, "value", path)?.clone())),
        "parse" => Ok(ArgTransform::Parse {
            grammar_ref: str_at(j, "grammar_ref", path)?,
        }),
        "coerce" => Ok(ArgTransform::Coerce {
            declared_loss: opt_str(j, "declared_loss"),
        }),
        "resolve_name" => Ok(ArgTransform::ResolveName {
            table_ref: str_at(j, "table_ref", path)?,
        }),
        "unescape" => Ok(ArgTransform::Unescape {
            policy_ref: str_at(j, "policy_ref", path)?,
        }),
        other => Err(schema_err(path, format!("transform kind: {other}"))),
    }
}

fn arg_map_json(m: &SurfaceArgMap) -> Json {
    arg_map_json_pub(m)
}

/// The canonical `SurfaceArgMap` encoding — `pub(crate)` so
/// `crate::surface::surface_id` hashes the same bytes the wire form writes
/// (CC7 — one encoding).
pub(crate) fn arg_map_json_pub(m: &SurfaceArgMap) -> Json {
    Json::Obj(
        m.iter()
            .map(|(k, e)| {
                let mut o = vec![
                    ("capability_param", Json::str(e.capability_param.clone())),
                    ("transform", transform_json(&e.transform)),
                ];
                if let Some(n) = &e.narrowing {
                    o.push(("narrowing", Json::str(n.clone())));
                }
                (k.clone(), Json::obj(o))
            })
            .collect(),
    )
}

fn arg_map_from_json(j: &Json, path: &str) -> Result<SurfaceArgMap, CompileError> {
    match j {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| {
                let p = format!("{path}.{k}");
                Ok((
                    k.clone(),
                    crate::equiv::ArgMapEntry {
                        capability_param: str_at(v, "capability_param", &p)?,
                        transform: transform_from_json(
                            req(v, "transform", &p)?,
                            &format!("{p}.transform"),
                        )?,
                        narrowing: opt_str(v, "narrowing"),
                    },
                ))
            })
            .collect(),
        _ => Err(schema_err(path, "arg_map must be an object")),
    }
}

/// The canonical JSON of a `SurfaceBinding` (the codec's write direction —
/// consumed by `hh-monitor`'s `authorize-input/1` capability rows; CC7).
pub fn surface_binding_json(b: &SurfaceBinding) -> Json {
    Json::obj([
        ("arg_map", arg_map_json(&b.arg_map)),
        (
            "admitted_modes",
            Json::Arr(
                b.admitted_modes
                    .iter()
                    .map(|m| Json::str(m.as_str()))
                    .collect(),
            ),
        ),
        ("capability_ref", pinned_ref_json(&b.capability_ref)),
        (
            "capability_refs",
            Json::Arr(
                b.capability_refs
                    .iter()
                    .map(|r| Json::str(r.clone()))
                    .collect(),
            ),
        ),
        ("dialect", Json::str(b.dialect.clone())),
        (
            "effects_bound",
            Json::Arr(
                b.effects_bound
                    .iter()
                    .map(|e| Json::str(e.clone()))
                    .collect(),
            ),
        ),
        (
            "evidence_ref",
            b.evidence_ref.clone().map_or(Json::Null, Json::str),
        ),
        ("exposure_mode", Json::str(b.exposure_mode.as_str())),
        (
            "family_id",
            b.family_id.clone().map_or(Json::Null, Json::str),
        ),
        ("hidden", Json::Bool(b.hidden)),
        ("hir_node_id", Json::str(b.hir_node_id.clone())),
        ("mapping", {
            match &b.mapping {
                crate::surface::BindingMapping::SurfaceArgMap => {
                    Json::obj([("kind", Json::str("surface_arg_map"))])
                }
                crate::surface::BindingMapping::PlanMap(r) => Json::obj([
                    ("kind", Json::str("plan_map")),
                    ("plan_ref", Json::str(r.clone())),
                ]),
            }
        }),
        ("pinned", Json::Bool(b.pinned)),
        (
            "rule_ids",
            Json::Arr(b.rule_ids.iter().map(|r| Json::str(r.clone())).collect()),
        ),
        (
            "safety_ref",
            b.safety_ref.clone().map_or(Json::Null, Json::str),
        ),
        ("surface_id", Json::str(b.surface_id.clone())),
        ("surface_name", Json::str(b.surface_name.clone())),
        (
            "variant_id",
            b.variant_id.clone().map_or(Json::Null, Json::str),
        ),
    ])
}

/// Parse a `SurfaceBinding` from its canonical JSON (the codec's read
/// direction — consumed by `hh-monitor`'s out-of-process `authorize`).
pub fn surface_binding_from_json(j: &Json, path: &str) -> Result<SurfaceBinding, CompileError> {
    let mapping = match j.get("mapping") {
        Some(m) => match m.get("kind").and_then(Json::as_str) {
            Some("surface_arg_map") => crate::surface::BindingMapping::SurfaceArgMap,
            Some("plan_map") => crate::surface::BindingMapping::PlanMap(
                m.get("plan_ref")
                    .and_then(Json::as_str)
                    .ok_or_else(|| {
                        schema_err(&format!("{path}.mapping"), "plan_map needs plan_ref")
                    })?
                    .to_string(),
            ),
            other => {
                return Err(schema_err(
                    &format!("{path}.mapping"),
                    format!("mapping kind: {other:?}"),
                ))
            }
        },
        // CC8: a pre-S1.17 body decodes as the C0 mapping.
        None => crate::surface::BindingMapping::SurfaceArgMap,
    };
    let admitted_modes = match j.get("admitted_modes") {
        Some(Json::Arr(ms)) => {
            let mut set = std::collections::BTreeSet::new();
            for m in ms {
                let sp = m.as_str().ok_or_else(|| {
                    schema_err(&format!("{path}.admitted_modes"), "mode must be a string")
                })?;
                set.insert(hh_hir::tools::ExposureMode::parse(sp).ok_or_else(|| {
                    schema_err(&format!("{path}.admitted_modes"), format!("mode {sp}"))
                })?);
            }
            set
        }
        Some(_) => {
            return Err(schema_err(
                &format!("{path}.admitted_modes"),
                "must be an array",
            ))
        }
        None => [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
    };
    let str_list = |k: &str| -> Result<Vec<String>, CompileError> {
        match j.get(k) {
            Some(Json::Arr(xs)) => xs
                .iter()
                .map(|x| {
                    x.as_str().map(String::from).ok_or_else(|| {
                        schema_err(&format!("{path}.{k}"), "member must be a string")
                    })
                })
                .collect(),
            Some(_) => Err(schema_err(&format!("{path}.{k}"), "must be an array")),
            None => Ok(Vec::new()),
        }
    };
    let mut b = SurfaceBinding {
        surface_name: str_at(j, "surface_name", path)?,
        surface_id: j
            .get("surface_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        exposure_mode: match j.get("exposure_mode").and_then(Json::as_str) {
            Some(sp) => crate::surface::CompileExposureMode::parse(sp).ok_or_else(|| {
                schema_err(&format!("{path}.exposure_mode"), format!("mode {sp}"))
            })?,
            None => crate::surface::CompileExposureMode::Primitive,
        },
        capability_ref: pinned_ref_from_json(
            req(j, "capability_ref", path)?,
            &format!("{path}.capability_ref"),
        )?,
        capability_refs: str_list("capability_refs")?,
        hir_node_id: str_at(j, "hir_node_id", path)?,
        arg_map: arg_map_from_json(req(j, "arg_map", path)?, &format!("{path}.arg_map"))?,
        mapping,
        rule_ids: str_list("rule_ids")?,
        evidence_ref: j
            .get("evidence_ref")
            .and_then(Json::as_str)
            .map(String::from),
        safety_ref: j.get("safety_ref").and_then(Json::as_str).map(String::from),
        effects_bound: str_list("effects_bound")?,
        family_id: j.get("family_id").and_then(Json::as_str).map(String::from),
        variant_id: j.get("variant_id").and_then(Json::as_str).map(String::from),
        dialect: str_at(j, "dialect", path)?,
        admitted_modes,
        pinned: matches!(j.get("pinned"), Some(Json::Bool(true))),
        hidden: matches!(j.get("hidden"), Some(Json::Bool(true))),
    };
    // `capability_refs` absent (a pre-S1.17 body) ⇒ the single pinned
    // capability's semantic id; present ⇒ must agree with `capability_ref` at
    // C0 (one-capability bindings — composites land at C1).
    if b.capability_refs.is_empty() {
        b.capability_refs = vec![b.capability_ref.semantic_id.clone()];
    } else if b.mapping == crate::surface::BindingMapping::SurfaceArgMap
        && b.capability_refs != vec![b.capability_ref.semantic_id.clone()]
    {
        return Err(schema_err(
            &format!("{path}.capability_refs"),
            "surface_arg_map bindings name exactly capability_ref.semantic_id",
        ));
    }
    // `surface_id` absent ⇒ minted; present ⇒ verified (integrity, never
    // silently re-derived — I-CLOSED).
    let computed = crate::surface::surface_id(&b);
    if b.surface_id.is_empty() {
        b.surface_id = computed;
    } else if b.surface_id != computed {
        return Err(schema_err(
            &format!("{path}.surface_id"),
            format!(
                "surface_id {} does not re-mint to {}",
                b.surface_id, computed
            ),
        ));
    }
    Ok(b)
}

fn tool_binding_json(t: &ToolBinding) -> Json {
    let mut pairs = vec![
        ("capability", pinned_ref_json(&t.capability)),
        (
            "effects",
            Json::Arr(t.effects.iter().map(|e| e.to_json()).collect()),
        ),
        ("observation_contract", t.observation_contract.clone()),
        (
            "preconditions",
            Json::Arr(t.preconditions.iter().map(Json::str).collect()),
        ),
        ("scope_bindings", t.scope_bindings.clone()),
    ];
    if let Some(s) = &t.surface {
        pairs.push(("surface", surface_binding_json(s)));
    }
    Json::obj(pairs)
}

fn tool_binding_from_json(j: &Json, path: &str) -> Result<ToolBinding, CompileError> {
    let effects = match req(j, "effects", path)? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, e)| {
                hh_hir::EffectClass::from_json(e, &format!("{path}.effects[{i}]"))
                    .map_err(|e| schema_err(path, format!("{e}")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(schema_err(path, "effects must be an array")),
    };
    Ok(ToolBinding {
        capability: pinned_ref_from_json(
            req(j, "capability", path)?,
            &format!("{path}.capability"),
        )?,
        effects,
        preconditions: str_vec(j, "preconditions", path)?,
        scope_bindings: req(j, "scope_bindings", path)?.clone(),
        observation_contract: req(j, "observation_contract", path)?.clone(),
        surface: j
            .get("surface")
            .map(|s| surface_binding_from_json(s, &format!("{path}.surface")))
            .transpose()?,
    })
}

fn dimensions_json(d: &BTreeMap<String, hh_hir::records::DimensionBound>) -> Json {
    Json::Obj(
        d.iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Json::obj([
                        ("hard", opt_or_null(v.hard.map(|h| Json::Int(h as i64)))),
                        ("soft", opt_or_null(v.soft.map(|s| Json::Int(s as i64)))),
                    ]),
                )
            })
            .collect(),
    )
}

fn dimensions_from_json(
    j: &Json,
    path: &str,
) -> Result<BTreeMap<String, hh_hir::records::DimensionBound>, CompileError> {
    match j {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    hh_hir::records::DimensionBound {
                        hard: v.get("hard").and_then(Json::as_int).map(|x| x as u64),
                        soft: v.get("soft").and_then(Json::as_int).map(|x| x as u64),
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, CompileError>>()
            .map_err(|e| schema_err(path, format!("{e:?}"))),
        _ => Err(schema_err(path, "dimensions must be an object")),
    }
}

fn policies_json(p: &PolicyTables) -> Json {
    Json::obj([
        (
            "budgets",
            Json::Arr(
                p.budgets
                    .iter()
                    .map(|b| {
                        Json::obj([
                            ("budget", pinned_ref_json(&b.budget)),
                            ("dimensions", dimensions_json(&b.dimensions)),
                            (
                                "parent",
                                opt_or_null(b.parent.as_ref().map(|s| Json::str(s.clone()))),
                            ),
                            ("scope", Json::str(b.scope.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "effect_classes",
            Json::Arr(
                p.effect_classes
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("capability", Json::str(r.capability.clone())),
                            ("effect_class", r.effect_class.to_json()),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "permissions",
            Json::Arr(
                p.permissions
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("grants", Json::Arr(r.grants.clone())),
                            ("holder", pinned_ref_json(&r.holder)),
                            ("issuer_authority", Json::str(r.issuer_authority.clone())),
                            ("permission", pinned_ref_json(&r.permission)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn policies_from_json(j: &Json, path: &str) -> Result<PolicyTables, CompileError> {
    let rows = |k: &str| -> Result<Vec<Json>, CompileError> {
        match req(j, k, path)? {
            Json::Arr(items) => Ok(items.clone()),
            _ => Err(schema_err(path, format!("{k} must be an array"))),
        }
    };
    Ok(PolicyTables {
        permissions: rows("permissions")?
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let p = format!("{path}.permissions[{i}]");
                Ok(PermissionRow {
                    permission: pinned_ref_from_json(
                        req(r, "permission", &p)?,
                        &format!("{p}.permission"),
                    )?,
                    holder: pinned_ref_from_json(req(r, "holder", &p)?, &format!("{p}.holder"))?,
                    grants: match req(r, "grants", &p)? {
                        Json::Arr(g) => g.clone(),
                        _ => return Err(schema_err(&p, "grants must be an array")),
                    },
                    issuer_authority: str_at(r, "issuer_authority", &p)?,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        effect_classes: rows("effect_classes")?
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let p = format!("{path}.effect_classes[{i}]");
                Ok(EffectRow {
                    capability: str_at(r, "capability", &p)?,
                    effect_class: hh_hir::EffectClass::from_json(
                        req(r, "effect_class", &p)?,
                        &format!("{p}.effect_class"),
                    )
                    .map_err(|e| schema_err(&p, format!("{e}")))?,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        budgets: rows("budgets")?
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let p = format!("{path}.budgets[{i}]");
                Ok(BudgetRow {
                    budget: pinned_ref_from_json(req(r, "budget", &p)?, &format!("{p}.budget"))?,
                    scope: str_at(r, "scope", &p)?,
                    parent: opt_str(r, "parent"),
                    dimensions: dimensions_from_json(
                        req(r, "dimensions", &p)?,
                        &format!("{p}.dimensions"),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
    })
}

fn bound_slot_json(s: &BoundSlot) -> Json {
    Json::obj([
        ("class_id", Json::str(s.class_id.clone())),
        ("declared_on", Json::str(s.declared_on.clone())),
        ("enabled", Json::Bool(s.enabled)),
        (
            "params",
            Json::Obj(
                s.params
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        ),
        ("slot", Json::str(s.slot.clone())),
        ("variant_id", Json::str(s.variant_id.clone())),
        ("version_id", Json::str(s.version_id.clone())),
    ])
}

fn bound_slot_from_json(j: &Json, path: &str) -> Result<BoundSlot, CompileError> {
    Ok(BoundSlot {
        slot: str_at(j, "slot", path)?,
        class_id: str_at(j, "class_id", path)?,
        variant_id: str_at(j, "variant_id", path)?,
        version_id: str_at(j, "version_id", path)?,
        params: match j.get("params") {
            Some(Json::Obj(m)) => m.clone(),
            _ => BTreeMap::new(),
        },
        enabled: matches!(j.get("enabled"), Some(Json::Bool(true))),
        declared_on: str_at(j, "declared_on", path)?,
    })
}

fn validator_binding_json(v: &ValidatorBinding) -> Json {
    Json::obj([
        ("carries_debt", Json::Bool(v.carries_debt)),
        ("deterministic", Json::Bool(v.deterministic)),
        (
            "inputs",
            Json::Arr(v.inputs.iter().map(pinned_ref_json).collect()),
        ),
        ("kind", Json::str(v.kind.clone())),
        (
            "required_by",
            Json::Arr(v.required_by.iter().map(Json::str).collect()),
        ),
        ("validator", pinned_ref_json(&v.validator)),
    ])
}

fn validator_binding_from_json(j: &Json, path: &str) -> Result<ValidatorBinding, CompileError> {
    Ok(ValidatorBinding {
        validator: pinned_ref_from_json(req(j, "validator", path)?, &format!("{path}.validator"))?,
        kind: str_at(j, "kind", path)?,
        deterministic: matches!(j.get("deterministic"), Some(Json::Bool(true))),
        inputs: match req(j, "inputs", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, r)| pinned_ref_from_json(r, &format!("{path}.inputs[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "inputs must be an array")),
        },
        required_by: str_vec(j, "required_by", path)?,
        carries_debt: matches!(j.get("carries_debt"), Some(Json::Bool(true))),
    })
}

fn budget_envelope_json(b: &BudgetEnvelope) -> Json {
    Json::obj([
        ("budget", pinned_ref_json(&b.budget)),
        (
            "control_boundary",
            hh_hir::wire::boundary_json(&b.control_boundary),
        ),
        ("dimensions", dimensions_json(&b.dimensions)),
        (
            "stop_rule_nodes",
            Json::Arr(b.stop_rule_nodes.iter().map(Json::str).collect()),
        ),
    ])
}

fn budget_envelope_from_json(j: &Json, path: &str) -> Result<BudgetEnvelope, CompileError> {
    Ok(BudgetEnvelope {
        budget: pinned_ref_from_json(req(j, "budget", path)?, &format!("{path}.budget"))?,
        dimensions: dimensions_from_json(
            req(j, "dimensions", path)?,
            &format!("{path}.dimensions"),
        )?,
        control_boundary: hh_hir::wire::boundary_from_json(
            req(j, "control_boundary", path)?,
            &format!("{path}.control_boundary"),
        )
        .map_err(|e| schema_err(path, format!("{e}")))?,
        stop_rule_nodes: str_vec(j, "stop_rule_nodes", path)?,
    })
}

/// The canonical JSON of a `RuntimePlan/1`.
pub fn plan_to_json(p: &RuntimePlan) -> Json {
    let mut pairs = vec![
        (
            "bound_slots",
            Json::Obj(
                p.bound_slots
                    .iter()
                    .map(|(k, v)| (k.clone(), bound_slot_json(v)))
                    .collect(),
            ),
        ),
        (
            "control",
            Json::Arr(p.control.iter().map(plan_node_json).collect()),
        ),
        (
            "ids",
            Json::obj([
                (
                    "artifact_ids",
                    Json::Obj(
                        p.ids
                            .artifact_ids
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                            .collect(),
                    ),
                ),
                (
                    "definition_ref",
                    Json::obj([
                        (
                            "semantic_id",
                            Json::str(p.ids.definition_ref.semantic_id.clone()),
                        ),
                        (
                            "version_id",
                            Json::str(p.ids.definition_ref.version_id.clone()),
                        ),
                    ]),
                ),
            ]),
        ),
        ("policies", policies_json(&p.policies)),
        ("plan_version", Json::str("RuntimePlan/1")),
        (
            "tools",
            Json::Arr(p.tools.iter().map(tool_binding_json).collect()),
        ),
        (
            "validators",
            Json::Arr(p.validators.iter().map(validator_binding_json).collect()),
        ),
    ];
    if let Some(b) = &p.budget {
        pairs.push(("budget", budget_envelope_json(b)));
    }
    if let Some(c) = &p.context_policy {
        pairs.push(("context_policy", bound_slot_json(c)));
    }
    Json::obj(pairs)
}

/// Parse a `RuntimePlan/1`.
pub fn plan_from_json(j: &Json, path: &str) -> Result<RuntimePlan, CompileError> {
    if str_at(j, "plan_version", path)? != "RuntimePlan/1" {
        return Err(schema_err(path, "plan_version must be RuntimePlan/1"));
    }
    let arr = |k: &str| -> Result<Vec<Json>, CompileError> {
        match req(j, k, path)? {
            Json::Arr(items) => Ok(items.clone()),
            _ => Err(schema_err(path, format!("{k} must be an array"))),
        }
    };
    let ids = req(j, "ids", path)?;
    let dref = req(ids, "definition_ref", &format!("{path}.ids"))?;
    Ok(RuntimePlan {
        control: arr("control")?
            .iter()
            .enumerate()
            .map(|(i, n)| plan_node_from_json(n, &format!("{path}.control[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        tools: arr("tools")?
            .iter()
            .enumerate()
            .map(|(i, t)| tool_binding_from_json(t, &format!("{path}.tools[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        policies: policies_from_json(req(j, "policies", path)?, &format!("{path}.policies"))?,
        budget: j
            .get("budget")
            .map(|b| budget_envelope_from_json(b, &format!("{path}.budget")))
            .transpose()?,
        context_policy: j
            .get("context_policy")
            .map(|c| bound_slot_from_json(c, &format!("{path}.context_policy")))
            .transpose()?,
        bound_slots: match req(j, "bound_slots", path)? {
            Json::Obj(m) => m
                .iter()
                .map(|(k, v)| {
                    bound_slot_from_json(v, &format!("{path}.bound_slots.{k}"))
                        .map(|s| (k.clone(), s))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            _ => return Err(schema_err(path, "bound_slots must be an object")),
        },
        validators: arr("validators")?
            .iter()
            .enumerate()
            .map(|(i, v)| validator_binding_from_json(v, &format!("{path}.validators[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        ids: PlanIds {
            definition_ref: hh_hir::DefinitionVersionRef {
                semantic_id: str_at(dref, "semantic_id", &format!("{path}.ids.definition_ref"))?,
                version_id: str_at(dref, "version_id", &format!("{path}.ids.definition_ref"))?,
            },
            artifact_ids: match ids.get("artifact_ids") {
                Some(Json::Obj(m)) => m
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect(),
                _ => BTreeMap::new(),
            },
        },
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// TraceMap / LcdReport / EquivalenceEvidence / CompiledBundle
// ─────────────────────────────────────────────────────────────────────────────

fn trace_map_json(m: &TraceMap) -> Json {
    Json::Obj(
        m.entries
            .iter()
            .map(|(k, e)| {
                (
                    k.clone(),
                    Json::obj([
                        (
                            "hir_node_ids",
                            Json::Arr(e.hir_node_ids.iter().map(Json::str).collect()),
                        ),
                        (
                            "rule_ids",
                            Json::Arr(e.rule_ids.iter().map(Json::str).collect()),
                        ),
                        (
                            "target_rule_ids",
                            Json::Arr(e.target_rule_ids.iter().map(Json::str).collect()),
                        ),
                    ]),
                )
            })
            .collect(),
    )
}

fn trace_map_from_json(j: &Json, path: &str) -> Result<TraceMap, CompileError> {
    match j {
        Json::Obj(m) => {
            let entries = m
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.clone(),
                        TraceEntry {
                            hir_node_ids: str_vec(v, "hir_node_ids", &format!("{path}.{k}"))?,
                            rule_ids: str_vec(v, "rule_ids", &format!("{path}.{k}"))?,
                            target_rule_ids: str_vec(v, "target_rule_ids", &format!("{path}.{k}"))?,
                        },
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, CompileError>>()?;
            Ok(TraceMap { entries })
        }
        _ => Err(schema_err(path, "trace_map must be an object")),
    }
}

fn conditioned_json(c: &ConditionedRule) -> Json {
    let mut pairs = vec![
        ("home", Json::str(c.home.name())),
        ("owner", Json::str(c.owner.clone())),
        ("rule_id", Json::str(c.rule_id.clone())),
        ("status", Json::str(c.status.clone())),
    ];
    // The §5h.6 additions — `evidence_grade` (derived) and
    // `removal_test.executable` — emitted when the home supplies them.
    if let Some(g) = &c.evidence_grade {
        pairs.push(("evidence_grade", Json::str(g.clone())));
    }
    if let Some(e) = c.removal_test_executable {
        pairs.push(("removal_test_executable", Json::Bool(e)));
    }
    Json::obj(pairs)
}

fn conditioned_from_json(j: &Json, path: &str) -> Result<ConditionedRule, CompileError> {
    Ok(ConditionedRule {
        rule_id: str_at(j, "rule_id", path)?,
        home: match str_at(j, "home", path)?.as_str() {
            "profile" => ConditionedRuleHome::Profile,
            "definition" => ConditionedRuleHome::Definition,
            "variant" => ConditionedRuleHome::Variant,
            other => return Err(schema_err(path, format!("home: {other}"))),
        },
        status: str_at(j, "status", path)?,
        owner: str_at(j, "owner", path)?,
        evidence_grade: opt_str(j, "evidence_grade"),
        removal_test_executable: match j.get("removal_test_executable") {
            None | Some(Json::Null) => None,
            Some(Json::Bool(b)) => Some(*b),
            _ => return Err(schema_err(path, "removal_test_executable: not a bool")),
        },
    })
}

fn loss_json(l: &LoweringLossReport) -> Json {
    Json::obj([
        (
            "entries",
            Json::Arr(
                l.entries
                    .iter()
                    .map(|e| {
                        let mut p = vec![
                            ("class", Json::str(e.class.name())),
                            ("detail", Json::str(e.detail.clone())),
                            ("field", Json::str(e.field.clone())),
                            ("hir_node_id", Json::str(e.hir_node_id.clone())),
                            ("severity", Json::str(e.severity.name())),
                        ];
                        if let Some(d) = &e.debt_ref {
                            p.push(("debt_ref", Json::str(d.clone())));
                        }
                        Json::obj(p)
                    })
                    .collect(),
            ),
        ),
        (
            "granularity_ceiling",
            Json::str(l.granularity_ceiling.name()),
        ),
        ("target", Json::str(l.target.clone())),
        ("target_version", Json::str(l.target_version.clone())),
    ])
}

fn loss_from_json(j: &Json, path: &str) -> Result<LoweringLossReport, CompileError> {
    let entries = match req(j, "entries", path)? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let p = format!("{path}.entries[{i}]");
                Ok(LossEntry {
                    hir_node_id: str_at(e, "hir_node_id", &p)?,
                    field: str_at(e, "field", &p)?,
                    class: LossKind::parse(&str_at(e, "class", &p)?)
                        .ok_or_else(|| schema_err(&p, "class: bad LossKind"))?,
                    severity: LossSeverity::parse(&str_at(e, "severity", &p)?)
                        .ok_or_else(|| schema_err(&p, "severity: bad LossSeverity"))?,
                    detail: str_at(e, "detail", &p)?,
                    debt_ref: opt_str(e, "debt_ref"),
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        _ => return Err(schema_err(path, "entries must be an array")),
    };
    Ok(LoweringLossReport {
        target: str_at(j, "target", path)?,
        target_version: str_at(j, "target_version", path)?,
        entries,
        granularity_ceiling: GranularityCeiling::parse(&str_at(j, "granularity_ceiling", path)?)
            .ok_or_else(|| schema_err(path, "granularity_ceiling: bad value"))?,
    })
}

fn lcd_json(r: &LcdReport) -> Json {
    Json::obj([
        (
            "benchmark_conditioned_rules",
            Json::Arr(
                r.benchmark_conditioned_rules
                    .iter()
                    .map(Json::str)
                    .collect(),
            ),
        ),
        (
            "conditioned_rules",
            Json::Arr(r.conditioned_rules.iter().map(conditioned_json).collect()),
        ),
        (
            "hosting_edges",
            Json::Arr(r.hosting_edges.iter().map(Json::str).collect()),
        ),
        (
            "identity_stability",
            opt_or_null(r.identity_stability.map(Json::Bool)),
        ),
        (
            "lowering_loss",
            Json::Arr(r.lowering_loss.iter().map(loss_json).collect()),
        ),
        (
            "opacity",
            opt_or_null(r.opacity.map(|(n, t)| {
                Json::obj([
                    ("opaque", Json::Int(n as i64)),
                    ("total", Json::Int(t as i64)),
                ])
            })),
        ),
        (
            "per_profile_diff_fields",
            Json::Obj(
                r.per_profile_diff_fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Arr(v.iter().map(Json::str).collect())))
                    .collect(),
            ),
        ),
    ])
}

fn lcd_from_json(j: &Json, path: &str) -> Result<LcdReport, CompileError> {
    Ok(LcdReport {
        benchmark_conditioned_rules: str_vec(j, "benchmark_conditioned_rules", path)?,
        identity_stability: match j.get("identity_stability") {
            Some(Json::Bool(b)) => Some(*b),
            _ => None,
        },
        hosting_edges: str_vec(j, "hosting_edges", path)?,
        opacity: match j.get("opacity") {
            Some(Json::Obj(_)) => {
                let o = j.get("opacity").expect("checked");
                match (
                    o.get("opaque").and_then(Json::as_int),
                    o.get("total").and_then(Json::as_int),
                ) {
                    (Some(n), Some(t)) => Some((n as usize, t as usize)),
                    _ => None,
                }
            }
            _ => None,
        },
        conditioned_rules: match req(j, "conditioned_rules", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, c)| conditioned_from_json(c, &format!("{path}.conditioned_rules[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "conditioned_rules must be an array")),
        },
        per_profile_diff_fields: match req(j, "per_profile_diff_fields", path)? {
            Json::Obj(m) => m
                .iter()
                .map(|(k, v)| {
                    str_arr(v, &format!("{path}.per_profile_diff_fields.{k}"))
                        .map(|f| (k.clone(), f))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            _ => {
                return Err(schema_err(
                    path,
                    "per_profile_diff_fields must be an object",
                ))
            }
        },
        lowering_loss: match req(j, "lowering_loss", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, l)| loss_from_json(l, &format!("{path}.lowering_loss[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "lowering_loss must be an array")),
        },
    })
}

fn verdict_json(v: &EvidenceVerdict) -> Json {
    match v {
        EvidenceVerdict::Pass => Json::str("pass"),
        EvidenceVerdict::Fail { reason } => Json::obj([("fail", Json::str(reason.clone()))]),
        EvidenceVerdict::NotApplicable { reason } => {
            Json::obj([("n/a", Json::str(reason.clone()))])
        }
    }
}

fn verdict_from_json(j: &Json, path: &str) -> Result<EvidenceVerdict, CompileError> {
    match j {
        Json::Str(s) if s == "pass" => Ok(EvidenceVerdict::Pass),
        Json::Obj(_) => {
            if let Some(r) = j.get("fail").and_then(Json::as_str) {
                return Ok(EvidenceVerdict::fail(r));
            }
            if let Some(r) = j.get("n/a").and_then(Json::as_str) {
                return Ok(EvidenceVerdict::na(r));
            }
            Err(schema_err(path, "verdict: bad shape"))
        }
        _ => Err(schema_err(path, "verdict: bad shape")),
    }
}

fn evidence_json(e: &EquivalenceEvidence) -> Json {
    Json::obj([
        ("capability", Json::str(e.capability.clone())),
        ("e1", verdict_json(&e.e1_effect_equality)),
        ("e2", verdict_json(&e.e2_authority)),
        ("e3", verdict_json(&e.e3_precondition_domain)),
        ("e4", verdict_json(&e.e4_differential)),
        ("e5", verdict_json(&e.e5_error_surjectivity)),
        ("e6", verdict_json(&e.e6_result_observation)),
        ("e7", verdict_json(&e.e7_accounting_identity)),
        ("surface_name", Json::str(e.surface_name.clone())),
    ])
}

fn evidence_from_json(j: &Json, path: &str) -> Result<EquivalenceEvidence, CompileError> {
    Ok(EquivalenceEvidence {
        surface_name: str_at(j, "surface_name", path)?,
        capability: str_at(j, "capability", path)?,
        e1_effect_equality: verdict_from_json(req(j, "e1", path)?, &format!("{path}.e1"))?,
        e2_authority: verdict_from_json(req(j, "e2", path)?, &format!("{path}.e2"))?,
        e3_precondition_domain: verdict_from_json(req(j, "e3", path)?, &format!("{path}.e3"))?,
        e4_differential: verdict_from_json(req(j, "e4", path)?, &format!("{path}.e4"))?,
        e5_error_surjectivity: verdict_from_json(req(j, "e5", path)?, &format!("{path}.e5"))?,
        e6_result_observation: verdict_from_json(req(j, "e6", path)?, &format!("{path}.e6"))?,
        e7_accounting_identity: verdict_from_json(req(j, "e7", path)?, &format!("{path}.e7"))?,
    })
}

fn error_spec_json(s: &crate::surface::ErrorFormatSpec) -> Json {
    Json::obj([
        (
            "distinguishability",
            Json::str(s.distinguishability.as_str()),
        ),
        (
            "renderings",
            Json::Obj(
                s.renderings
                    .iter()
                    .map(|(k, t)| (k.clone(), t.to_json()))
                    .collect(),
            ),
        ),
    ])
}

fn error_spec_from_json(
    j: &Json,
    path: &str,
) -> Result<crate::surface::ErrorFormatSpec, CompileError> {
    let renderings = match req(j, "renderings", path)? {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| {
                hh_hir::leaves::Text::from_json(v, &format!("{path}.renderings.{k}"))
                    .map(|t| (k.clone(), t))
                    .map_err(|e| schema_err(&format!("{path}.renderings.{k}"), format!("{e}")))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        _ => return Err(schema_err(path, "renderings must be an object")),
    };
    Ok(crate::surface::ErrorFormatSpec {
        renderings,
        distinguishability: crate::surface::Distinguishability::parse(&str_at(
            j,
            "distinguishability",
            path,
        )?)
        .ok_or_else(|| schema_err(path, "distinguishability: bad value"))?,
    })
}

fn result_spec_json(s: &crate::surface::ResultRenderSpec) -> Json {
    let (mode, extra): (Json, Vec<(&str, Json)>) = match &s.mode {
        crate::surface::RenderMode::Full => (Json::str("full"), Vec::new()),
        crate::surface::RenderMode::Truncate {
            max_lines,
            max_bytes,
            max_tokens,
            direction,
        } => {
            let mut p = vec![("direction", Json::str(direction.as_str()))];
            if let Some(v) = max_lines {
                p.push(("max_lines", Json::Int(*v as i64)));
            }
            if let Some(v) = max_bytes {
                p.push(("max_bytes", Json::Int(*v as i64)));
            }
            if let Some(v) = max_tokens {
                p.push(("max_tokens", Json::Int(*v as i64)));
            }
            (Json::str("truncate"), p)
        }
    };
    let mut mode_obj = vec![("kind", mode)];
    mode_obj.extend(extra);
    Json::obj([
        (
            "declared_loss",
            match &s.declared_loss {
                Some(f) => Json::Arr(f.iter().map(Json::str).collect()),
                None => Json::Null,
            },
        ),
        ("mode", Json::obj(mode_obj)),
        (
            "validator_reads",
            Json::Arr(s.validator_reads.iter().map(Json::str).collect()),
        ),
    ])
}

fn result_spec_from_json(
    j: &Json,
    path: &str,
) -> Result<crate::surface::ResultRenderSpec, CompileError> {
    let mode_j = req(j, "mode", path)?;
    let mode = match str_at(mode_j, "kind", &format!("{path}.mode"))?.as_str() {
        "full" => crate::surface::RenderMode::Full,
        "truncate" => crate::surface::RenderMode::Truncate {
            max_lines: mode_j
                .get("max_lines")
                .and_then(Json::as_int)
                .map(|i| i as u64),
            max_bytes: mode_j
                .get("max_bytes")
                .and_then(Json::as_int)
                .map(|i| i as u64),
            max_tokens: mode_j
                .get("max_tokens")
                .and_then(Json::as_int)
                .map(|i| i as u64),
            direction: mode_j
                .get("direction")
                .and_then(Json::as_str)
                .and_then(crate::surface::TruncateDirection::parse)
                .unwrap_or(crate::surface::TruncateDirection::Head),
        },
        other => return Err(schema_err(path, format!("result_render.mode: {other}"))),
    };
    let str_vec = |k: &str| -> Vec<String> {
        match j.get(k) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    };
    Ok(crate::surface::ResultRenderSpec {
        mode,
        declared_loss: match j.get("declared_loss") {
            Some(Json::Arr(_)) => Some(str_vec("declared_loss")),
            _ => None,
        },
        validator_reads: str_vec("validator_reads"),
    })
}

fn compiled_tool_json(t: &crate::lower::CompiledToolSurface) -> Json {
    let mut pairs = vec![
        ("binding", surface_binding_json(&t.binding)),
        ("description", Json::str(t.description.clone())),
        ("schema", t.schema.clone()),
    ];
    if let Some(e) = &t.error_format {
        pairs.push(("error_format", error_spec_json(e)));
    }
    if let Some(e) = &t.equivalence {
        pairs.push(("equivalence", evidence_json(e)));
    }
    if let Some(r) = &t.result_render {
        pairs.push(("result_render", result_spec_json(r)));
    }
    Json::obj(pairs)
}

fn compiled_tool_from_json(
    j: &Json,
    path: &str,
) -> Result<crate::lower::CompiledToolSurface, CompileError> {
    Ok(crate::lower::CompiledToolSurface {
        binding: surface_binding_from_json(req(j, "binding", path)?, &format!("{path}.binding"))?,
        schema: req(j, "schema", path)?.clone(),
        description: str_at(j, "description", path)?,
        error_format: match j.get("error_format") {
            Some(e) => Some(error_spec_from_json(e, &format!("{path}.error_format"))?),
            None => None,
        },
        result_render: match j.get("result_render") {
            Some(r) => Some(result_spec_from_json(r, &format!("{path}.result_render"))?),
            None => None,
        },
        equivalence: match j.get("equivalence") {
            Some(e) => Some(evidence_from_json(e, &format!("{path}.equivalence"))?),
            None => None,
        },
    })
}

fn section_json(s: &crate::lower::CompiledSection) -> Json {
    Json::obj([
        (
            "rule_ids",
            Json::Arr(s.rule_ids.iter().map(Json::str).collect()),
        ),
        ("section_id", Json::str(s.section_id.clone())),
        ("slots", Json::Arr(s.slots.iter().map(Json::str).collect())),
        (
            "source_node_ids",
            Json::Arr(s.source_node_ids.iter().map(Json::str).collect()),
        ),
        ("text", s.text.clone().map_or(Json::Null, Json::str)),
    ])
}

fn section_from_json(j: &Json, path: &str) -> Result<crate::lower::CompiledSection, CompileError> {
    let str_vec = |k: &str| -> Vec<String> {
        match j.get(k) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    };
    Ok(crate::lower::CompiledSection {
        section_id: str_at(j, "section_id", path)?,
        source_node_ids: str_vec("source_node_ids"),
        slots: str_vec("slots"),
        text: match j.get("text") {
            Some(Json::Str(t)) => Some(t.clone()),
            _ => None,
        },
        rule_ids: str_vec("rule_ids"),
    })
}

fn model_surface_json(m: &ModelSurfaceState) -> Json {
    match m {
        ModelSurfaceState::Deferred => Json::obj([("deferred", Json::str("stage_3"))]),
        ModelSurfaceState::Lowered(s) => Json::obj([
            (
                "dialects",
                Json::Obj(
                    s.dialects
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                        .collect(),
                ),
            ),
            ("interaction_mode", Json::str(s.interaction_mode.clone())),
            (
                "layout",
                Json::Arr(s.layout.iter().map(section_json).collect()),
            ),
            ("params", s.params.clone()),
            ("profile", Json::str(s.profile.clone())),
            (
                "tools",
                Json::Arr(s.tools.iter().map(compiled_tool_json).collect()),
            ),
            ("transcript_renderer", s.transcript_renderer.clone()),
        ]),
    }
}

fn model_surface_from_json(j: &Json, path: &str) -> Result<ModelSurfaceState, CompileError> {
    if j.get("deferred").and_then(Json::as_str) == Some("stage_3") {
        return Ok(ModelSurfaceState::Deferred);
    }
    Ok(ModelSurfaceState::Lowered(ModelSurface {
        profile: str_at(j, "profile", path)?,
        layout: match req(j, "layout", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| section_from_json(v, &format!("{path}.layout[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "layout must be an array")),
        },
        tools: match req(j, "tools", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| compiled_tool_from_json(v, &format!("{path}.tools[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "tools must be an array")),
        },
        interaction_mode: str_at(j, "interaction_mode", path)?,
        params: req(j, "params", path)?.clone(),
        transcript_renderer: req(j, "transcript_renderer", path)?.clone(),
        dialects: match req(j, "dialects", path)? {
            Json::Obj(m) => m
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect(),
            _ => BTreeMap::new(),
        },
    }))
}

/// The canonical JSON of a `CompiledBundle`.
pub fn bundle_to_json(b: &CompiledBundle) -> Json {
    Json::obj([
        ("bundle_id", Json::str(b.bundle_id.clone())),
        ("derivation_key", Json::str(b.derivation_key.clone())),
        (
            "diagnostics",
            Json::Arr(
                b.diagnostics
                    .iter()
                    .map(hh_assembly::diagnostic_json)
                    .collect(),
            ),
        ),
        (
            "equivalence_evidence",
            Json::Arr(b.equivalence_evidence.iter().map(evidence_json).collect()),
        ),
        ("lcd_report", lcd_json(&b.lcd_report)),
        (
            "loss_reports",
            Json::Arr(b.loss_reports.iter().map(loss_json).collect()),
        ),
        ("model_surface", model_surface_json(&b.model_surface)),
        (
            "opacity_report",
            opt_or_null(b.opacity_report.as_ref().map(|o| {
                Json::obj([
                    (
                        "by_class",
                        Json::Obj(
                            o.by_class
                                .iter()
                                .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                                .collect(),
                        ),
                    ),
                    ("opaque_ratio_num", Json::Int(o.opaque_ratio_num as i64)),
                    ("total", Json::Int(o.total as i64)),
                ])
            })),
        ),
        (
            "profile_chain",
            Json::Arr(b.profile_chain.iter().map(Json::str).collect()),
        ),
        (
            "profile_test_report_ref",
            opt_or_null(b.profile_test_report_ref.as_ref().map(Json::str)),
        ),
        ("fallback_used", Json::Bool(b.fallback_used)),
        ("runtime_plan", plan_to_json(&b.runtime_plan)),
        ("schema", Json::str("CompiledBundle/1")),
        (
            "target_artefacts",
            Json::Obj(
                b.target_artefacts
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        ),
        ("trace_map", trace_map_json(&b.trace_map)),
    ])
}

/// Parse a `CompiledBundle`.
pub fn bundle_from_json(j: &Json) -> Result<CompiledBundle, CompileError> {
    let path = "bundle";
    if str_at(j, "schema", path)? != "CompiledBundle/1" {
        return Err(schema_err(path, "schema must be CompiledBundle/1"));
    }
    Ok(CompiledBundle {
        bundle_id: str_at(j, "bundle_id", path)?,
        derivation_key: str_at(j, "derivation_key", path)?,
        runtime_plan: plan_from_json(
            req(j, "runtime_plan", path)?,
            &format!("{path}.runtime_plan"),
        )?,
        model_surface: model_surface_from_json(
            req(j, "model_surface", path)?,
            &format!("{path}.model_surface"),
        )?,
        target_artefacts: match req(j, "target_artefacts", path)? {
            Json::Obj(m) => m.clone(),
            _ => return Err(schema_err(path, "target_artefacts must be an object")),
        },
        trace_map: trace_map_from_json(req(j, "trace_map", path)?, &format!("{path}.trace_map"))?,
        lcd_report: lcd_from_json(req(j, "lcd_report", path)?, &format!("{path}.lcd_report"))?,
        loss_reports: match req(j, "loss_reports", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, l)| loss_from_json(l, &format!("{path}.loss_reports[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "loss_reports must be an array")),
        },
        opacity_report: match j.get("opacity_report") {
            Some(Json::Obj(o)) => Some(hh_assembly::OpacitySummary {
                by_class: match o.get("by_class") {
                    Some(Json::Obj(m)) => m
                        .iter()
                        .filter_map(|(k, v)| v.as_int().map(|i| (k.clone(), i as usize)))
                        .collect(),
                    _ => BTreeMap::new(),
                },
                total: o.get("total").and_then(Json::as_int).unwrap_or(0) as usize,
                opaque_ratio_num: o
                    .get("opaque_ratio_num")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as usize,
            }),
            _ => None,
        },
        equivalence_evidence: match req(j, "equivalence_evidence", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, e)| evidence_from_json(e, &format!("{path}.equivalence_evidence[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "equivalence_evidence must be an array")),
        },
        diagnostics: match req(j, "diagnostics", path)? {
            Json::Arr(items) => items
                .iter()
                .map(|d| {
                    hh_assembly::diagnostic_from_json(d, "bundle.diagnostics")
                        .map_err(|e| schema_err(path, format!("{e}")))
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "diagnostics must be an array")),
        },
        profile_chain: str_vec(j, "profile_chain", path)?,
        profile_test_report_ref: j
            .get("profile_test_report_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
        fallback_used: matches!(j.get("fallback_used"), Some(Json::Bool(true))),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// CompileInputs — the out-of-process seam's wire record (AC-CP-11)
// ─────────────────────────────────────────────────────────────────────────────

/// The canonical JSON of a `TargetSpec`.
pub fn target_spec_json(t: &TargetSpec) -> Json {
    Json::obj([
        ("content_hash", Json::str(t.content_hash.clone())),
        ("spec_version", Json::str(t.spec_version.clone())),
        ("target_id", Json::str(t.target_id.clone())),
    ])
}

/// Parse a `TargetSpec`.
pub fn target_spec_from_json(j: &Json, path: &str) -> Result<TargetSpec, CompileError> {
    Ok(TargetSpec {
        target_id: str_at(j, "target_id", path)?,
        spec_version: str_at(j, "spec_version", path)?,
        content_hash: str_at(j, "content_hash", path)?,
    })
}

/// The `CompileInputs` wire record (AC-CP-11): `{document, profile_refs,
/// fallback_profile?, profiles[], variants[], targets[], compile_for_expired}` — the
/// profile *records* and variant *records* the in-process views would serve, carried as
/// data (the binary has no registry).
pub struct WireCompileInputs {
    /// The sealed definition's canonical JSON.
    pub document: Json,
    /// The bound profile coordinates.
    pub profile_refs: Vec<String>,
    /// The explicit fallback coordinate.
    pub fallback_profile: Option<String>,
    /// The profile records the view serves (the bound view, carried).
    pub profiles: Vec<ModelProfile>,
    /// The variant records (as `hh_registry` record bodies).
    pub variants: Vec<(String, hh_registry::records::VariantRecord)>,
    /// The target specs.
    pub targets: Vec<TargetSpec>,
    /// The recorded intent flag.
    pub compile_for_expired: bool,
    /// The `ProfileTestReport` records the registry holds beside the carried
    /// profiles — the link gate (AC-R-2.3.3-13) reads them; a bound profile
    /// without a carried report is `profile_untested` out-of-process too.
    pub test_reports: Vec<crate::profile_test::ProfileTestReport>,
}

/// The canonical JSON of a `WireCompileInputs`.
pub fn compile_inputs_json(i: &WireCompileInputs) -> Json {
    let mut pairs = vec![
        ("compile_for_expired", Json::Bool(i.compile_for_expired)),
        ("document", i.document.clone()),
        (
            "profile_refs",
            Json::Arr(i.profile_refs.iter().map(Json::str).collect()),
        ),
        (
            "profiles",
            Json::Arr(i.profiles.iter().map(profile_to_json).collect()),
        ),
        (
            "targets",
            Json::Arr(i.targets.iter().map(target_spec_json).collect()),
        ),
        (
            "test_reports",
            Json::Arr(
                i.test_reports
                    .iter()
                    .map(crate::profile_test::test_report_json)
                    .collect(),
            ),
        ),
        (
            "variants",
            Json::Arr(
                i.variants
                    .iter()
                    .map(|(vid, v)| {
                        Json::obj([
                            (
                                "record",
                                hh_registry::schema::body_json(
                                    &hh_registry::records::RegistryRecord::Variant(v.clone()),
                                    false,
                                ),
                            ),
                            ("version_id", Json::str(vid.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ];
    if let Some(f) = &i.fallback_profile {
        pairs.push(("fallback_profile", Json::str(f.clone())));
    }
    Json::obj(pairs)
}

/// Parse a `WireCompileInputs`.
pub fn compile_inputs_from_json(j: &Json) -> Result<WireCompileInputs, CompileError> {
    let path = "inputs";
    let profiles = match req(j, "profiles", path)? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, p)| profile_from_json(p, &format!("{path}.profiles[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(schema_err(path, "profiles must be an array")),
    };
    let variants = match j.get("variants") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let p = format!("{path}.variants[{i}]");
                let vid = str_at(v, "version_id", &p)?;
                let rec = hh_registry::schema::record_from_json(
                    hh_registry::kinds::RecordKind::Variant,
                    req(v, "record", &p)?,
                )
                .map_err(|e| schema_err(&p, format!("{e}")))?;
                match rec {
                    hh_registry::records::RegistryRecord::Variant(vr) => Ok((vid, vr)),
                    _ => Err(schema_err(&p, "record is not a VariantRecord")),
                }
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        _ => Vec::new(),
    };
    Ok(WireCompileInputs {
        document: req(j, "document", path)?.clone(),
        profile_refs: str_vec(j, "profile_refs", path)?,
        fallback_profile: opt_str(j, "fallback_profile"),
        profiles,
        variants,
        targets: match req(j, "targets", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, t)| target_spec_from_json(t, &format!("{path}.targets[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(schema_err(path, "targets must be an array")),
        },
        compile_for_expired: matches!(j.get("compile_for_expired"), Some(Json::Bool(true))),
        test_reports: match j.get("test_reports") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(crate::profile_test::test_report_from_json)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| schema_err(path, "test_reports contains a malformed report"))?,
            Some(Json::Null) | None => Vec::new(),
            _ => return Err(schema_err(path, "test_reports must be an array")),
        },
    })
}
