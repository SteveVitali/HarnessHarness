//! The HIR/1 canonical encoding: `HirDocument ⇄ canonical JSON` (the WS-C encoding — sorted
//! keys, compact form, integer-only, SHA-256; `hh_wire::Json` is already canonical by
//! construction). This module is the **only** place the wire shape is defined — the schema
//! source of truth (CC1). [`crate::document::parse_document`] is the out-of-process entry.
//!
//! Two projections are produced here:
//! - the **canonical form** — the full record (`provenance`, `semantic`, `surface`, `ext`,
//!   `version`) — feeds `version_id` and the document encoding;
//! - the **semantic projection** — `{kind ∥ semantic ∥ refs-by-semantic_id}` with `Text`
//!   leaves contributing their content hash — feeds `semantic_id` (§3.1.2).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ontology::control::{ControlBoundary, DecisionPoint, Owner};
use hh_ontology::participant::{HostingMechanism, Observability};
use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, AuthorityClass, Derivation, DerivationKind,
    HumanRole, Origin, PersistenceScope, ProvenanceRecord, ReaderSet, TaintTag,
};
use hh_wire::json::Json;

use crate::document::{Edge, HirDocument, Node};
use crate::errors::HirError;
use crate::kinds::{
    ChargedTo, EdgeKind, EffectClass, EntityKind, JudgeProfile, PreconditionDomain, ToolEffects,
    ValidatorExecutable, ValidatorKind,
};
use crate::leaves::{CompiledPayload, Text};
use crate::records::*;
use crate::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref, RunRef};

// ─────────────────────────────────────────────────────────────────────────────
// Small decode helpers
// ─────────────────────────────────────────────────────────────────────────────

fn miss(path: &str, key: &str) -> HirError {
    HirError::SchemaViolation {
        detail: format!("{path}.{key} missing"),
    }
}

fn req<'a>(j: &'a Json, key: &str, path: &str) -> Result<&'a Json, HirError> {
    j.get(key).ok_or_else(|| miss(path, key))
}

fn req_str(j: &Json, key: &str, path: &str) -> Result<String, HirError> {
    req(j, key, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| HirError::SchemaViolation {
            detail: format!("{path}.{key} must be a string"),
        })
}

fn req_int(j: &Json, key: &str, path: &str) -> Result<u64, HirError> {
    match req(j, key, path)? {
        Json::Int(i) if *i >= 0 => Ok(*i as u64),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}.{key} must be a non-negative integer"),
        }),
    }
}

fn opt_str(j: &Json, key: &str) -> Option<String> {
    j.get(key).and_then(Json::as_str).map(str::to_string)
}

fn arr<'a>(j: &'a Json, key: &str, path: &str) -> Result<&'a [Json], HirError> {
    match req(j, key, path)? {
        Json::Arr(items) => Ok(items),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}.{key} must be an array"),
        }),
    }
}

fn str_arr(items: &[Json], path: &str) -> Result<Vec<String>, HirError> {
    items
        .iter()
        .enumerate()
        .map(|(i, s)| {
            s.as_str()
                .map(str::to_string)
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}[{i}] must be a string"),
                })
        })
        .collect()
}

fn str_set(items: &[Json], path: &str) -> Result<BTreeSet<String>, HirError> {
    Ok(str_arr(items, path)?.into_iter().collect())
}

fn bool_at(j: &Json, key: &str, path: &str) -> Result<bool, HirError> {
    match req(j, key, path)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}.{key} must be a bool"),
        }),
    }
}

fn tag_of<'a>(j: &'a Json, path: &str) -> Result<&'a str, HirError> {
    j.get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| HirError::SchemaViolation {
            detail: format!("{path}.kind missing"),
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// hh-provenance value (de)serialization — mirrors `ProvenanceRecord::to_json`
// ─────────────────────────────────────────────────────────────────────────────

fn origin_from_json(j: &Json, path: &str) -> Result<Origin, HirError> {
    let kind = tag_of(j, path)?;
    let s = |k: &str| req_str(j, k, path);
    match kind {
        "kernel" => Ok(Origin::kernel(&s("component_ref")?)),
        "human" => {
            let role = match s("role")?.as_str() {
                "author" => HumanRole::Author,
                "principal" => HumanRole::Principal,
                "reviewer" => HumanRole::Reviewer,
                other => {
                    return Err(HirError::UnknownKind {
                        kind: format!("human role {other}"),
                    })
                }
            };
            Ok(Origin::human(&s("author_ref")?, role))
        }
        "model" => Ok(Origin::model(
            &s("model_ref")?,
            &s("run_ref")?,
            &s("response_id")?,
        )),
        "tool" => {
            // `inner_source` is a `TaintTag` on the wire (`as_string` form).
            let inner = opt_str(j, "inner_source")
                .map(|inner| taint_parse(&inner).map(Box::new))
                .transpose()?;
            Ok(Origin::Tool {
                capability: s("capability")?,
                invocation_ref: s("invocation_ref")?,
                inner_source: inner,
            })
        }
        "evolution" => Ok(Origin::evolution(
            &s("candidate_id")?,
            &s("hypothesis_ref")?,
        )),
        "import" => Ok(Origin::import(&s("source_system")?, &s("mapping_version")?)),
        "migration" => Ok(Origin::Migration {
            from_dialect: s("from_dialect")?,
        }),
        "participant" => Ok(Origin::participant(
            &s("participant_ref")?,
            &s("hosting_mechanism")?,
        )),
        other => Err(HirError::UnknownKind {
            kind: format!("origin {other}"),
        }),
    }
}

fn scope_parse(s: &str) -> Result<PersistenceScope, HirError> {
    match s {
        "definition" => Ok(PersistenceScope::Definition),
        "user" => Ok(PersistenceScope::User),
        "project" => Ok(PersistenceScope::Project),
        "session" => Ok(PersistenceScope::Session),
        "run" => Ok(PersistenceScope::Run),
        "turn" => Ok(PersistenceScope::Turn),
        other => Err(HirError::UnknownKind {
            kind: format!("scope {other}"),
        }),
    }
}

fn taint_parse(s: &str) -> Result<TaintTag, HirError> {
    if let Some(rest) = s.strip_prefix("tool:") {
        let (cap, inner) = match rest.split_once('+') {
            Some((c, i)) => (c.to_string(), Some(i.to_string())),
            None => (rest.to_string(), None),
        };
        Ok(TaintTag::Tool {
            capability: cap,
            inner_source: inner,
        })
    } else if let Some(p) = s.strip_prefix("participant:") {
        Ok(TaintTag::Participant {
            participant: p.to_string(),
        })
    } else if let Some(sys) = s.strip_prefix("import:") {
        Ok(TaintTag::Import {
            source_system: sys.to_string(),
        })
    } else if let Some(e) = s.strip_prefix("extension:") {
        Ok(TaintTag::Extension {
            extension_id: e.to_string(),
        })
    } else {
        Err(HirError::UnknownKind {
            kind: format!("taint tag {s}"),
        })
    }
}

fn derivation_kind_parse(s: &str) -> Result<DerivationKind, HirError> {
    match s {
        "summary" => Ok(DerivationKind::Summary),
        "compaction" => Ok(DerivationKind::Compaction),
        "extraction" => Ok(DerivationKind::Extraction),
        "quotation" => Ok(DerivationKind::Quotation),
        "revision" => Ok(DerivationKind::Revision),
        "translation" => Ok(DerivationKind::Translation),
        "subagent_result" => Ok(DerivationKind::SubagentResult),
        "projection" => Ok(DerivationKind::Projection),
        other => Err(HirError::UnknownKind {
            kind: format!("derivation kind {other}"),
        }),
    }
}

fn attestation_from_json(j: &Json, path: &str) -> Result<Attestation, HirError> {
    let kind = match req_str(j, "kind", path)?.as_str() {
        "seal" => AttestationKind::Seal,
        "hash_chain" => AttestationKind::HashChain,
        "signature" => AttestationKind::Signature,
        "pin" => AttestationKind::Pin,
        other => {
            return Err(HirError::UnknownKind {
                kind: format!("attestation kind {other}"),
            })
        }
    };
    let anchor_j = req(j, "anchor", path)?;
    let anchor = if let Some(s) = anchor_j.get("signer").and_then(Json::as_str) {
        AttestationAnchor::Signer(s.to_string())
    } else if let Some(c) = anchor_j.get("chain").and_then(Json::as_str) {
        AttestationAnchor::Chain(c.to_string())
    } else {
        return Err(HirError::SchemaViolation {
            detail: format!("{path}.anchor must be signer|chain"),
        });
    };
    Ok(Attestation {
        kind,
        subject_hash: req_str(j, "subject_hash", path)?,
        anchor,
        verified_by: req_str(j, "verified_by", path)?,
        verified_at: req_int(j, "verified_at", path)?,
    })
}

/// Parse a `ProvenanceRecord` from its canonical form. `Derivation.deriver` is not carried
/// on the wire (the canonical form is `{kind, inputs, deterministic}` — `hh-provenance`'s
/// choice); the parser restores a kernel placeholder, which round-trips byte-identically.
pub(crate) fn provenance_from_json(j: &Json, path: &str) -> Result<ProvenanceRecord, HirError> {
    let origin = origin_from_json(req(j, "origin", path)?, &format!("{path}.origin"))?;
    let authority = AuthorityClass::parse(&req_str(j, "authority", path)?).ok_or_else(|| {
        HirError::UnknownKind {
            kind: format!("{path}.authority"),
        }
    })?;
    let scope = scope_parse(&req_str(j, "scope", path)?)?;
    let created_at = req_int(j, "created_at", path)?;
    let taint = match j.get("taint") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|t| {
                t.as_str()
                    .ok_or_else(|| HirError::SchemaViolation {
                        detail: format!("{path}.taint members must be strings"),
                    })
                    .and_then(taint_parse)
            })
            .collect::<Result<BTreeSet<_>, _>>()?,
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.taint must be an array"),
            })
        }
        None => BTreeSet::new(),
    };
    let readers = match j.get("readers") {
        Some(Json::Arr(items)) => {
            ReaderSet::Restricted(str_set(items, &format!("{path}.readers"))?)
        }
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.readers must be an array"),
            })
        }
        None => ReaderSet::Public,
    };
    let derived_from = match j.get("derived_from") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let dp = format!("{path}.derived_from[{i}]");
                Ok(Derivation {
                    kind: derivation_kind_parse(&req_str(d, "kind", &dp)?)?,
                    inputs: str_arr(arr(d, "inputs", &dp)?, &format!("{dp}.inputs"))?,
                    // The deriver is not on the wire (see fn doc); a kernel placeholder
                    // round-trips byte-identically.
                    deriver: Origin::kernel("hir:wire"),
                    deterministic: bool_at(d, "deterministic", &dp)?,
                })
            })
            .collect::<Result<Vec<_>, HirError>>()?,
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.derived_from must be an array"),
            })
        }
        None => Vec::new(),
    };
    let attestation = match j.get("attestation") {
        Some(a) => Some(attestation_from_json(a, &format!("{path}.attestation"))?),
        None => None,
    };
    Ok(ProvenanceRecord {
        origin,
        authority,
        taint,
        readers,
        scope,
        derived_from,
        created_at,
        attestation,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared record (de)serialization
// ─────────────────────────────────────────────────────────────────────────────

fn validity_json(v: &Validity) -> Json {
    let mut pairs = vec![("from", Json::Int(v.from as i64))];
    if let Some(u) = v.until {
        pairs.push(("until", Json::Int(u as i64)));
    }
    if let Some(c) = &v.condition {
        pairs.push(("condition", Json::str(c.clone())));
    }
    Json::obj(pairs)
}

fn validity_from_json(j: &Json, path: &str) -> Result<Validity, HirError> {
    Ok(Validity {
        from: req_int(j, "from", path)?,
        until: j
            .get("until")
            .map(|u| match u {
                Json::Int(i) if *i >= 0 => Ok(*i as u64),
                _ => Err(HirError::SchemaViolation {
                    detail: format!("{path}.until must be a non-negative integer"),
                }),
            })
            .transpose()?,
        condition: opt_str(j, "condition"),
    })
}

fn profile_ref_json(p: &ProfileRef) -> Json {
    Json::obj([
        ("profile_ref", Json::str(p.profile.clone())),
        ("pinned", Json::Bool(p.pinned)),
    ])
}

fn profile_ref_from_json(j: &Json, path: &str) -> Result<ProfileRef, HirError> {
    Ok(ProfileRef {
        profile: req_str(j, "profile_ref", path)?,
        pinned: bool_at(j, "pinned", path).unwrap_or(false),
    })
}

fn run_refs_json(rs: &[RunRef]) -> Json {
    Json::Arr(rs.iter().map(|r| Json::str(r.run.clone())).collect())
}

fn run_refs_from_json(j: &Json, path: &str) -> Result<Vec<RunRef>, HirError> {
    arr(j, "source_trajectories", path)
        .or_else(|_| match j {
            Json::Arr(items) => Ok(items.as_slice()),
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path} must be an array"),
            }),
        })?
        .iter()
        .enumerate()
        .map(|(i, s)| {
            s.as_str()
                .map(|r| RunRef { run: r.to_string() })
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}[{i}] must be a string"),
                })
        })
        .collect::<Result<_, HirError>>()
}

fn text_json(t: &Text, semantic: bool) -> Json {
    if semantic {
        t.semantic_json()
    } else {
        t.to_json()
    }
}

fn payload_json(p: &CompiledPayload, semantic: bool) -> Json {
    if semantic {
        p.semantic_json()
    } else {
        p.to_json()
    }
}

fn ref_json(r: &Ref, semantic: bool) -> Json {
    if semantic {
        r.semantic_json()
    } else {
        r.to_json()
    }
}

fn ref_vec_json(rs: &[Ref], semantic: bool) -> Json {
    Json::Arr(rs.iter().map(|r| ref_json(r, semantic)).collect())
}

fn effect_class_json(e: &EffectClass, semantic: bool) -> Json {
    if semantic {
        e.semantic_json()
    } else {
        e.to_json()
    }
}

fn effect_class_arr_json(es: &BTreeSet<EffectClass>, semantic: bool) -> Json {
    Json::Arr(es.iter().map(|e| effect_class_json(e, semantic)).collect())
}

fn goal_origin_parse(s: &str) -> Result<GoalOrigin, HirError> {
    match s {
        "human" => Ok(GoalOrigin::Human),
        "system" => Ok(GoalOrigin::System),
        "delegated" => Ok(GoalOrigin::Delegated),
        "scheduled" => Ok(GoalOrigin::Scheduled),
        other => Err(HirError::UnknownKind {
            kind: format!("goal origin {other}"),
        }),
    }
}

fn observation_source_parse(s: &str) -> Result<ObservationSource, HirError> {
    match s {
        "tool_result" => Ok(ObservationSource::ToolResult),
        "environment" => Ok(ObservationSource::Environment),
        "human" => Ok(ObservationSource::Human),
        "model_claim" => Ok(ObservationSource::ModelClaim),
        "validator" => Ok(ObservationSource::Validator),
        other => Err(HirError::UnknownKind {
            kind: format!("observation source {other}"),
        }),
    }
}

/// The canonical `AssumptionDebtRecord` encoding (CC7 — the schema source owns
/// both directions; `hh-registry` reuses it for `VariantRecord.conditioned_rules`).
pub fn debt_json(d: &AssumptionDebtRecord, semantic: bool) -> Json {
    Json::obj([
        ("rule_id", Json::str(d.rule_id.clone())),
        ("hypothesis", text_json(&d.hypothesis, semantic)),
        (
            "evidence_refs",
            Json::Arr(
                d.evidence_refs
                    .iter()
                    .map(|e| Json::str(e.clone()))
                    .collect(),
            ),
        ),
        ("owner", Json::str(d.owner.clone())),
        ("expiry_condition", Json::str(d.expiry_condition.clone())),
        ("removal_test_ref", Json::str(d.removal_test_ref.clone())),
        ("status", Json::str(d.status.name())),
    ])
}

/// Decode a canonical `AssumptionDebtRecord` (the [`debt_json`] direction).
pub fn debt_from_json(j: &Json, path: &str) -> Result<AssumptionDebtRecord, HirError> {
    let status = match req_str(j, "status", path)?.as_str() {
        "open" => DebtStatus::Open,
        "discharged" => DebtStatus::Discharged,
        "violated" => DebtStatus::Violated,
        other => {
            return Err(HirError::UnknownKind {
                kind: format!("debt status {other}"),
            })
        }
    };
    Ok(AssumptionDebtRecord {
        rule_id: req_str(j, "rule_id", path)?,
        hypothesis: Text::from_json(req(j, "hypothesis", path)?, &format!("{path}.hypothesis"))?,
        evidence_refs: str_arr(
            arr(j, "evidence_refs", path)?,
            &format!("{path}.evidence_refs"),
        )?,
        owner: req_str(j, "owner", path)?,
        expiry_condition: req_str(j, "expiry_condition", path)?,
        removal_test_ref: req_str(j, "removal_test_ref", path)?,
        status,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-kind semantic record (de)serialization
// ─────────────────────────────────────────────────────────────────────────────

fn step_json(s: &ProcedureStep, semantic: bool) -> Json {
    match s {
        ProcedureStep::Instruction(t) => Json::obj([
            ("kind", Json::str("instruction")),
            ("text", text_json(t, semantic)),
        ]),
        ProcedureStep::Invoke { tool, args } => Json::obj([
            ("kind", Json::str("invoke")),
            ("tool", ref_json(tool, semantic)),
            ("args", args.clone()),
        ]),
        ProcedureStep::Delegate {
            spec,
            budget,
            permission,
        } => Json::obj([
            ("kind", Json::str("delegate")),
            ("spec", spec.clone()),
            ("budget", ref_json(budget, semantic)),
            ("permission", ref_json(permission, semantic)),
        ]),
        ProcedureStep::Branch {
            condition,
            then_body,
            else_body,
        } => Json::obj([
            ("kind", Json::str("branch")),
            ("condition", condition.clone()),
            (
                "then_body",
                Json::Arr(then_body.iter().map(|s| step_json(s, semantic)).collect()),
            ),
            (
                "else_body",
                Json::Arr(else_body.iter().map(|s| step_json(s, semantic)).collect()),
            ),
        ]),
        ProcedureStep::Loop { bound, body } => Json::obj([
            ("kind", Json::str("loop")),
            ("bound", ref_json(bound, semantic)),
            (
                "body",
                Json::Arr(body.iter().map(|s| step_json(s, semantic)).collect()),
            ),
        ]),
        ProcedureStep::Verify { validator } => Json::obj([
            ("kind", Json::str("verify")),
            ("validator", ref_json(validator, semantic)),
        ]),
        ProcedureStep::Opaque(p) => Json::obj([
            ("kind", Json::str("opaque")),
            ("payload", payload_json(p, semantic)),
        ]),
    }
}

fn step_from_json(j: &Json, path: &str) -> Result<ProcedureStep, HirError> {
    match tag_of(j, path)? {
        "instruction" => Ok(ProcedureStep::Instruction(Text::from_json(
            req(j, "text", path)?,
            &format!("{path}.text"),
        )?)),
        "invoke" => Ok(ProcedureStep::Invoke {
            tool: Ref::from_json(req(j, "tool", path)?, &format!("{path}.tool"))?,
            args: req(j, "args", path)?.clone(),
        }),
        "delegate" => Ok(ProcedureStep::Delegate {
            spec: req(j, "spec", path)?.clone(),
            budget: Ref::from_json(req(j, "budget", path)?, &format!("{path}.budget"))?,
            permission: Ref::from_json(req(j, "permission", path)?, &format!("{path}.permission"))?,
        }),
        "branch" => Ok(ProcedureStep::Branch {
            condition: req(j, "condition", path)?.clone(),
            then_body: steps_from_json(req(j, "then_body", path)?, &format!("{path}.then_body"))?,
            else_body: steps_from_json(req(j, "else_body", path)?, &format!("{path}.else_body"))?,
        }),
        "loop" => Ok(ProcedureStep::Loop {
            bound: Ref::from_json(req(j, "bound", path)?, &format!("{path}.bound"))?,
            body: steps_from_json(req(j, "body", path)?, &format!("{path}.body"))?,
        }),
        "verify" => Ok(ProcedureStep::Verify {
            validator: Ref::from_json(req(j, "validator", path)?, &format!("{path}.validator"))?,
        }),
        "opaque" => Ok(ProcedureStep::Opaque(CompiledPayload::from_json(
            req(j, "payload", path)?,
            &format!("{path}.payload"),
        )?)),
        other => Err(HirError::UnknownKind {
            kind: format!("procedure step {other}"),
        }),
    }
}

fn steps_from_json(j: &Json, path: &str) -> Result<Vec<ProcedureStep>, HirError> {
    match j {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, s)| step_from_json(s, &format!("{path}[{i}]")))
            .collect(),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path} must be an array of steps"),
        }),
    }
}

fn tool_effects_json(e: &ToolEffects, semantic: bool) -> Json {
    match e {
        ToolEffects::Pure => Json::str("pure"),
        ToolEffects::Declared(set) => {
            Json::obj([("declared", effect_class_arr_json(set, semantic))])
        }
    }
}

fn tool_effects_from_json(j: &Json, path: &str) -> Result<ToolEffects, HirError> {
    match j {
        Json::Str(s) if s == "pure" => Ok(ToolEffects::Pure),
        Json::Obj(_) => match req(j, "declared", path)? {
            Json::Arr(items) => Ok(ToolEffects::Declared(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, e)| EffectClass::from_json(e, &format!("{path}.declared[{i}]")))
                    .collect::<Result<_, _>>()?,
            )),
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}.declared must be an array"),
            }),
        },
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}.effects must be \"pure\" or {{declared:[...]}}"),
        }),
    }
}

fn resources_json(r: &Resources) -> Json {
    match r {
        Resources::Declared(keys) => Json::obj([(
            "declared",
            Json::Arr(keys.iter().map(|k| Json::str(k.clone())).collect()),
        )]),
        Resources::NoneDeclared => Json::obj([("state", Json::str("none"))]),
        Resources::Unknown => Json::obj([("state", Json::str("unknown"))]),
    }
}

fn resources_from_json(j: &Json, path: &str) -> Result<Resources, HirError> {
    if let Some(d) = j.get("declared") {
        return match d {
            Json::Arr(items) => Ok(Resources::Declared(str_set(
                items,
                &format!("{path}.declared"),
            )?)),
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}.declared must be an array"),
            }),
        };
    }
    match j.get("state").and_then(Json::as_str) {
        Some("none") => Ok(Resources::NoneDeclared),
        Some("unknown") => Ok(Resources::Unknown),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}: resources must be declared|none|unknown"),
        }),
    }
}

/// The canonical `Grant` JSON (`semantic = true` projects refs inside
/// `reversibility` by semantic_id only — the semantic-projection form).
/// Exposed for the monitor's `security.permission.granted` payloads (S1.11 —
/// the grant record is the one vocabulary; CC1).
pub fn grant_json(g: &Grant, semantic: bool) -> Json {
    let mut c = vec![];
    if let Some(b) = &g.constraints.budget {
        c.push(("budget", b.clone()));
    }
    if let Some(t) = g.constraints.time {
        c.push(("time", Json::Int(t as i64)));
    }
    if let Some(n) = g.constraints.count {
        c.push(("count", Json::Int(n as i64)));
    }
    Json::obj([
        ("effect", effect_class_json(&g.effect, semantic)),
        ("scope", Json::str(g.scope.clone())),
        ("constraints", Json::obj(c)),
        ("delegable", Json::Bool(g.delegable)),
    ])
}

/// Parse a canonical `Grant` — `path` prefixes `SchemaViolation` details.
/// Exposed for the monitor's `granted`-event fold (S1.11).
pub fn grant_from_json(j: &Json, path: &str) -> Result<Grant, HirError> {
    let cj = req(j, "constraints", path)?;
    Ok(Grant {
        effect: EffectClass::from_json(req(j, "effect", path)?, &format!("{path}.effect"))?,
        scope: req_str(j, "scope", path)?,
        constraints: GrantConstraints {
            budget: cj.get("budget").cloned(),
            time: match cj.get("time") {
                Some(Json::Int(i)) if *i >= 0 => Some(*i as u64),
                None => None,
                _ => {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{path}.constraints.time must be a non-negative int"),
                    })
                }
            },
            count: match cj.get("count") {
                Some(Json::Int(i)) if *i >= 0 => Some(*i as u64),
                None => None,
                _ => {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{path}.constraints.count must be a non-negative int"),
                    })
                }
            },
        },
        delegable: bool_at(j, "delegable", path)?,
    })
}

fn validator_kind_json(k: &ValidatorKind, semantic: bool) -> Json {
    match k {
        ValidatorKind::Executable(ValidatorExecutable::Payload(p)) => Json::obj([(
            "executable",
            Json::obj([("payload", payload_json(p, semantic))]),
        )]),
        ValidatorKind::Executable(ValidatorExecutable::Invoke(r)) => {
            Json::obj([("executable", Json::obj([("invoke", ref_json(r, semantic))]))])
        }
        ValidatorKind::Schema => Json::str("schema"),
        ValidatorKind::Predicate => Json::str("predicate"),
        ValidatorKind::Judge(j) => Json::obj([(
            "judge",
            Json::obj({
                let mut v = vec![
                    ("rubric", text_json(&j.rubric, semantic)),
                    ("profile", profile_ref_json(&j.profile)),
                    ("charged_to", Json::str(j.charged_to.name())),
                ];
                if let Some(c) = &j.calibration_ref {
                    v.push(("calibration_ref", Json::str(c.clone())));
                }
                v
            }),
        )]),
        ValidatorKind::Human => Json::str("human"),
    }
}

fn validator_kind_from_json(j: &Json, path: &str) -> Result<ValidatorKind, HirError> {
    match j {
        Json::Str(s) => match s.as_str() {
            "schema" => Ok(ValidatorKind::Schema),
            "predicate" => Ok(ValidatorKind::Predicate),
            "human" => Ok(ValidatorKind::Human),
            other => Err(HirError::UnknownKind {
                kind: format!("validator kind {other}"),
            }),
        },
        Json::Obj(_) => {
            if let Some(e) = j.get("executable") {
                if let Some(p) = e.get("payload") {
                    return Ok(ValidatorKind::Executable(ValidatorExecutable::Payload(
                        Box::new(CompiledPayload::from_json(
                            p,
                            &format!("{path}.executable.payload"),
                        )?),
                    )));
                }
                if let Some(r) = e.get("invoke") {
                    return Ok(ValidatorKind::Executable(ValidatorExecutable::Invoke(
                        Ref::from_json(r, &format!("{path}.executable.invoke"))?,
                    )));
                }
                return Err(HirError::SchemaViolation {
                    detail: format!("{path}.executable must be payload|invoke"),
                });
            }
            if let Some(g) = j.get("judge") {
                let charged_to = match req_str(g, "charged_to", &format!("{path}.judge"))?.as_str()
                {
                    "subject" => ChargedTo::Subject,
                    "instrument" => ChargedTo::Instrument,
                    other => {
                        return Err(HirError::UnknownKind {
                            kind: format!("charged_to {other}"),
                        })
                    }
                };
                return Ok(ValidatorKind::Judge(Box::new(JudgeProfile {
                    rubric: Text::from_json(
                        req(g, "rubric", &format!("{path}.judge"))?,
                        &format!("{path}.judge.rubric"),
                    )?,
                    profile: profile_ref_from_json(
                        req(g, "profile", &format!("{path}.judge"))?,
                        &format!("{path}.judge.profile"),
                    )?,
                    calibration_ref: opt_str(g, "calibration_ref"),
                    charged_to,
                })));
            }
            Err(HirError::SchemaViolation {
                detail: format!(
                    "{path}: validator kind must be executable|judge|schema|predicate|human"
                ),
            })
        }
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path}: bad validator kind"),
        }),
    }
}

fn decision_point_name(p: DecisionPoint) -> &'static str {
    match p {
        DecisionPoint::Plan => "plan",
        DecisionPoint::Act => "act",
        DecisionPoint::Retrieve => "retrieve",
        DecisionPoint::Compact => "compact",
        DecisionPoint::Verify => "verify",
        DecisionPoint::Delegate => "delegate",
        DecisionPoint::Authorize => "authorize",
        DecisionPoint::Retry => "retry",
        DecisionPoint::Stop => "stop",
        DecisionPoint::Escalate => "escalate",
    }
}

fn decision_point_parse(s: &str) -> Result<DecisionPoint, HirError> {
    DecisionPoint::ALL
        .iter()
        .copied()
        .find(|p| decision_point_name(*p) == s)
        .ok_or_else(|| HirError::UnknownKind {
            kind: format!("decision point {s}"),
        })
}

fn owner_name(o: Owner) -> &'static str {
    match o {
        Owner::Code => "code",
        Owner::Model => "model",
        Owner::Human => "human",
    }
}

fn owner_parse(s: &str) -> Result<Owner, HirError> {
    match s {
        "code" => Ok(Owner::Code),
        "model" => Ok(Owner::Model),
        "human" => Ok(Owner::Human),
        other => Err(HirError::UnknownKind {
            kind: format!("owner {other}"),
        }),
    }
}

pub fn boundary_json(b: &ControlBoundary) -> Json {
    Json::obj([
        (
            "assignments",
            Json::Obj(
                b.assignments
                    .iter()
                    .map(|(p, o)| {
                        (
                            decision_point_name(*p).to_string(),
                            Json::str(owner_name(*o)),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "guards",
            Json::Obj(
                b.guards
                    .iter()
                    .map(|(p, g)| (decision_point_name(*p).to_string(), Json::str(g.clone())))
                    .collect(),
            ),
        ),
    ])
}

pub fn boundary_from_json(j: &Json, path: &str) -> Result<ControlBoundary, HirError> {
    let mut b = ControlBoundary::default();
    if let Some(Json::Obj(m)) = j.get("assignments") {
        for (k, v) in m {
            let owner = v
                .as_str()
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}.assignments.{k} must be a string"),
                })
                .and_then(owner_parse)?;
            b.assignments.insert(decision_point_parse(k)?, owner);
        }
    }
    if let Some(Json::Obj(m)) = j.get("guards") {
        for (k, v) in m {
            let g = v.as_str().ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.guards.{k} must be a string"),
            })?;
            b.guards.insert(decision_point_parse(k)?, g.to_string());
        }
    }
    Ok(b)
}

/// The wire spelling of `hh_ontology::participant::HostingMechanism` (underscore form —
/// ADR-0164 D7). `None` serializes as `none`; a hosted body never carries it (the parse
/// path refuses it; CF-351).
fn hosting_mechanism_name(m: hh_ontology::participant::HostingMechanism) -> &'static str {
    use hh_ontology::participant::HostingMechanism::*;
    match m {
        None => "none",
        SessionAbi => "session_abi",
        ModelBoundaryIntercept => "model_boundary_intercept",
        ContainerInstalled => "container_installed",
    }
}

fn observability_name(o: Observability) -> &'static str {
    match o {
        Observability::Events => "events",
        Observability::ModelIo => "model_io",
        Observability::EndState => "end_state",
        Observability::Ledger => "ledger",
    }
}

fn observability_parse(s: &str) -> Result<Observability, HirError> {
    [
        Observability::Events,
        Observability::ModelIo,
        Observability::EndState,
        Observability::Ledger,
    ]
    .iter()
    .copied()
    .find(|o| observability_name(*o) == s)
    .ok_or_else(|| HirError::UnknownKind {
        kind: format!("observability {s}"),
    })
}

fn capability_state_parse(s: &str) -> Result<CapabilityState, HirError> {
    match s {
        "supported" => Ok(CapabilityState::Supported),
        "unsupported" => Ok(CapabilityState::Unsupported),
        "unknown" => Ok(CapabilityState::Unknown),
        other => Err(HirError::UnknownKind {
            kind: format!("capability state {other}"),
        }),
    }
}

pub(crate) fn cap_decl_json(c: &CapabilityDeclarationRecord) -> Json {
    Json::obj([
        ("streaming", Json::str(c.streaming.name())),
        ("interrupt", Json::str(c.interrupt.name())),
        ("steer", Json::str(c.steer.name())),
        ("live_queue", Json::str(c.live_queue.name())),
        ("resume", Json::str(c.resume.name())),
        ("fork", Json::str(c.fork.name())),
        ("compaction", Json::str(c.compaction.name())),
        ("images", Json::str(c.images.name())),
        ("subagents", Json::str(c.subagents.name())),
        ("permission_surface", Json::str(c.permission_surface.name())),
        (
            "instruction_delivery",
            Json::str(c.instruction_delivery.name()),
        ),
        ("model_family", Json::str(c.model_family.name())),
        ("effort_vocabulary", Json::str(c.effort_vocabulary.name())),
        ("trajectory_export", Json::str(c.trajectory_export.name())),
        ("native_config", Json::str(c.native_config.name())),
    ])
}

fn cap_decl_from_json(j: &Json, path: &str) -> Result<CapabilityDeclarationRecord, HirError> {
    let s = |k: &str| capability_state_parse(&req_str(j, k, path)?);
    Ok(CapabilityDeclarationRecord {
        streaming: s("streaming")?,
        interrupt: s("interrupt")?,
        steer: s("steer")?,
        live_queue: s("live_queue")?,
        resume: s("resume")?,
        fork: s("fork")?,
        compaction: s("compaction")?,
        images: s("images")?,
        subagents: s("subagents")?,
        permission_surface: s("permission_surface")?,
        instruction_delivery: s("instruction_delivery")?,
        model_family: s("model_family")?,
        effort_vocabulary: s("effort_vocabulary")?,
        trajectory_export: s("trajectory_export")?,
        native_config: s("native_config")?,
    })
}

/// The canonical JSON of one `SlotBinding` (`semantic = true` drops the pin — the
/// variant contributes by name, refs-by-semantic_id §3.1.2).
pub fn slot_binding_json(b: &SlotBinding, semantic: bool) -> Json {
    // `variant` contributes by name in the semantic projection (the pin is a version
    // coordinate — refs-by-semantic_id, §3.1.2); `params`/`enabled` are semantic (§3.3.2:
    // a disabled binding still counts toward identity); `locality` is a hint — semantic too
    // (desired state), carried as declared.
    let mut v = vec![
        (
            "variant",
            if semantic {
                b.variant.semantic_json()
            } else {
                b.variant.to_json()
            },
        ),
        ("params", Json::Obj(b.params.clone())),
        ("enabled", Json::Bool(b.enabled)),
    ];
    if let Some(l) = b.locality {
        v.push(("locality", Json::str(l.name())));
    }
    Json::obj(v)
}

/// The canonical JSON of a `slots` map (`{name → {one|many}}`).
pub fn slots_json(slots: &BTreeMap<String, SlotBindings>, semantic: bool) -> Json {
    Json::Obj(
        slots
            .iter()
            .map(|(name, bs)| {
                let v = match bs {
                    SlotBindings::One(b) => Json::obj([("one", slot_binding_json(b, semantic))]),
                    SlotBindings::Many(many) => Json::obj([(
                        "many",
                        Json::Arr(
                            many.iter()
                                .map(|b| slot_binding_json(b, semantic))
                                .collect(),
                        ),
                    )]),
                };
                (name.clone(), v)
            })
            .collect(),
    )
}

/// Parse one `SlotBinding` (`{variant, params?, enabled?, locality?}`) — the §3.3.2
/// record shape shared by `native.slots` and `assembly.slots` (CC7: one decoder).
pub fn slot_binding_from_json(b: &Json, p: &str) -> Result<SlotBinding, HirError> {
    let m = match b {
        Json::Obj(m) => m,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: format!("{p} must be an object"),
            })
        }
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "variant" | "params" | "enabled" | "locality") {
            return Err(HirError::SchemaViolation {
                detail: format!("{p}.{k}: unknown SlotBinding member"),
            });
        }
    }
    let params = match b.get("params") {
        None => BTreeMap::new(),
        Some(Json::Obj(pm)) => pm.clone(),
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{p}.params must be an object"),
            })
        }
    };
    let enabled = match b.get("enabled") {
        None => true,
        Some(Json::Bool(e)) => *e,
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{p}.enabled must be a boolean"),
            })
        }
    };
    let locality = match b.get("locality") {
        None => None,
        Some(Json::Str(s)) => Some(Locality::parse(s).ok_or_else(|| HirError::UnknownKind {
            kind: format!("locality:{s}"),
        })?),
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{p}.locality must be a string"),
            })
        }
    };
    Ok(SlotBinding {
        variant: ComponentVariantRef::from_json(req(b, "variant", p)?, &format!("{p}.variant"))?,
        params,
        enabled,
        locality,
    })
}

/// Parse a `slots` map — the §3.3.2 slot grammar shared by `native.slots` and
/// `assembly.slots` (CC7: one decoder).
pub fn slots_from_json(j: &Json, path: &str) -> Result<BTreeMap<String, SlotBindings>, HirError> {
    let m = match j {
        Json::Obj(m) => m,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path} must be an object"),
            })
        }
    };
    let mut out = BTreeMap::new();
    for (name, v) in m {
        if let Some(one) = v.get("one") {
            out.insert(
                name.clone(),
                SlotBindings::One(slot_binding_from_json(one, &format!("{path}.{name}.one"))?),
            );
        } else if let Some(Json::Arr(items)) = v.get("many") {
            let many = items
                .iter()
                .enumerate()
                .map(|(i, b)| slot_binding_from_json(b, &format!("{path}.{name}.many[{i}]")))
                .collect::<Result<Vec<_>, _>>()?;
            out.insert(name.clone(), SlotBindings::Many(many));
        } else {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.{name} must be one|many"),
            });
        }
    }
    Ok(out)
}

fn supplies_json(s: &Supplies, semantic: bool) -> Json {
    Json::obj([
        ("context", ref_vec_json(&s.context, semantic)),
        ("tools", ref_vec_json(&s.tools, semantic)),
        ("procedures", ref_vec_json(&s.procedures, semantic)),
    ])
}

fn supplies_from_json(j: &Json, path: &str) -> Result<Supplies, HirError> {
    let refs = |k: &str| -> Result<Vec<Ref>, HirError> {
        crate::refs::refs_from_json(
            j.get(k).unwrap_or(&Json::Arr(vec![])),
            &format!("{path}.{k}"),
        )
    };
    Ok(Supplies {
        context: refs("context")?,
        tools: refs("tools")?,
        procedures: refs("procedures")?,
    })
}

fn action_json(a: &RuleAction, semantic: bool) -> Json {
    match a {
        RuleAction::InsertContextItem(r) => {
            Json::obj([("insert_context_item", ref_json(r, semantic))])
        }
        RuleAction::RestrictToolSet(rs) => {
            Json::obj([("restrict_tool_set", ref_vec_json(rs, semantic))])
        }
        RuleAction::SetCompactionPolicy(p) => Json::obj([("set_compaction_policy", p.clone())]),
        RuleAction::SetRetryStopPolicy(p) => Json::obj([("set_retry_stop_policy", p.clone())]),
        RuleAction::RequireValidator(r) => {
            Json::obj([("require_validator", ref_json(r, semantic))])
        }
        RuleAction::RequestApproval(r) => Json::obj([("request_approval", r.clone())]),
        RuleAction::FlowPolicy(p) => Json::obj([("flow_policy", p.clone())]),
        RuleAction::Declassify(rs) => Json::obj([(
            "declassify",
            Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect()),
        )]),
        RuleAction::Sanitize {
            sanitizer_ref,
            param,
        } => Json::obj([(
            "sanitize",
            Json::obj([
                ("sanitizer_ref", Json::str(sanitizer_ref.clone())),
                ("param", param.clone()),
            ]),
        )]),
    }
}

fn action_from_json(j: &Json, path: &str) -> Result<RuleAction, HirError> {
    if let Some(r) = j.get("insert_context_item") {
        return Ok(RuleAction::InsertContextItem(Ref::from_json(
            r,
            &format!("{path}.insert_context_item"),
        )?));
    }
    if let Some(rs) = j.get("restrict_tool_set") {
        return Ok(RuleAction::RestrictToolSet(crate::refs::refs_from_json(
            rs,
            &format!("{path}.restrict_tool_set"),
        )?));
    }
    if let Some(p) = j.get("set_compaction_policy") {
        return Ok(RuleAction::SetCompactionPolicy(p.clone()));
    }
    if let Some(p) = j.get("set_retry_stop_policy") {
        return Ok(RuleAction::SetRetryStopPolicy(p.clone()));
    }
    if let Some(r) = j.get("require_validator") {
        return Ok(RuleAction::RequireValidator(Ref::from_json(
            r,
            &format!("{path}.require_validator"),
        )?));
    }
    if let Some(r) = j.get("request_approval") {
        return Ok(RuleAction::RequestApproval(r.clone()));
    }
    if let Some(p) = j.get("flow_policy") {
        return Ok(RuleAction::FlowPolicy(p.clone()));
    }
    if let Some(rs) = j.get("declassify") {
        return match rs {
            Json::Arr(items) => Ok(RuleAction::Declassify(str_arr(
                items,
                &format!("{path}.declassify"),
            )?)),
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}.declassify must be an array"),
            }),
        };
    }
    if let Some(s) = j.get("sanitize") {
        return Ok(RuleAction::Sanitize {
            sanitizer_ref: req_str(s, "sanitizer_ref", &format!("{path}.sanitize"))?,
            param: s.get("param").cloned().unwrap_or(Json::Null),
        });
    }
    Err(HirError::SchemaViolation {
        detail: format!("{path}: unknown rule action"),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// KindRecord serialization (the dispatch table)
// ─────────────────────────────────────────────────────────────────────────────

/// Serialize a `KindRecord`'s semantic member. `semantic = true` produces the
/// semantic-projection form (refs by `semantic_id`, leaves by content hash — §3.1.2).
pub(crate) fn semantic_record_json(rec: &KindRecord, semantic: bool) -> Json {
    match rec {
        KindRecord::Goal(g) => Json::obj({
            let mut v = vec![
                ("statement", text_json(&g.statement, semantic)),
                (
                    "success_criteria",
                    ref_vec_json(&g.success_criteria, semantic),
                ),
                ("budget", ref_json(&g.budget, semantic)),
                ("origin", Json::str(g.origin.name())),
            ];
            if let Some(r) = &g.unverifiable_reason {
                v.push(("unverifiable_reason", text_json(r, semantic)));
            }
            if let Some(p) = &g.parent {
                v.push(("parent", ref_json(p, semantic)));
            }
            v
        }),
        KindRecord::Observation(o) => Json::obj({
            let content = match &o.content {
                ObservationContent::Text(t) => Json::obj([("text", text_json(t, semantic))]),
                ObservationContent::Structured(s) => Json::obj([("structured", s.clone())]),
                ObservationContent::Artifact(r) => Json::obj([("artifact", ref_json(r, semantic))]),
            };
            let mut v = vec![
                ("source", Json::str(o.source.name())),
                ("producer", Json::str(o.producer.clone())),
                ("content", content),
                ("authority", Json::str(o.authority.as_str())),
                (
                    "taint",
                    Json::Arr(o.taint.iter().map(|t| Json::str(t.as_string())).collect()),
                ),
            ];
            if let Some(r) = &o.risk_assessment {
                v.push(("risk_assessment", r.clone()));
            }
            v
        }),
        KindRecord::ContextItem(c) => Json::obj({
            let mut v = vec![
                ("payload", ref_json(&c.payload, semantic)),
                ("authority", Json::str(c.authority.as_str())),
                ("validity", validity_json(&c.validity)),
                ("priority", Json::Int(c.priority)),
                ("delivery_id", Json::str(c.delivery_id.clone())),
                ("activation_observable", c.activation_observable.clone()),
            ];
            if let Some(p) = &c.placement_policy {
                v.push(("placement_policy", ref_json(p, semantic)));
            }
            v
        }),
        KindRecord::Memory(m) => Json::obj([
            ("content", text_json(&m.content, semantic)),
            ("validity", validity_json(&m.validity)),
            ("authority", Json::str(m.authority.as_str())),
            ("confidence", m.confidence.clone()),
            ("source_trajectories", run_refs_json(&m.source_trajectories)),
            ("scope", Json::str(m.scope.as_str())),
        ]),
        KindRecord::Procedure(p) => Json::obj([
            ("preconditions", p.preconditions.clone()),
            (
                "steps",
                Json::Arr(p.steps.iter().map(|s| step_json(s, semantic)).collect()),
            ),
            ("expected_evidence", p.expected_evidence.clone()),
            (
                "allowed_capabilities",
                ref_vec_json(&p.allowed_capabilities, semantic),
            ),
            ("failure_handlers", p.failure_handlers.clone()),
        ]),
        KindRecord::ToolCapability(t) => Json::obj({
            let mut v = vec![
                ("purpose", text_json(&t.purpose, semantic)),
                ("input_schema", t.input_schema.clone()),
                ("effects", tool_effects_json(&t.effects, semantic)),
                (
                    "preconditions",
                    Json::Arr(
                        t.preconditions
                            .iter()
                            .map(|p| Json::str(p.name()))
                            .collect(),
                    ),
                ),
                ("resources", resources_json(&t.resources)),
                ("observation_contract", t.observation_contract.clone()),
                ("execution_requirement", t.execution_requirement.clone()),
                ("source", t.source.clone()),
                ("exposure_hint", t.exposure_hint.clone()),
                ("postconditions", ref_vec_json(&t.postconditions, semantic)),
            ];
            if let Some(o) = &t.output_schema {
                v.push(("output_schema", o.clone()));
            }
            match &t.scope_bindings {
                ScopeBindings::Bindings(b) => v.push(("scope_bindings", b.clone())),
                ScopeBindings::Unknown => v.push(("scope_bindings_unknown", Json::Bool(true))),
            }
            if let Some(c) = &t.cost_model {
                v.push(("cost_model", c.clone()));
            }
            if let Some(f) = &t.flow_contract {
                v.push(("flow_contract", f.clone()));
            }
            v
        }),
        KindRecord::Permission(p) => Json::obj({
            let mut v = vec![
                ("holder", ref_json(&p.holder, semantic)),
                (
                    "grants",
                    Json::Arr(p.grants.iter().map(|g| grant_json(g, semantic)).collect()),
                ),
                (
                    "issuer",
                    Json::obj([
                        ("authority", Json::str(p.issuer.authority.as_str())),
                        ("reference", Json::str(p.issuer.reference.clone())),
                    ]),
                ),
                ("validity", validity_json(&p.validity)),
            ];
            if let Some(r) = &p.revocation {
                v.push(("revocation", r.clone()));
            }
            v
        }),
        KindRecord::Effect(e) => Json::obj({
            let mut v = vec![
                ("run_id", Json::str(e.run_id.clone())),
                ("turn_id", Json::str(e.turn_id.clone())),
                ("model_call_id", Json::str(e.model_call_id.clone())),
                ("tool_call_id", Json::str(e.tool_call_id.clone())),
                ("declared", effect_class_json(&e.declared, semantic)),
                (
                    "handle_ids",
                    Json::Arr(e.handle_ids.iter().map(|h| Json::str(h.clone())).collect()),
                ),
            ];
            if let Some(r) = &e.effective_risk_class {
                v.push(("effective_risk_class", Json::str(r.clone())));
            }
            v
        }),
        KindRecord::Artifact(a) => Json::obj([
            ("content_hash", Json::str(a.content_hash.clone())),
            ("media_type", Json::str(a.media_type.clone())),
            ("size", Json::Int(a.size as i64)),
            ("storage_ref", Json::str(a.storage_ref.clone())),
            ("produced_by", ref_json(&a.produced_by, semantic)),
        ]),
        KindRecord::Validator(vr) => Json::obj({
            let mut v = vec![
                ("kind", validator_kind_json(&vr.kind, semantic)),
                ("inputs", ref_vec_json(&vr.inputs, semantic)),
                ("verdict_type", Json::str(vr.verdict_type.clone())),
                ("deterministic", Json::Bool(vr.deterministic)),
                ("cost", vr.cost.clone()),
                ("evidence_out", vr.evidence_out.clone()),
            ];
            if let Some(d) = &vr.assumption_debt {
                v.push(("assumption_debt", debt_json(d, semantic)));
            }
            v
        }),
        KindRecord::AgentProcess(a) => Json::obj([(
            "body",
            match &a.body {
                AgentProcessBody::Native(n) => Json::obj([(
                    "native",
                    Json::obj([
                        ("harness_def", ref_json(&n.harness_def, semantic)),
                        ("profile", profile_ref_json(&n.profile)),
                        ("slots", slots_json(&n.slots, semantic)),
                        ("control_boundary", boundary_json(&n.control_boundary)),
                        ("budget", ref_json(&n.budget, semantic)),
                        ("permissions", ref_json(&n.permissions, semantic)),
                        (
                            "environment",
                            Json::obj([(
                                "environment",
                                Json::str(n.environment.environment.clone()),
                            )]),
                        ),
                    ]),
                )]),
                AgentProcessBody::Hosted(h) => Json::obj([(
                    "hosted",
                    Json::obj({
                        let mut v = vec![
                            ("participant_ref", Json::str(h.participant_ref.clone())),
                            (
                                "declared_capabilities",
                                cap_decl_json(&h.declared_capabilities),
                            ),
                            (
                                "observability_levels",
                                Json::Arr(
                                    h.observability_levels
                                        .iter()
                                        .map(|o| Json::str(observability_name(*o)))
                                        .collect(),
                                ),
                            ),
                            (
                                "participant_version",
                                Json::str(h.participant_version.clone()),
                            ),
                            (
                                "hosting_mechanism",
                                Json::str(hosting_mechanism_name(h.hosting_mechanism)),
                            ),
                            ("supplies", supplies_json(&h.supplies, semantic)),
                            ("budget", ref_json(&h.budget, semantic)),
                            ("permissions", ref_json(&h.permissions, semantic)),
                        ];
                        if let Some(vi) = &h.version_identity {
                            v.push(("version_identity", Json::str(vi.clone())));
                        }
                        if let Some(p) = &h.params {
                            v.push(("params", p.clone()));
                        }
                        v
                    }),
                )]),
            },
        )]),
        KindRecord::Budget(b) => Json::obj({
            let mut v = vec![
                (
                    "dimensions",
                    Json::Obj(
                        b.dimensions
                            .iter()
                            .map(|(d, bound)| {
                                (
                                    d.clone(),
                                    Json::obj({
                                        let mut p = vec![];
                                        if let Some(h) = bound.hard {
                                            p.push(("hard", Json::Int(h as i64)));
                                        }
                                        if let Some(s) = bound.soft {
                                            p.push(("soft", Json::Int(s as i64)));
                                        }
                                        p
                                    }),
                                )
                            })
                            .collect(),
                    ),
                ),
                ("scope", Json::str(b.scope.clone())),
                ("accounting", ref_json(&b.accounting, semantic)),
            ];
            if let Some(p) = &b.parent {
                v.push(("parent", ref_json(p, semantic)));
            }
            v
        }),
        KindRecord::HarnessRule(r) => Json::obj({
            let mut v = vec![
                ("rule_id", Json::str(r.rule_id.clone())),
                ("trigger", r.trigger.clone()),
                ("action", action_json(&r.action, semantic)),
                ("scope", r.scope.clone()),
            ];
            if let Some(c) = &r.conditioned_on {
                v.push(("conditioned_on", profile_ref_json(c)));
            }
            if let Some(d) = &r.assumption_debt {
                v.push(("assumption_debt", debt_json(d, semantic)));
            }
            v
        }),
    }
}

/// Parse a `KindRecord` for `kind` from its semantic member.
pub(crate) fn semantic_record_from_json(
    kind: EntityKind,
    j: &Json,
    path: &str,
) -> Result<KindRecord, HirError> {
    let rec = match kind {
        EntityKind::Goal => KindRecord::Goal(GoalRecord {
            statement: Text::from_json(req(j, "statement", path)?, &format!("{path}.statement"))?,
            success_criteria: crate::refs::refs_from_json(
                req(j, "success_criteria", path)?,
                &format!("{path}.success_criteria"),
            )?,
            unverifiable_reason: j
                .get("unverifiable_reason")
                .map(|r| Text::from_json(r, &format!("{path}.unverifiable_reason")))
                .transpose()?,
            budget: Ref::from_json(req(j, "budget", path)?, &format!("{path}.budget"))?,
            origin: goal_origin_parse(&req_str(j, "origin", path)?)?,
            parent: j
                .get("parent")
                .map(|p| Ref::from_json(p, &format!("{path}.parent")))
                .transpose()?,
        }),
        EntityKind::Observation => {
            let cj = req(j, "content", path)?;
            let content = if let Some(t) = cj.get("text") {
                ObservationContent::Text(Box::new(Text::from_json(
                    t,
                    &format!("{path}.content.text"),
                )?))
            } else if let Some(s) = cj.get("structured") {
                ObservationContent::Structured(s.clone())
            } else if let Some(a) = cj.get("artifact") {
                ObservationContent::Artifact(Ref::from_json(
                    a,
                    &format!("{path}.content.artifact"),
                )?)
            } else {
                return Err(HirError::SchemaViolation {
                    detail: format!("{path}.content must be text|structured|artifact"),
                });
            };
            KindRecord::Observation(ObservationRecord {
                source: observation_source_parse(&req_str(j, "source", path)?)?,
                producer: req_str(j, "producer", path)?,
                content,
                authority: AuthorityClass::parse(&req_str(j, "authority", path)?).ok_or_else(
                    || HirError::UnknownKind {
                        kind: format!("{path}.authority"),
                    },
                )?,
                taint: match j.get("taint") {
                    Some(Json::Arr(items)) => items
                        .iter()
                        .map(|t| {
                            t.as_str()
                                .ok_or_else(|| HirError::SchemaViolation {
                                    detail: format!("{path}.taint members must be strings"),
                                })
                                .and_then(taint_parse)
                        })
                        .collect::<Result<_, _>>()?,
                    _ => BTreeSet::new(),
                },
                risk_assessment: j.get("risk_assessment").cloned(),
            })
        }
        EntityKind::ContextItem => KindRecord::ContextItem(ContextItemRecord {
            payload: Ref::from_json(req(j, "payload", path)?, &format!("{path}.payload"))?,
            authority: AuthorityClass::parse(&req_str(j, "authority", path)?).ok_or_else(|| {
                HirError::UnknownKind {
                    kind: format!("{path}.authority"),
                }
            })?,
            validity: validity_from_json(req(j, "validity", path)?, &format!("{path}.validity"))?,
            placement_policy: j
                .get("placement_policy")
                .map(|p| Ref::from_json(p, &format!("{path}.placement_policy")))
                .transpose()?,
            priority: match req(j, "priority", path)? {
                Json::Int(i) => *i,
                _ => {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{path}.priority must be an integer"),
                    })
                }
            },
            delivery_id: req_str(j, "delivery_id", path)?,
            activation_observable: req(j, "activation_observable", path)?.clone(),
        }),
        EntityKind::Memory => KindRecord::Memory(MemoryRecord {
            content: Text::from_json(req(j, "content", path)?, &format!("{path}.content"))?,
            validity: validity_from_json(req(j, "validity", path)?, &format!("{path}.validity"))?,
            authority: AuthorityClass::parse(&req_str(j, "authority", path)?).ok_or_else(|| {
                HirError::UnknownKind {
                    kind: format!("{path}.authority"),
                }
            })?,
            confidence: req(j, "confidence", path)?.clone(),
            source_trajectories: run_refs_from_json(
                req(j, "source_trajectories", path)?,
                &format!("{path}.source_trajectories"),
            )?,
            scope: scope_parse(&req_str(j, "scope", path)?)?,
        }),
        EntityKind::Procedure => KindRecord::Procedure(ProcedureRecord {
            preconditions: req(j, "preconditions", path)?.clone(),
            steps: steps_from_json(req(j, "steps", path)?, &format!("{path}.steps"))?,
            expected_evidence: req(j, "expected_evidence", path)?.clone(),
            allowed_capabilities: crate::refs::refs_from_json(
                req(j, "allowed_capabilities", path)?,
                &format!("{path}.allowed_capabilities"),
            )?,
            failure_handlers: req(j, "failure_handlers", path)?.clone(),
        }),
        EntityKind::ToolCapability => {
            let scope_bindings = if j.get("scope_bindings_unknown").is_some() {
                ScopeBindings::Unknown
            } else {
                ScopeBindings::Bindings(j.get("scope_bindings").cloned().unwrap_or(Json::Null))
            };
            KindRecord::ToolCapability(ToolCapabilityRecord {
                purpose: Text::from_json(req(j, "purpose", path)?, &format!("{path}.purpose"))?,
                input_schema: req(j, "input_schema", path)?.clone(),
                output_schema: j.get("output_schema").cloned(),
                effects: tool_effects_from_json(
                    req(j, "effects", path)?,
                    &format!("{path}.effects"),
                )?,
                preconditions: arr(j, "preconditions", path)?
                    .iter()
                    .map(|p| {
                        p.as_str()
                            .ok_or_else(|| HirError::SchemaViolation {
                                detail: format!("{path}.preconditions members must be strings"),
                            })
                            .and_then(PreconditionDomain::parse)
                    })
                    .collect::<Result<_, _>>()?,
                scope_bindings,
                resources: resources_from_json(
                    req(j, "resources", path)?,
                    &format!("{path}.resources"),
                )?,
                observation_contract: req(j, "observation_contract", path)?.clone(),
                cost_model: j.get("cost_model").cloned(),
                execution_requirement: req(j, "execution_requirement", path)?.clone(),
                source: req(j, "source", path)?.clone(),
                exposure_hint: req(j, "exposure_hint", path)?.clone(),
                postconditions: crate::refs::refs_from_json(
                    req(j, "postconditions", path)?,
                    &format!("{path}.postconditions"),
                )?,
                flow_contract: j.get("flow_contract").cloned(),
            })
        }
        EntityKind::Permission => {
            let ij = req(j, "issuer", path)?;
            KindRecord::Permission(PermissionRecord {
                holder: Ref::from_json(req(j, "holder", path)?, &format!("{path}.holder"))?,
                grants: arr(j, "grants", path)?
                    .iter()
                    .enumerate()
                    .map(|(i, g)| grant_from_json(g, &format!("{path}.grants[{i}]")))
                    .collect::<Result<_, _>>()?,
                issuer: Issuer {
                    authority: AuthorityClass::parse(&req_str(
                        ij,
                        "authority",
                        &format!("{path}.issuer"),
                    )?)
                    .ok_or_else(|| HirError::UnknownKind {
                        kind: format!("{path}.issuer.authority"),
                    })?,
                    reference: req_str(ij, "reference", &format!("{path}.issuer"))?,
                },
                validity: validity_from_json(
                    req(j, "validity", path)?,
                    &format!("{path}.validity"),
                )?,
                revocation: j.get("revocation").cloned(),
            })
        }
        EntityKind::Effect => KindRecord::Effect(EffectRecord {
            run_id: req_str(j, "run_id", path)?,
            turn_id: req_str(j, "turn_id", path)?,
            model_call_id: req_str(j, "model_call_id", path)?,
            tool_call_id: req_str(j, "tool_call_id", path)?,
            declared: EffectClass::from_json(
                req(j, "declared", path)?,
                &format!("{path}.declared"),
            )?,
            effective_risk_class: opt_str(j, "effective_risk_class"),
            handle_ids: str_arr(arr(j, "handle_ids", path)?, &format!("{path}.handle_ids"))?,
        }),
        EntityKind::Artifact => KindRecord::Artifact(ArtifactRecord {
            content_hash: req_str(j, "content_hash", path)?,
            media_type: req_str(j, "media_type", path)?,
            size: req_int(j, "size", path)?,
            storage_ref: req_str(j, "storage_ref", path)?,
            produced_by: Ref::from_json(
                req(j, "produced_by", path)?,
                &format!("{path}.produced_by"),
            )?,
        }),
        EntityKind::Validator => KindRecord::Validator(ValidatorRecord {
            kind: validator_kind_from_json(req(j, "kind", path)?, &format!("{path}.kind"))?,
            inputs: crate::refs::refs_from_json(
                req(j, "inputs", path)?,
                &format!("{path}.inputs"),
            )?,
            verdict_type: req_str(j, "verdict_type", path)?,
            deterministic: bool_at(j, "deterministic", path)?,
            cost: req(j, "cost", path)?.clone(),
            evidence_out: req(j, "evidence_out", path)?.clone(),
            assumption_debt: j
                .get("assumption_debt")
                .map(|d| debt_from_json(d, &format!("{path}.assumption_debt")))
                .transpose()?,
        }),
        EntityKind::AgentProcess => {
            let body_j = req(j, "body", path)?;
            let body = if let Some(n) = body_j.get("native") {
                let np = format!("{path}.body.native");
                AgentProcessBody::Native(NativeProcess {
                    harness_def: Ref::from_json(
                        req(n, "harness_def", &np)?,
                        &format!("{np}.harness_def"),
                    )?,
                    profile: profile_ref_from_json(
                        req(n, "profile", &np)?,
                        &format!("{np}.profile"),
                    )?,
                    slots: slots_from_json(req(n, "slots", &np)?, &format!("{np}.slots"))?,
                    control_boundary: boundary_from_json(
                        req(n, "control_boundary", &np)?,
                        &format!("{np}.control_boundary"),
                    )?,
                    budget: Ref::from_json(req(n, "budget", &np)?, &format!("{np}.budget"))?,
                    permissions: Ref::from_json(
                        req(n, "permissions", &np)?,
                        &format!("{np}.permissions"),
                    )?,
                    environment: EnvironmentRef {
                        environment: req_str(
                            req(n, "environment", &np)?,
                            "environment",
                            &format!("{np}.environment"),
                        )?,
                    },
                })
            } else if let Some(h) = body_j.get("hosted") {
                let hp = format!("{path}.body.hosted");
                AgentProcessBody::Hosted(OpaqueProcess {
                    participant_ref: req_str(h, "participant_ref", &hp)?,
                    declared_capabilities: cap_decl_from_json(
                        req(h, "declared_capabilities", &hp)?,
                        &format!("{hp}.declared_capabilities"),
                    )?,
                    observability_levels: arr(h, "observability_levels", &hp)?
                        .iter()
                        .map(|o| {
                            o.as_str()
                                .ok_or_else(|| HirError::SchemaViolation {
                                    detail: format!("{hp}.observability_levels must be strings"),
                                })
                                .and_then(observability_parse)
                        })
                        .collect::<Result<_, _>>()?,
                    participant_version: req_str(h, "participant_version", &hp)?,
                    version_identity: opt_str(h, "version_identity"),
                    hosting_mechanism: match req_str(h, "hosting_mechanism", &hp)?.as_str() {
                        // `none` is legal only on a native descriptor (CF-351); a hosted
                        // body carrying it is a schema-level refusal.
                        "none" => {
                            return Err(HirError::SchemaViolation {
                                detail: format!(
                                    "{hp}.hosting_mechanism: none on OpaqueProcess (CF-351)"
                                ),
                            })
                        }
                        "session_abi" => HostingMechanism::SessionAbi,
                        "model_boundary_intercept" => HostingMechanism::ModelBoundaryIntercept,
                        "container_installed" => HostingMechanism::ContainerInstalled,
                        other => {
                            return Err(HirError::UnknownKind {
                                kind: format!("hosting mechanism {other}"),
                            })
                        }
                    },
                    supplies: supplies_from_json(
                        req(h, "supplies", &hp)?,
                        &format!("{hp}.supplies"),
                    )?,
                    budget: Ref::from_json(req(h, "budget", &hp)?, &format!("{hp}.budget"))?,
                    permissions: Ref::from_json(
                        req(h, "permissions", &hp)?,
                        &format!("{hp}.permissions"),
                    )?,
                    params: h.get("params").cloned(),
                })
            } else {
                return Err(HirError::SchemaViolation {
                    detail: format!("{path}.body must be native|hosted"),
                });
            };
            KindRecord::AgentProcess(AgentProcessRecord { body })
        }
        EntityKind::Budget => {
            let dims = match req(j, "dimensions", path)? {
                Json::Obj(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let bound = DimensionBound {
                            hard: v.get("hard").and_then(|h| match h {
                                Json::Int(i) if *i >= 0 => Some(*i as u64),
                                _ => None,
                            }),
                            soft: v.get("soft").and_then(|s| match s {
                                Json::Int(i) if *i >= 0 => Some(*i as u64),
                                _ => None,
                            }),
                        };
                        Ok((k.clone(), bound))
                    })
                    .collect::<Result<BTreeMap<_, _>, HirError>>()?,
                _ => {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{path}.dimensions must be an object"),
                    })
                }
            };
            KindRecord::Budget(BudgetRecord {
                dimensions: dims,
                scope: req_str(j, "scope", path)?,
                parent: j
                    .get("parent")
                    .map(|p| Ref::from_json(p, &format!("{path}.parent")))
                    .transpose()?,
                accounting: Ref::from_json(
                    req(j, "accounting", path)?,
                    &format!("{path}.accounting"),
                )?,
            })
        }
        EntityKind::HarnessRule => KindRecord::HarnessRule(HarnessRuleRecord {
            rule_id: req_str(j, "rule_id", path)?,
            trigger: req(j, "trigger", path)?.clone(),
            action: action_from_json(req(j, "action", path)?, &format!("{path}.action"))?,
            scope: req(j, "scope", path)?.clone(),
            conditioned_on: j
                .get("conditioned_on")
                .map(|c| profile_ref_from_json(c, &format!("{path}.conditioned_on")))
                .transpose()?,
            assumption_debt: j
                .get("assumption_debt")
                .map(|d| debt_from_json(d, &format!("{path}.assumption_debt")))
                .transpose()?,
        }),
    };
    Ok(rec)
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface records
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn surface_json(s: &SurfaceRecord, semantic: bool) -> Json {
    // Surface records never enter semantic_id; `semantic` is irrelevant except that Text
    // leaves inside a surface still serialize canonically (with provenance).
    let _ = semantic;
    match s {
        SurfaceRecord::Tool(t) => Json::obj({
            let mut v = vec![
                ("name", Json::str(t.name.clone())),
                ("namespace", Json::str(t.namespace.clone())),
                ("description_template", t.description_template.to_json()),
                (
                    "argument_order",
                    Json::Arr(
                        t.argument_order
                            .iter()
                            .map(|a| Json::str(a.clone()))
                            .collect(),
                    ),
                ),
                ("examples", t.examples.clone()),
                ("error_format", t.error_format.clone()),
                ("result_renderer", t.result_renderer.clone()),
                ("strictness", t.strictness.clone()),
                (
                    "schema_dialect_narrowing",
                    t.schema_dialect_narrowing.clone(),
                ),
                ("exposure_mode", t.exposure_mode.clone()),
            ];
            if let Some(d) = &t.display_title {
                v.push(("display_title", Json::str(d.clone())));
            }
            if let Some(i) = &t.icon_ref {
                v.push(("icon_ref", Json::str(i.clone())));
            }
            v
        }),
        SurfaceRecord::Validator(v) => Json::obj([
            ("rubric_rendering", v.rubric_rendering.clone()),
            ("explain", v.explain.clone()),
        ]),
        SurfaceRecord::Procedure(p) => {
            Json::obj([("compile_hint", Json::str(p.compile_hint.name()))])
        }
        SurfaceRecord::ContextItem(c) => Json::obj([
            ("rendering_template", c.rendering_template.clone()),
            ("position", c.position.clone()),
        ]),
    }
}

pub(crate) fn surface_from_json(
    kind: EntityKind,
    j: &Json,
    path: &str,
) -> Result<SurfaceRecord, HirError> {
    match kind {
        EntityKind::ToolCapability => Ok(SurfaceRecord::Tool(Box::new(ToolSurface {
            name: req_str(j, "name", path)?,
            namespace: req_str(j, "namespace", path)?,
            description_template: Text::from_json(
                req(j, "description_template", path)?,
                &format!("{path}.description_template"),
            )?,
            argument_order: str_arr(
                arr(j, "argument_order", path)?,
                &format!("{path}.argument_order"),
            )?,
            examples: j.get("examples").cloned().unwrap_or(Json::Arr(vec![])),
            error_format: j.get("error_format").cloned().unwrap_or(Json::Null),
            result_renderer: j.get("result_renderer").cloned().unwrap_or(Json::Null),
            strictness: j.get("strictness").cloned().unwrap_or(Json::Null),
            schema_dialect_narrowing: j
                .get("schema_dialect_narrowing")
                .cloned()
                .unwrap_or(Json::Null),
            exposure_mode: j.get("exposure_mode").cloned().unwrap_or(Json::Null),
            display_title: opt_str(j, "display_title"),
            icon_ref: opt_str(j, "icon_ref"),
        }))),
        EntityKind::Validator => Ok(SurfaceRecord::Validator(ValidatorSurface {
            rubric_rendering: j.get("rubric_rendering").cloned().unwrap_or(Json::Null),
            explain: j.get("explain").cloned().unwrap_or(Json::Null),
        })),
        EntityKind::Procedure => {
            let hint = match req_str(j, "compile_hint", path)?.as_str() {
                "instruction" => CompileHint::Instruction,
                "workflow_node" => CompileHint::WorkflowNode,
                "subagent_task" => CompileHint::SubagentTask,
                other => {
                    return Err(HirError::UnknownKind {
                        kind: format!("compile hint {other}"),
                    })
                }
            };
            Ok(SurfaceRecord::Procedure(ProcedureSurface {
                compile_hint: hint,
            }))
        }
        EntityKind::ContextItem => Ok(SurfaceRecord::ContextItem(ContextItemSurface {
            rendering_template: j.get("rendering_template").cloned().unwrap_or(Json::Null),
            position: j.get("position").cloned().unwrap_or(Json::Null),
        })),
        _ => Err(HirError::UnexpressibleSurface {
            detail: format!("{path}: kind {kind} carries no surface record (§3.1.5)"),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge records
// ─────────────────────────────────────────────────────────────────────────────

fn edge_fields_json(e: &EdgeRecord, semantic: bool) -> Json {
    match e {
        EdgeRecord::DependsOn { reason } => Json::obj([("reason", Json::str(reason.clone()))]),
        EdgeRecord::Supersedes { reason } => Json::obj([("reason", Json::str(reason.name()))]),
        EdgeRecord::Authorizes {
            scope,
            effect_class,
        } => Json::obj({
            let mut v = vec![("scope", Json::str(scope.clone()))];
            if let Some(c) = effect_class {
                v.push(("effect_class", effect_class_json(c, semantic)));
            }
            v
        }),
        EdgeRecord::ProducedBy => Json::obj([]),
        EdgeRecord::Validates => Json::obj([]),
        EdgeRecord::DelegatedTo { permission, budget } => Json::obj([
            ("permission", ref_json(permission, semantic)),
            ("budget", ref_json(budget, semantic)),
        ]),
        EdgeRecord::DerivedFrom {
            hypothesis,
            trajectories,
            candidate_id,
        } => Json::obj({
            let mut v = vec![
                ("hypothesis", text_json(hypothesis, semantic)),
                ("trajectories", run_refs_json(trajectories)),
            ];
            if let Some(c) = candidate_id {
                v.push(("candidate_id", Json::str(c.clone())));
            }
            v
        }),
    }
}

fn supersede_reason_parse(s: &str) -> Result<SupersedeReason, HirError> {
    match s {
        "edit" => Ok(SupersedeReason::Edit),
        "revocation" => Ok(SupersedeReason::Revocation),
        "expiry" => Ok(SupersedeReason::Expiry),
        "migration" => Ok(SupersedeReason::Migration),
        "consolidation" => Ok(SupersedeReason::Consolidation),
        "fork" => Ok(SupersedeReason::Fork),
        other => Err(HirError::UnknownKind {
            kind: format!("supersede reason {other}"),
        }),
    }
}

fn edge_fields_from_json(kind: EdgeKind, j: &Json, path: &str) -> Result<EdgeRecord, HirError> {
    match kind {
        EdgeKind::DependsOn => Ok(EdgeRecord::DependsOn {
            reason: req_str(j, "reason", path)?,
        }),
        EdgeKind::Supersedes => Ok(EdgeRecord::Supersedes {
            reason: supersede_reason_parse(&req_str(j, "reason", path)?)?,
        }),
        EdgeKind::Authorizes => Ok(EdgeRecord::Authorizes {
            scope: req_str(j, "scope", path)?,
            effect_class: j
                .get("effect_class")
                .map(|c| EffectClass::from_json(c, &format!("{path}.effect_class")))
                .transpose()?,
        }),
        EdgeKind::ProducedBy => Ok(EdgeRecord::ProducedBy),
        EdgeKind::Validates => Ok(EdgeRecord::Validates),
        EdgeKind::DelegatedTo => Ok(EdgeRecord::DelegatedTo {
            permission: Ref::from_json(req(j, "permission", path)?, &format!("{path}.permission"))?,
            budget: Ref::from_json(req(j, "budget", path)?, &format!("{path}.budget"))?,
        }),
        EdgeKind::DerivedFrom => Ok(EdgeRecord::DerivedFrom {
            hypothesis: Box::new(Text::from_json(
                req(j, "hypothesis", path)?,
                &format!("{path}.hypothesis"),
            )?),
            trajectories: run_refs_from_json(
                req(j, "trajectories", path)?,
                &format!("{path}.trajectories"),
            )?,
            candidate_id: opt_str(j, "candidate_id"),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Version record / node / edge / document
// ─────────────────────────────────────────────────────────────────────────────

fn version_json(v: &VersionRecord) -> Json {
    Json::obj({
        let mut p = vec![("sealed", Json::Bool(v.sealed))];
        if let Some(i) = &v.version_id {
            p.push(("version_id", Json::str(i.clone())));
        }
        if let Some(i) = &v.semantic_id {
            p.push(("semantic_id", Json::str(i.clone())));
        }
        if let Some(s) = &v.supersedes {
            p.push(("supersedes", Json::str(s.clone())));
        }
        p
    })
}

fn version_from_json(j: &Json, path: &str) -> Result<VersionRecord, HirError> {
    Ok(VersionRecord {
        version_id: opt_str(j, "version_id"),
        semantic_id: opt_str(j, "semantic_id"),
        dialect: "HIR/1".into(),
        supersedes: opt_str(j, "supersedes"),
        sealed: bool_at(j, "sealed", path).unwrap_or(false),
    })
}

/// A node's canonical JSON. `version_id`/`semantic_id` appear inside `version` when set —
/// the **identity basis** ([`crate::identity`]) serializes the same form with the id members
/// removed, which is why `identity` is a pure projection, not a parse-time fact.
pub fn node_to_json(n: &Node) -> Json {
    Json::obj({
        let mut v = vec![
            ("kind", Json::str(n.kind.name())),
            ("dialect", Json::str(n.version.dialect.clone())),
            ("provenance", n.provenance.to_json()),
            ("semantic", semantic_record_json(&n.semantic, false)),
            ("version", version_json(&n.version)),
        ];
        if let Some(s) = &n.surface {
            v.push(("surface", surface_json(s, false)));
        }
        if !n.ext.is_empty() {
            v.push((
                "ext",
                Json::Obj(n.ext.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            ));
        }
        v
    })
}

/// The semantic projection of a node — `{kind ∥ semantic ∥ refs-by-semantic_id}` with
/// `Text` leaves contributing their content hash; `surface`, `provenance`, `ext` and the
/// version record are excluded (§3.1.2).
pub fn node_semantic_projection(n: &Node) -> Json {
    Json::obj([
        ("kind", Json::str(n.kind.name())),
        ("semantic", semantic_record_json(&n.semantic, true)),
    ])
}

/// The identity-basis form — the canonical node with the computed id members removed (what
/// `version_id = H(canonical(node))` hashes).
pub(crate) fn node_identity_basis(n: &Node) -> Json {
    let mut basis = node_to_json(n);
    if let Some(Json::Obj(m)) = basis.get("version").cloned().map(|v| {
        let mut m = match v {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        m.remove("version_id");
        m.remove("semantic_id");
        Json::Obj(m)
    }) {
        if let Json::Obj(members) = &mut basis {
            members.insert("version".to_string(), Json::Obj(m));
        }
    }
    basis
}

/// An edge's canonical JSON.
pub(crate) fn edge_to_json(e: &Edge) -> Json {
    Json::obj([
        ("kind", Json::str(e.kind.name())),
        ("dialect", Json::str(e.version.dialect.clone())),
        ("from", Json::str(e.from.clone())),
        ("to", Json::str(e.to.clone())),
        ("provenance", e.provenance.to_json()),
        ("fields", edge_fields_json(&e.fields, false)),
        ("version", version_json(&e.version)),
    ])
}

/// The semantic projection of an edge — `{kind, from, to, fields}` with `fields`' refs by
/// `semantic_id` (§3.1.2; the version record is uniform across nodes and edges).
pub(crate) fn edge_semantic_projection(e: &Edge) -> Json {
    Json::obj([
        ("kind", Json::str(e.kind.name())),
        ("from", Json::str(e.from.clone())),
        ("to", Json::str(e.to.clone())),
        ("fields", edge_fields_json(&e.fields, true)),
    ])
}

/// The edge identity basis (version ids removed).
pub(crate) fn edge_identity_basis(e: &Edge) -> Json {
    let mut basis = edge_to_json(e);
    if let Some(Json::Obj(m)) = basis.get("version").cloned().map(|v| {
        let mut m = match v {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        m.remove("version_id");
        m.remove("semantic_id");
        Json::Obj(m)
    }) {
        if let Json::Obj(members) = &mut basis {
            members.insert("version".to_string(), Json::Obj(m));
        }
    }
    basis
}

impl Node {
    /// The canonical JSON of this node.
    pub fn to_json(&self) -> Json {
        node_to_json(self)
    }
}

impl Edge {
    /// The canonical JSON of this edge.
    pub fn to_json(&self) -> Json {
        edge_to_json(self)
    }
}

impl HirDocument {
    /// The canonical JSON of the document — `{hir_version, root, nodes, edges, assembly?}`.
    /// `nodes`/`edges` are order-insignificant sets, so the canonical form sorts each
    /// member by its canonical bytes — `canonicalize` is order-normalizing and two
    /// implementations agree byte-for-byte without an order convention (§3.1.7).
    pub fn to_json(&self) -> Json {
        let mut nodes: Vec<Json> = self.nodes.iter().map(node_to_json).collect();
        nodes.sort_by_key(Json::to_canonical_string);
        let mut edges: Vec<Json> = self.edges.iter().map(edge_to_json).collect();
        edges.sort_by_key(Json::to_canonical_string);
        Json::obj({
            let mut v = vec![
                ("hir_version", Json::str(self.hir_version.clone())),
                ("root", self.root.to_json()),
                ("nodes", Json::Arr(nodes)),
                ("edges", Json::Arr(edges)),
            ];
            if let Some(a) = &self.assembly {
                v.push(("assembly", a.clone()));
            }
            v
        })
    }
}

pub(crate) fn node_from_json(j: &Json, path: &str) -> Result<Node, HirError> {
    let kind = EntityKind::parse(&req_str(j, "kind", path)?)?;
    let dialect = req_str(j, "dialect", path)?;
    if dialect != "HIR/1" {
        return Err(HirError::DialectUnsupported { dialect });
    }
    let provenance_j = req(j, "provenance", path).map_err(|_| HirError::MissingProvenance {
        what: format!("{path} (node {kind})"),
    })?;
    let provenance = provenance_from_json(provenance_j, &format!("{path}.provenance"))?;
    let semantic =
        semantic_record_from_json(kind, req(j, "semantic", path)?, &format!("{path}.semantic"))?;
    let surface = j
        .get("surface")
        .map(|s| surface_from_json(kind, s, &format!("{path}.surface")))
        .transpose()?;
    let ext = match j.get("ext") {
        Some(Json::Obj(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Some(_) => {
            return Err(HirError::SchemaViolation {
                detail: format!("{path}.ext must be an object"),
            })
        }
        None => BTreeMap::new(),
    };
    let mut version = version_from_json(req(j, "version", path)?, &format!("{path}.version"))?;
    version.dialect = dialect;
    Ok(Node {
        kind,
        provenance,
        semantic,
        surface,
        ext,
        version,
    })
}

fn edge_from_json(j: &Json, path: &str) -> Result<Edge, HirError> {
    let kind = EdgeKind::parse(&req_str(j, "kind", path)?)?;
    let dialect = req_str(j, "dialect", path)?;
    if dialect != "HIR/1" {
        return Err(HirError::DialectUnsupported { dialect });
    }
    let provenance_j = req(j, "provenance", path).map_err(|_| HirError::MissingProvenance {
        what: format!("{path} (edge {kind})"),
    })?;
    let provenance = provenance_from_json(provenance_j, &format!("{path}.provenance"))?;
    let fields = edge_fields_from_json(kind, req(j, "fields", path)?, &format!("{path}.fields"))?;
    let mut version = version_from_json(req(j, "version", path)?, &format!("{path}.version"))?;
    version.dialect = dialect;
    Ok(Edge {
        kind,
        from: req_str(j, "from", path)?,
        to: req_str(j, "to", path)?,
        provenance,
        fields,
        version,
    })
}

/// Parse a document from its canonical JSON (post-`parse_canonical` — the input is already
/// the sorted-key compact form).
pub(crate) fn document_from_json(j: &Json) -> Result<HirDocument, HirError> {
    let hir_version = req_str(j, "hir_version", "$")?;
    if hir_version != "HIR/1" {
        return Err(HirError::DialectUnsupported {
            dialect: hir_version,
        });
    }
    let nodes = match req(j, "nodes", "$")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, n)| node_from_json(n, &format!("$.nodes[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: "$.nodes must be an array".into(),
            })
        }
    };
    let edges = match req(j, "edges", "$")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, e)| edge_from_json(e, &format!("$.edges[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: "$.edges must be an array".into(),
            })
        }
    };
    Ok(HirDocument {
        hir_version,
        root: Ref::from_json(req(j, "root", "$")?, "$.root")?,
        nodes,
        edges,
        assembly: j.get("assembly").cloned(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// HirDiff canonical form (§3.1.7) — the diff is itself a canonical record.
// ─────────────────────────────────────────────────────────────────────────────

fn tag_json(t: &crate::diff::DiffOpTag) -> Json {
    Json::obj({
        let mut v = vec![
            ("entity_kind", Json::str(t.entity_kind.clone())),
            ("semantic", Json::Bool(t.semantic)),
        ];
        if let Some(p) = &t.plane {
            v.push(("plane", Json::str(p.clone())));
        }
        v
    })
}

fn tag_from_json(j: &Json, path: &str) -> Result<crate::diff::DiffOpTag, HirError> {
    Ok(crate::diff::DiffOpTag {
        plane: j.get("plane").and_then(Json::as_str).map(str::to_string),
        entity_kind: req_str(j, "entity_kind", path)?,
        semantic: bool_at(j, "semantic", path)?,
    })
}

/// The canonical JSON of a [`crate::diff::DiffOp`] — `{op, tag{plane?, entity_kind,
/// semantic}, …op fields}`.
pub(crate) fn diff_op_json(op: &crate::diff::DiffOp) -> Json {
    use crate::diff::DiffOp::*;
    let (name, fields): (&'static str, Vec<(&'static str, Json)>) = match op {
        AddNode { node, .. } => ("add_node", vec![("node", node.clone())]),
        RemoveNode { id, node, .. } => (
            "remove_node",
            vec![("id", Json::str(id.clone())), ("node", node.clone())],
        ),
        ReplaceField {
            id, path, old, new, ..
        } => (
            "replace_field",
            vec![
                ("id", Json::str(id.clone())),
                ("path", Json::str(path.clone())),
                ("old", old.clone()),
                ("new", new.clone()),
            ],
        ),
        ReplaceLeaf {
            id,
            path,
            old_hash,
            new_hash,
            ..
        } => (
            "replace_leaf",
            vec![
                ("id", Json::str(id.clone())),
                ("path", Json::str(path.clone())),
                ("old_hash", Json::str(old_hash.clone())),
                ("new_hash", Json::str(new_hash.clone())),
            ],
        ),
        AddEdge { edge, .. } => ("add_edge", vec![("edge", edge.clone())]),
        RemoveEdge { edge, .. } => ("remove_edge", vec![("edge", edge.clone())]),
        Rebind {
            id,
            path,
            old_ref,
            new_ref,
            ..
        } => (
            "rebind",
            vec![
                ("id", Json::str(id.clone())),
                ("path", Json::str(path.clone())),
                ("old_ref", old_ref.clone()),
                ("new_ref", new_ref.clone()),
            ],
        ),
        SurfaceEdit {
            profile,
            id,
            path,
            old,
            new,
            ..
        } => (
            "surface_edit",
            vec![
                ("id", Json::str(id.clone())),
                ("path", Json::str(path.clone())),
                ("old", old.clone()),
                ("new", new.clone()),
                (
                    "profile_ref",
                    profile.clone().map(Json::str).unwrap_or(Json::Null),
                ),
            ],
        ),
        ExtEdit {
            id, key, old, new, ..
        } => (
            "ext_edit",
            vec![
                ("id", Json::str(id.clone())),
                ("key", Json::str(key.clone())),
                ("old", old.clone()),
                ("new", new.clone()),
            ],
        ),
    };
    let mut pairs = vec![("op", Json::str(name)), ("tag", tag_json(op.tag()))];
    pairs.extend(fields);
    Json::obj(pairs)
}

/// Parse a diff op from its canonical form.
pub(crate) fn diff_op_from_json(j: &Json, path: &str) -> Result<crate::diff::DiffOp, HirError> {
    use crate::diff::DiffOp::*;
    let name = req_str(j, "op", path)?;
    let tag = tag_from_json(req(j, "tag", path)?, &format!("{path}.tag"))?;
    let s = |k: &str| req_str(j, k, path);
    let r = |k: &str| req(j, k, path).cloned();
    match name.as_str() {
        "add_node" => Ok(AddNode {
            node: r("node")?,
            tag,
        }),
        "remove_node" => Ok(RemoveNode {
            id: s("id")?,
            node: r("node")?,
            tag,
        }),
        "replace_field" => Ok(ReplaceField {
            id: s("id")?,
            path: s("path")?,
            old: r("old")?,
            new: r("new")?,
            tag,
        }),
        "replace_leaf" => Ok(ReplaceLeaf {
            id: s("id")?,
            path: s("path")?,
            old_hash: s("old_hash")?,
            new_hash: s("new_hash")?,
            tag,
        }),
        "add_edge" => Ok(AddEdge {
            edge: r("edge")?,
            tag,
        }),
        "remove_edge" => Ok(RemoveEdge {
            edge: r("edge")?,
            tag,
        }),
        "rebind" => Ok(Rebind {
            id: s("id")?,
            path: s("path")?,
            old_ref: r("old_ref")?,
            new_ref: r("new_ref")?,
            tag,
        }),
        "surface_edit" => Ok(SurfaceEdit {
            profile: j
                .get("profile_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            id: s("id")?,
            path: s("path")?,
            old: r("old")?,
            new: r("new")?,
            tag,
        }),
        "ext_edit" => Ok(ExtEdit {
            id: s("id")?,
            key: s("key")?,
            old: r("old")?,
            new: r("new")?,
            tag,
        }),
        other => Err(HirError::UnknownKind {
            kind: format!("diff op {other}"),
        }),
    }
}

fn delta_name(d: crate::diff::Delta) -> &'static str {
    match d {
        crate::diff::Delta::None => "none",
        crate::diff::Delta::Tightening => "tightening",
        crate::diff::Delta::Loosening => "loosening",
    }
}

fn delta_parse(s: &str, path: &str) -> Result<crate::diff::Delta, HirError> {
    match s {
        "none" => Ok(crate::diff::Delta::None),
        "tightening" => Ok(crate::diff::Delta::Tightening),
        "loosening" => Ok(crate::diff::Delta::Loosening),
        other => Err(HirError::UnknownKind {
            kind: format!("{path}: delta {other}"),
        }),
    }
}

fn classification_json(c: &crate::diff::DiffClassification) -> Json {
    Json::obj([
        ("semantic_ops", Json::Int(c.semantic_ops as i64)),
        ("surface_ops", Json::Int(c.surface_ops as i64)),
        (
            "provenance_only_ops",
            Json::Int(c.provenance_only_ops as i64),
        ),
        ("ext_ops", Json::Int(c.ext_ops as i64)),
        (
            "authority_delta",
            Json::str(match c.authority_delta {
                crate::diff::AuthorityDelta::None => "none",
                crate::diff::AuthorityDelta::Narrowing => "narrowing",
                crate::diff::AuthorityDelta::Widening => "widening",
            }),
        ),
        ("budget_delta", Json::str(delta_name(c.budget_delta))),
        ("validity_delta", Json::str(delta_name(c.validity_delta))),
        (
            "coordination_delta",
            Json::str(delta_name(c.coordination_delta)),
        ),
        (
            "touches_conditioned_rules",
            Json::Arr(
                c.touches_conditioned_rules
                    .iter()
                    .map(|r| Json::str(r.clone()))
                    .collect(),
            ),
        ),
    ])
}

fn classification_from_json(
    j: &Json,
    path: &str,
) -> Result<crate::diff::DiffClassification, HirError> {
    let int = |k: &str| req_int(j, k, path).map(|v| v as usize);
    Ok(crate::diff::DiffClassification {
        semantic_ops: int("semantic_ops")?,
        surface_ops: int("surface_ops")?,
        provenance_only_ops: int("provenance_only_ops")?,
        ext_ops: int("ext_ops")?,
        authority_delta: match req_str(j, "authority_delta", path)?.as_str() {
            "none" => crate::diff::AuthorityDelta::None,
            "narrowing" => crate::diff::AuthorityDelta::Narrowing,
            "widening" => crate::diff::AuthorityDelta::Widening,
            other => {
                return Err(HirError::UnknownKind {
                    kind: format!("{path}.authority_delta: {other}"),
                })
            }
        },
        budget_delta: delta_parse(
            &req_str(j, "budget_delta", path)?,
            &format!("{path}.budget_delta"),
        )?,
        validity_delta: delta_parse(
            &req_str(j, "validity_delta", path)?,
            &format!("{path}.validity_delta"),
        )?,
        coordination_delta: delta_parse(
            &req_str(j, "coordination_delta", path)?,
            &format!("{path}.coordination_delta"),
        )?,
        touches_conditioned_rules: str_arr(
            arr(j, "touches_conditioned_rules", path)?,
            &format!("{path}.touches_conditioned_rules"),
        )?,
    })
}

/// The canonical JSON of a [`crate::diff::HirDiff`].
pub(crate) fn diff_to_json(d: &crate::diff::HirDiff) -> Json {
    let mut v = vec![
        ("dialect", Json::str(d.dialect.clone())),
        (
            "base",
            Json::obj([
                ("semantic_id", Json::str(d.base.semantic_id.clone())),
                ("version_id", Json::str(d.base.version_id.clone())),
            ]),
        ),
        (
            "target",
            Json::obj([
                ("semantic_id", Json::str(d.target.semantic_id.clone())),
                ("version_id", Json::str(d.target.version_id.clone())),
            ]),
        ),
        ("ops", Json::Arr(d.ops.iter().map(diff_op_json).collect())),
        ("classification", classification_json(&d.classification)),
        ("provenance", d.provenance.to_json()),
    ];
    // `derived_from` — the derivation record (empty only for human|migration origins).
    let mut der = vec![];
    if let Some(h) = &d.derivation.hypothesis {
        der.push(("hypothesis", text_json(h, false)));
    }
    if !d.derivation.trajectories.is_empty() {
        der.push((
            "trajectories",
            Json::Arr(
                d.derivation
                    .trajectories
                    .iter()
                    .map(|r| Json::obj([("run", Json::str(r.run.clone()))]))
                    .collect(),
            ),
        ));
    }
    if let Some(c) = &d.derivation.candidate_id {
        der.push(("candidate_id", Json::str(c.clone())));
    }
    v.push(("derived_from", Json::obj(der)));
    Json::obj(v)
}

/// Parse a [`crate::diff::HirDiff`] from its canonical form.
pub(crate) fn diff_from_json(j: &Json) -> Result<crate::diff::HirDiff, HirError> {
    let dialect = req_str(j, "dialect", "$")?;
    if dialect != crate::DIALECT {
        return Err(HirError::DialectUnsupported { dialect });
    }
    let def_ref = |k: &str| -> Result<crate::document::DefinitionVersionRef, HirError> {
        let r = req(j, k, "$")?;
        Ok(crate::document::DefinitionVersionRef {
            semantic_id: req_str(r, "semantic_id", &format!("$.{k}"))?,
            version_id: req_str(r, "version_id", &format!("$.{k}"))?,
        })
    };
    let ops = match req(j, "ops", "$")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, op)| diff_op_from_json(op, &format!("$.ops[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(HirError::SchemaViolation {
                detail: "$.ops must be an array".into(),
            })
        }
    };
    let der = req(j, "derived_from", "$")?;
    let derivation = crate::diff::DiffDerivation {
        hypothesis: der
            .get("hypothesis")
            .map(|h| Text::from_json(h, "$.derived_from.hypothesis"))
            .transpose()?,
        trajectories: match der.get("trajectories") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    req_str(t, "run", &format!("$.derived_from.trajectories[{i}]"))
                        .map(|run| crate::refs::RunRef { run })
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(HirError::SchemaViolation {
                    detail: "$.derived_from.trajectories must be an array".into(),
                })
            }
            None => Vec::new(),
        },
        candidate_id: der
            .get("candidate_id")
            .and_then(Json::as_str)
            .map(str::to_string),
    };
    Ok(crate::diff::HirDiff {
        base: def_ref("base")?,
        target: def_ref("target")?,
        dialect,
        ops,
        classification: classification_from_json(
            req(j, "classification", "$")?,
            "$.classification",
        )?,
        provenance: provenance_from_json(req(j, "provenance", "$")?, "$.provenance")?,
        derivation,
    })
}
