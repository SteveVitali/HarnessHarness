//! `memory_abi` — the `memory_store` class contract's wire codec and op
//! dispatch (§5c.3 `MemoryStore` table; ADR-0078 d2/0080; S4.16b).
//!
//! The contract binds in-process ([`crate::memory::MemoryStore`] through
//! [`crate::memory::MemoryStorePort`]) and out-of-process (the packaged
//! `hh-memory-store` plugin over `plugin_abi/1`). One codec serves both
//! placements (CC1/CC7 — the wire shape is declared here, never twice):
//!
//! - `dispatch(store, operation, inputs)` — every [`MemoryStorePort`] op as
//!   `(operation, inputs[]) → outputs[]` over canonical JSON;
//! - domain refusals ride **inside** the outputs as the typed
//!   `{ok:false, refusal:{code, detail}}` envelope — ABI-level failures
//!   (`AbiError`) are reserved for channel/schema faults, never for a
//!   domain refusal (the refusal is data, not a fault);
//! - decode is strict (`CodecError::{BadMember, MissingMember,
//!   TypeMismatch}`) — an unknown member refuses, never coerces (CC3).
//!
//! Wire shapes are the `body_json`/`contract_json` encodings `memory.rs`
//! already mints (`version` outputs are byte-identical to what the store
//! hashes), extended with the members the boundary needs (`version_id`,
//! `label`, `contract`, `provenance`, `validity`, `supersedes_claim`,
//! `conflict_set_ref`, `validator_endorsed`).

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_hir::records::Validity;
use hh_identity::kinds::RecordKind;
use hh_identity::names::ResolveMode;
use hh_identity::refs::{Name, NameSelector, VersionedRef};
use hh_identity::supersede::{RevocationRecord, SupersedeReason, SupersedesEdge};
use hh_ledger::manifest::EventRef;
use hh_provenance::label::Label;
use hh_provenance::{PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

use crate::codec::{
    arr_at, bool_at, expect_obj, int_at, label_json, reject_unknown, str_at, validity_json,
    CodecError,
};
use crate::lifecycle::LifecycleError;
use crate::memory::{
    render_origin, ConflictSet, DependencyStamp, Freshness, InvalidationContract, Justification,
    JustificationKind, MemoryDraft, MemoryManifest, MemoryRevocation, MemoryStorePort,
    MemoryVersion, NameBinding, PutOutcome, ResolveOutcome, SupersedeClaim, SupersedeClaimReason,
    WriteContext,
};
use crate::vocab::{
    CacheHint, ConflictResolution, DependencyKind, Granularity, InvalidationCondition, Layer,
    LifecycleStateKind, MemoryContent, MemoryKind, Revalidation, RevocationReason,
};

/// The class id the contract binds under (`plugin_abi/1` `class_contract`).
pub const MEMORY_STORE_CLASS: &str = "memory_store";
/// The contract version this codec implements.
pub const MEMORY_STORE_CONTRACT: &str = "1.0";

// ─────────────────────────────────────────────────────────────────────────────
// Small codecs
// ─────────────────────────────────────────────────────────────────────────────

fn opt_str(m: &BTreeMap<String, Json>, member: &str) -> Result<Option<String>, CodecError> {
    match m.get(member) {
        Some(Json::Str(s)) => Ok(Some(s.clone())),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "string",
        }),
    }
}

fn opt_int(m: &BTreeMap<String, Json>, member: &str) -> Result<Option<u64>, CodecError> {
    match m.get(member) {
        Some(Json::Int(i)) => Ok(Some(*i as u64)),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "integer",
        }),
    }
}

fn scope_from_str(s: &str) -> Result<PersistenceScope, CodecError> {
    PersistenceScope::parse(s).ok_or(CodecError::TypeMismatch {
        member: "scope".to_string(),
        expected: "a PersistenceScope spelling",
    })
}

fn scope_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
) -> Result<PersistenceScope, CodecError> {
    scope_from_str(str_at(m, member, "memory_store.input")?)
}

/// `RecordKind` from its `domain_tag` (the wire carries the tag — the same
/// spelling `vref_str` renders).
fn record_kind_from_tag(tag: &str) -> Result<RecordKind, CodecError> {
    use RecordKind::*;
    for k in [
        HirNode,
        HirEdge,
        HirTextLeaf,
        SealedDefinition,
        VariantRecord,
        ModelProfile,
        PermissionPolicy,
        Budget,
        Validator,
        TaskDataManifest,
        CapabilityDeclaration,
        RunManifest,
        BundleManifest,
        NameBindingRecord,
        IdentityProfile,
        ExtensionRecord,
        RegistryRecord,
        RevocationRecord,
        Configuration,
        ConfigurationVersion,
        ContainmentPolicy,
        SecretChannel,
        CredentialBinding,
        SinkPolicy,
        EnvironmentRecord,
        ToolSurface,
        Memory,
        MemoryManifest,
        TaskRecord,
        SuiteManifest,
        SplitAssignment,
        ExperimentSpec,
        CellPlan,
        AnalysisRecord,
        ModelSnapshot,
        ComputeDecisionRecord,
    ] {
        if k.domain_tag() == tag {
            return Ok(k);
        }
    }
    Err(CodecError::TypeMismatch {
        member: "kind".to_string(),
        expected: "a RecordKind domain tag",
    })
}

fn supersede_reason_str(r: SupersedeReason) -> &'static str {
    match r {
        SupersedeReason::Edit => "edit",
        SupersedeReason::Revocation => "revocation",
        SupersedeReason::Expiry => "expiry",
        SupersedeReason::Migration => "migration",
        SupersedeReason::Consolidation => "consolidation",
        SupersedeReason::Fork => "fork",
    }
}

fn justification_kind_from_str(s: &str) -> Result<JustificationKind, CodecError> {
    for k in [
        JustificationKind::DeliveredMemory,
        JustificationKind::ExpandedHandle,
        JustificationKind::DeclaredInput,
        JustificationKind::DependencyStamp,
    ] {
        if k.as_str() == s {
            return Ok(k);
        }
    }
    Err(CodecError::TypeMismatch {
        member: "kind".to_string(),
        expected: "a JustificationKind spelling",
    })
}

fn supersede_claim_reason_from_str(s: &str) -> Result<SupersedeClaimReason, CodecError> {
    for r in [
        SupersedeClaimReason::Correction,
        SupersedeClaimReason::Replacement,
        SupersedeClaimReason::Compaction,
        SupersedeClaimReason::Migration,
        SupersedeClaimReason::Consolidation,
    ] {
        if r.as_str() == s {
            return Ok(r);
        }
    }
    Err(CodecError::TypeMismatch {
        member: "reason".to_string(),
        expected: "a SupersedeClaimReason spelling",
    })
}

fn revalidation_from_json(j: &Json) -> Result<Revalidation, CodecError> {
    match j {
        Json::Str(s) if s == "must_revalidate" => Ok(Revalidation::MustRevalidate),
        Json::Str(s) if s == "never" => Ok(Revalidation::Never),
        Json::Obj(m) => {
            reject_unknown(m, &["stale_ok"], "revalidation")?;
            let inner = expect_obj(
                m.get("stale_ok").ok_or(CodecError::MissingMember {
                    member: "stale_ok",
                    record: "revalidation",
                })?,
                "revalidation.stale_ok",
            )?;
            reject_unknown(inner, &["max_stale"], "revalidation.stale_ok")?;
            Ok(Revalidation::StaleOk {
                max_stale: int_at(inner, "max_stale", "revalidation.stale_ok")? as u64,
            })
        }
        _ => Err(CodecError::TypeMismatch {
            member: "revalidation".to_string(),
            expected: "must_revalidate | never | {stale_ok{max_stale}}",
        }),
    }
}

fn dep_from_json(j: &Json) -> Result<DependencyStamp, CodecError> {
    let m = expect_obj(j, "dependency_stamp")?;
    reject_unknown(
        m,
        &["kind", "ref", "stamp", "granularity", "validator_ref"],
        "dependency_stamp",
    )?;
    let kind = DependencyKind::parse(str_at(m, "kind", "dependency_stamp")?).ok_or(
        CodecError::TypeMismatch {
            member: "kind".to_string(),
            expected: "a DependencyKind spelling",
        },
    )?;
    let granularity = match str_at(m, "granularity", "dependency_stamp")? {
        "row" => Granularity::Row,
        "table" => Granularity::Table,
        other => {
            return Err(CodecError::TypeMismatch {
                member: "granularity".to_string(),
                expected: "row | table",
            })
            .map_err(|mut e| {
                if let CodecError::TypeMismatch { member, .. } = &mut e {
                    *member = format!("granularity {other}");
                }
                e
            })?
        }
    };
    Ok(DependencyStamp {
        kind,
        ref_: str_at(m, "ref", "dependency_stamp")?.to_string(),
        stamp: str_at(m, "stamp", "dependency_stamp")?.to_string(),
        granularity,
        validator_ref: opt_str(m, "validator_ref")?,
    })
}

/// `DependencyStamp` → the boundary member form (the client encodes the
/// same spelling).
pub fn dep_json(d: &DependencyStamp) -> Json {
    let mut m = vec![
        ("kind", Json::str(d.kind.as_str())),
        ("ref", Json::str(d.ref_.clone())),
        ("stamp", Json::str(d.stamp.clone())),
        ("granularity", Json::str(d.granularity.as_str())),
    ];
    if let Some(vr) = &d.validator_ref {
        m.push(("validator_ref", Json::str(vr.clone())));
    }
    Json::obj(m)
}

fn freshness_from_json(j: &Json) -> Result<Freshness, CodecError> {
    let m = expect_obj(j, "freshness")?;
    reject_unknown(m, &["valid_until", "max_age"], "freshness")?;
    if let Some(u) = m.get("valid_until") {
        return match u {
            Json::Int(i) => Ok(Freshness::ValidUntil(*i as u64)),
            _ => Err(CodecError::TypeMismatch {
                member: "valid_until".to_string(),
                expected: "integer",
            }),
        };
    }
    let inner = expect_obj(
        m.get("max_age").ok_or(CodecError::MissingMember {
            member: "max_age",
            record: "freshness",
        })?,
        "freshness.max_age",
    )?;
    reject_unknown(inner, &["n", "from"], "freshness.max_age")?;
    Ok(Freshness::MaxAge {
        max_age: int_at(inner, "n", "freshness.max_age")? as u64,
        from: int_at(inner, "from", "freshness.max_age")? as u64,
    })
}

fn invalidation_condition_from_json(j: &Json) -> Result<InvalidationCondition, CodecError> {
    let m = expect_obj(j, "invalidation_condition")?;
    reject_unknown(m, &["kind", "scope", "name"], "invalidation_condition")?;
    match str_at(m, "kind", "invalidation_condition")? {
        "dependency_changed" => Ok(InvalidationCondition::DependencyChanged),
        "ttl_elapsed" => Ok(InvalidationCondition::TtlElapsed),
        "validator_fails" => Ok(InvalidationCondition::ValidatorFails),
        "superseded" => Ok(InvalidationCondition::Superseded),
        "scope_ended" => Ok(InvalidationCondition::ScopeEnded(scope_at(m, "scope")?)),
        "replacement_published" => Ok(InvalidationCondition::ReplacementPublished(
            str_at(m, "name", "invalidation_condition")?.to_string(),
        )),
        _ => Err(CodecError::TypeMismatch {
            member: "kind".to_string(),
            expected: "an InvalidationCondition kind",
        }),
    }
}

fn contract_from_json(j: &Json) -> Result<InvalidationContract, CodecError> {
    let m = expect_obj(j, "invalidation_contract")?;
    reject_unknown(
        m,
        &[
            "dependencies",
            "cache_hint",
            "validator_ref",
            "freshness",
            "invalidation_condition",
            "revalidation",
        ],
        "invalidation_contract",
    )?;
    let mut dependencies = Vec::new();
    if m.get("dependencies").is_some() {
        for d in arr_at(m, "dependencies", "invalidation_contract")? {
            dependencies.push(dep_from_json(d)?);
        }
    }
    let cache_hint = match m.get("cache_hint") {
        Some(Json::Str(s)) => CacheHint::parse(s).ok_or(CodecError::TypeMismatch {
            member: "cache_hint".to_string(),
            expected: "a CacheHint spelling",
        })?,
        Some(Json::Null) | None => CacheHint::Cacheable,
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "cache_hint".to_string(),
                expected: "string",
            })
        }
    };
    let revalidation = match m.get("revalidation") {
        Some(j) => revalidation_from_json(j)?,
        None => Revalidation::MustRevalidate,
    };
    Ok(InvalidationContract {
        dependencies,
        cache_hint,
        validator_ref: opt_str(m, "validator_ref")?,
        freshness: m.get("freshness").map(freshness_from_json).transpose()?,
        invalidation_condition: m
            .get("invalidation_condition")
            .map(invalidation_condition_from_json)
            .transpose()?,
        revalidation,
    })
}

/// `VersionedRef` → the boundary member form.
pub fn vref_json(vr: &VersionedRef) -> Json {
    let mut m = vec![
        ("kind", Json::str(vr.kind.domain_tag())),
        ("version_id", Json::str(vr.version_id.clone())),
        ("provenance", vr.provenance.to_json()),
    ];
    if let Some(s) = &vr.semantic_id {
        m.push(("semantic_id", Json::str(s.clone())));
    }
    if let Some(n) = &vr.name {
        let mut nm = vec![
            ("namespace", Json::str(n.namespace.clone())),
            ("name", Json::str(n.name.clone())),
        ];
        if let Some(l) = &n.label {
            nm.push(("label", Json::str(l.clone())));
        }
        m.push(("name", Json::obj(nm)));
    }
    if let Some(r) = &vr.resolved_from {
        let mut rm = vec![
            ("namespace", Json::str(r.namespace.clone())),
            ("name", Json::str(r.name.clone())),
        ];
        if let Some(l) = &r.label {
            rm.push(("label", Json::str(l.clone())));
        }
        m.push(("resolved_from", Json::obj(rm)));
    }
    if let Some(s) = &vr.supersedes {
        m.push(("supersedes", Json::str(s.clone())));
    }
    Json::obj(m)
}

fn vref_from_json(j: &Json) -> Result<VersionedRef, CodecError> {
    let m = expect_obj(j, "versioned_ref")?;
    reject_unknown(
        m,
        &[
            "kind",
            "version_id",
            "semantic_id",
            "name",
            "resolved_from",
            "supersedes",
            "provenance",
        ],
        "versioned_ref",
    )?;
    let name = match m.get("name") {
        Some(Json::Obj(nm)) => {
            reject_unknown(nm, &["namespace", "name", "label"], "versioned_ref.name")?;
            Some(Name {
                namespace: str_at(nm, "namespace", "versioned_ref.name")?.to_string(),
                name: str_at(nm, "name", "versioned_ref.name")?.to_string(),
                label: opt_str(nm, "label")?,
            })
        }
        Some(Json::Null) | None => None,
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "name".to_string(),
                expected: "object",
            })
        }
    };
    let resolved_from = match m.get("resolved_from") {
        Some(Json::Obj(rm)) => {
            reject_unknown(
                rm,
                &["namespace", "name", "label"],
                "versioned_ref.resolved_from",
            )?;
            Some(NameSelector {
                namespace: str_at(rm, "namespace", "versioned_ref.resolved_from")?.to_string(),
                name: str_at(rm, "name", "versioned_ref.resolved_from")?.to_string(),
                label: opt_str(rm, "label")?,
            })
        }
        Some(Json::Null) | None => None,
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "resolved_from".to_string(),
                expected: "object",
            })
        }
    };
    let provenance =
        ProvenanceRecord::from_json(m.get("provenance").ok_or(CodecError::MissingMember {
            member: "provenance",
            record: "versioned_ref",
        })?)
        .map_err(|_| CodecError::TypeMismatch {
            member: "provenance".to_string(),
            expected: "a ProvenanceRecord",
        })?;
    Ok(VersionedRef {
        kind: record_kind_from_tag(str_at(m, "kind", "versioned_ref")?)?,
        version_id: str_at(m, "version_id", "versioned_ref")?.to_string(),
        semantic_id: opt_str(m, "semantic_id")?,
        name,
        resolved_from,
        supersedes: opt_str(m, "supersedes")?,
        provenance,
        idp: hh_identity::idp::IDP_1.idp_id,
    })
}

fn justification_from_json(j: &Json) -> Result<Justification, CodecError> {
    let m = expect_obj(j, "justification")?;
    reject_unknown(m, &["kind", "ref", "at"], "justification")?;
    let at = str_at(m, "at", "justification")?;
    let (run_id, event_id) = at.split_once(':').ok_or(CodecError::TypeMismatch {
        member: "at".to_string(),
        expected: "run_id:event_id",
    })?;
    Ok(Justification {
        kind: justification_kind_from_str(str_at(m, "kind", "justification")?)?,
        ref_: vref_from_json(m.get("ref").ok_or(CodecError::MissingMember {
            member: "ref",
            record: "justification",
        })?)?,
        at: EventRef {
            run_id: run_id.to_string(),
            event_id: event_id.to_string(),
        },
    })
}

/// `Justification` → the boundary member form.
pub fn justification_json(j: &Justification) -> Json {
    Json::obj([
        ("kind", Json::str(j.kind.as_str())),
        ("ref", vref_json(&j.ref_)),
        ("at", Json::str(crate::codec::event_ref_str(&j.at))),
    ])
}

fn validity_from_json(j: &Json) -> Result<Validity, CodecError> {
    let m = expect_obj(j, "validity")?;
    reject_unknown(m, &["from", "until", "condition"], "validity")?;
    Ok(Validity {
        from: int_at(m, "from", "validity")? as u64,
        until: opt_int(m, "until")?,
        condition: opt_str(m, "condition")?,
    })
}

fn subject_key_json(sk: &crate::vocab::SubjectKey) -> Json {
    Json::obj([
        ("schema_ref", Json::str(sk.schema_ref.clone())),
        ("key", Json::str(sk.key.clone())),
    ])
}

fn subject_key_from_json(j: &Json) -> Result<crate::vocab::SubjectKey, CodecError> {
    let m = expect_obj(j, "subject_key")?;
    reject_unknown(m, &["schema_ref", "key"], "subject_key")?;
    Ok(crate::vocab::SubjectKey {
        schema_ref: str_at(m, "schema_ref", "subject_key")?.to_string(),
        key: str_at(m, "key", "subject_key")?.to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// The record codec
// ─────────────────────────────────────────────────────────────────────────────

/// `MemoryVersion` → the boundary encoding (`body_json` plus the members
/// the body deliberately hashes around — `version_id`, `contract`,
/// `validity`, `supersedes_claim`, `conflict_set_ref`,
/// `validator_endorsed`; the members the caller checks the contract's
/// invariants against).
pub fn version_json(v: &MemoryVersion) -> Json {
    let Json::Obj(mut m) = v.body_json() else {
        unreachable!("body_json is an object")
    };
    m.insert("version_id".to_string(), Json::str(v.version_id.clone()));
    if let Some(vv) = &v.validity {
        m.insert("validity".to_string(), validity_json(vv));
    }
    if let Some(sc) = &v.supersedes_claim {
        m.insert(
            "supersedes_claim".to_string(),
            Json::obj([
                ("version_id", Json::str(sc.version_id.clone())),
                ("reason", Json::str(sc.reason.as_str())),
            ]),
        );
    }
    if let Some(cr) = &v.conflict_set_ref {
        m.insert("conflict_set_ref".to_string(), Json::str(cr.clone()));
    }
    m.insert(
        "validator_endorsed".to_string(),
        Json::Bool(v.validator_endorsed),
    );
    // The boundary record carries the *full* ref encodings where the body
    // renders the coordinates (`kind-tag:id`) for hashing — a client
    // reconstructs `VersionedRef`/`Justification` from these members
    // (`version_from_json`), so they must be the decodable spellings.
    m.insert(
        "declared_inputs".to_string(),
        Json::Arr(v.declared_inputs.iter().map(vref_json).collect()),
    );
    m.insert(
        "justifications".to_string(),
        Json::Arr(v.justifications.iter().map(justification_json).collect()),
    );
    Json::Obj(m)
}

/// `MemoryDraft` decode — the `put` input (strict).
pub fn draft_from_json(j: &Json) -> Result<MemoryDraft, CodecError> {
    let m = expect_obj(j, "memory_draft")?;
    reject_unknown(
        m,
        &[
            "kind",
            "subject_key",
            "content",
            "contract",
            "scope",
            "declared_inputs",
            "justifications",
            "supersedes",
            "validity",
            "provenance",
            "semantic_id",
            "validator_endorsed",
        ],
        "memory_draft",
    )?;
    let kind =
        MemoryKind::parse(str_at(m, "kind", "memory_draft")?).ok_or(CodecError::TypeMismatch {
            member: "kind".to_string(),
            expected: "a MemoryKind spelling",
        })?;
    let content = {
        let cm = expect_obj(
            m.get("content").ok_or(CodecError::MissingMember {
                member: "content",
                record: "memory_draft",
            })?,
            "memory_draft.content",
        )?;
        reject_unknown(cm, &["text", "structured", "owner"], "memory_draft.content")?;
        match (cm.get("text"), cm.get("structured")) {
            (Some(Json::Str(t)), None) => {
                let owner = opt_str(cm, "owner")?.unwrap_or_else(|| "writer".to_string());
                let prov = match m.get("provenance") {
                    Some(p) => {
                        ProvenanceRecord::from_json(p).map_err(|_| CodecError::TypeMismatch {
                            member: "provenance".to_string(),
                            expected: "a ProvenanceRecord",
                        })?
                    }
                    None => {
                        return Err(CodecError::MissingMember {
                            member: "provenance",
                            record: "memory_draft",
                        })
                    }
                };
                MemoryContent::Text(Box::new(Text::new(t.clone(), owner, prov)))
            }
            (None, Some(s)) => MemoryContent::Structured(s.clone()),
            _ => {
                return Err(CodecError::TypeMismatch {
                    member: "content".to_string(),
                    expected: "{text} xor {structured}",
                })
            }
        }
    };
    let mut declared_inputs = Vec::new();
    if let Some(Json::Arr(a)) = m.get("declared_inputs") {
        for v in a {
            declared_inputs.push(vref_from_json(v)?);
        }
    }
    let mut justifications = Vec::new();
    if let Some(Json::Arr(a)) = m.get("justifications") {
        for v in a {
            justifications.push(justification_from_json(v)?);
        }
    }
    let supersedes = match m.get("supersedes") {
        Some(Json::Obj(sm)) => {
            reject_unknown(sm, &["version_id", "reason"], "memory_draft.supersedes")?;
            Some(SupersedeClaim {
                version_id: str_at(sm, "version_id", "memory_draft.supersedes")?.to_string(),
                reason: supersede_claim_reason_from_str(str_at(
                    sm,
                    "reason",
                    "memory_draft.supersedes",
                )?)?,
            })
        }
        Some(Json::Null) | None => None,
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "supersedes".to_string(),
                expected: "object",
            })
        }
    };
    Ok(MemoryDraft {
        kind,
        subject_key: m
            .get("subject_key")
            .map(subject_key_from_json)
            .transpose()?,
        content,
        contract: m.get("contract").map(contract_from_json).transpose()?,
        scope: scope_at(m, "scope")?,
        declared_inputs,
        justifications,
        supersedes,
        validity: m.get("validity").map(validity_from_json).transpose()?,
        provenance: m
            .get("provenance")
            .map(|p| {
                ProvenanceRecord::from_json(p).map_err(|_| CodecError::TypeMismatch {
                    member: "provenance".to_string(),
                    expected: "a ProvenanceRecord",
                })
            })
            .transpose()?,
        semantic_id: opt_str(m, "semantic_id")?,
        validator_endorsed: m
            .get("validator_endorsed")
            .map(|_| bool_at(m, "validator_endorsed", "memory_draft"))
            .transpose()?
            .unwrap_or(false),
    })
}

/// `WriteContext` decode — the `put` context.
pub fn write_ctx_from_json(j: &Json) -> Result<WriteContext, CodecError> {
    let m = expect_obj(j, "write_context")?;
    reject_unknown(
        m,
        &["context_label", "lease_generation", "at_seq", "run_id"],
        "write_context",
    )?;
    let label = label_from_json(m.get("context_label").ok_or(CodecError::MissingMember {
        member: "context_label",
        record: "write_context",
    })?)?;
    Ok(WriteContext {
        context_label: label,
        lease_generation: int_at(m, "lease_generation", "write_context")? as u64,
        at_seq: int_at(m, "at_seq", "write_context")? as u64,
        run_id: str_at(m, "run_id", "write_context")?.to_string(),
    })
}

fn binding_json(b: &NameBinding) -> Json {
    let mut m = vec![
        ("scope", Json::str(b.scope.as_str())),
        ("name", Json::str(b.name.clone())),
        ("version_id", Json::str(b.version_id.clone())),
        ("bound_at", Json::Int(b.bound_at as i64)),
        ("reason", Json::str(b.reason.clone())),
    ];
    if let Some(s) = &b.supersedes {
        m.push(("supersedes", Json::str(s.clone())));
    }
    Json::obj(m)
}

fn conflict_json(c: &ConflictSet) -> Json {
    let resolution = match &c.resolution {
        ConflictResolution::Superseded { head } => {
            Json::obj([("superseded", Json::str(head.clone()))])
        }
        ConflictResolution::Coexist => Json::str("coexist"),
        ConflictResolution::Withheld => Json::str("withheld"),
        ConflictResolution::Escalated { principal_ref } => {
            Json::obj([("escalated", Json::str(principal_ref.clone()))])
        }
    };
    let mut m = vec![
        ("conflict_set_id", Json::str(c.conflict_set_id.clone())),
        ("subject_key", subject_key_json(&c.subject_key)),
        (
            "members",
            Json::Arr(c.members.iter().map(|x| Json::str(x.clone())).collect()),
        ),
        ("detector", Json::str(c.detector.clone())),
        ("resolution", resolution),
    ];
    if let Some(e) = &c.escalated_to {
        m.push(("escalated_to", Json::str(e.clone())));
    }
    Json::obj(m)
}

fn put_outcome_json(o: &PutOutcome) -> Json {
    let mut m = vec![
        ("version", version_json(&o.version)),
        (
            "event",
            Json::obj([
                ("class", Json::str(o.event.0.clone())),
                ("payload", o.event.1.clone()),
            ]),
        ),
    ];
    if let Some(c) = &o.conflict {
        m.push(("conflict", conflict_json(c)));
    }
    Json::obj(m)
}

fn manifest_json(mf: &MemoryManifest) -> Json {
    Json::obj([
        ("manifest_id", Json::str(mf.manifest_id.clone())),
        ("scope", Json::str(mf.scope.as_str())),
        (
            "entries",
            Json::Obj(
                mf.entries
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
        ("at_seq", Json::Int(mf.at_seq as i64)),
    ])
}

fn resolve_json(o: &ResolveOutcome) -> Json {
    match o {
        ResolveOutcome::Live { version } => Json::obj([("live", version_json(version))]),
        ResolveOutcome::Unservable { version_id, state } => Json::obj([(
            "unservable",
            Json::obj([
                ("version_id", Json::str(version_id.clone())),
                ("state", Json::str(state.as_str())),
            ]),
        )]),
        ResolveOutcome::Annotated { version, state } => Json::obj([(
            "annotated",
            Json::obj([
                ("version", version_json(version)),
                ("state", Json::str(state.as_str())),
            ]),
        )]),
    }
}

fn rev_record_json(r: &RevocationRecord) -> Json {
    let mut m = vec![
        ("kind", Json::str(r.kind.domain_tag())),
        ("revokes", Json::str(r.revokes.clone())),
        ("reason", Json::str(supersede_reason_str(r.reason))),
        ("authority", Json::str(render_origin(&r.authority))),
        ("provenance", r.provenance.to_json()),
    ];
    if let Some(u) = r.validity_until {
        m.push(("validity_until", Json::Int(u as i64)));
    }
    if let Some(rep) = &r.replacement {
        m.push(("replacement", Json::str(rep.clone())));
    }
    Json::obj(m)
}

fn memory_revocation_json(r: &MemoryRevocation) -> Json {
    let mut m = vec![
        ("version_id", Json::str(r.version_id.clone())),
        ("reason", Json::str(r.reason.as_str())),
        ("revoker", r.revoker.to_json()),
        ("at_seq", Json::Int(r.at_seq as i64)),
    ];
    if let Some(rep) = &r.replacement {
        m.push(("replacement", Json::str(rep.clone())));
    }
    Json::obj(m)
}

fn edge_json(e: &SupersedesEdge) -> Json {
    Json::obj([
        ("newer", Json::str(e.newer.clone())),
        ("older", Json::str(e.older.clone())),
        ("reason", Json::str(supersede_reason_str(e.reason))),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────
// The refusal envelope — typed domain refusals ride inside outputs.
// ─────────────────────────────────────────────────────────────────────────────

/// `MemoryError` → the structured refusal payload (the `refusal.error`
/// member — `{variant, …fields}`; `memory_error_from_json` is the mirror).
fn memory_error_json(e: &crate::memory::MemoryError) -> Json {
    use crate::memory::MemoryError as E;
    let (variant, mut fields): (&'static str, Vec<(&'static str, Json)>) = match e {
        E::MissingProvenance => ("MissingProvenance", vec![]),
        E::MissingContract { scope } => (
            "MissingContract",
            vec![("scope", Json::str(scope.as_str()))],
        ),
        E::ScopeCeilingExceeded { scope, writer } => (
            "ScopeCeilingExceeded",
            vec![
                ("scope", Json::str(scope.as_str())),
                ("writer", Json::str(writer.clone())),
            ],
        ),
        E::Fenced {
            scope,
            expected,
            got,
        } => (
            "Fenced",
            vec![
                ("scope", Json::str(scope.as_str())),
                ("expected", Json::Int(*expected as i64)),
                ("got", Json::Int(*got as i64)),
            ],
        ),
        E::UnknownVersion { version_id } => (
            "UnknownVersion",
            vec![("version_id", Json::str(version_id.clone()))],
        ),
        E::UnknownName { scope, name } => (
            "UnknownName",
            vec![
                ("scope", Json::str(scope.as_str())),
                ("name", Json::str(name.clone())),
            ],
        ),
        E::CycleDetected { newer, older } => (
            "CycleDetected",
            vec![
                ("newer", Json::str(newer.clone())),
                ("older", Json::str(older.clone())),
            ],
        ),
        E::KindMismatch { newer, older } => (
            "KindMismatch",
            vec![
                ("newer", Json::str(newer.as_str())),
                ("older", Json::str(older.as_str())),
            ],
        ),
        E::AuthorityInsufficient {
            new_authority,
            old_authority,
        } => (
            "AuthorityInsufficient",
            vec![
                ("new_authority", Json::str(new_authority.clone())),
                ("old_authority", Json::str(old_authority.clone())),
            ],
        ),
        E::TaintedAboveExternal { detail } => (
            "TaintedAboveExternal",
            vec![("detail", Json::str(detail.clone()))],
        ),
        E::NotPersistable { detail } => (
            "NotPersistable",
            vec![("detail", Json::str(detail.clone()))],
        ),
        E::UnvalidatedExternalDependency { ref_ } => (
            "UnvalidatedExternalDependency",
            vec![("ref", Json::str(ref_.clone()))],
        ),
        E::Channel { detail } => ("Channel", vec![("detail", Json::str(detail.clone()))]),
    };
    fields.push(("variant", Json::str(variant)));
    Json::obj(fields)
}

/// `LifecycleError` → the structured refusal payload (`Store` wraps the
/// memory-error payload — one nesting, same spelling).
fn lifecycle_error_json(e: &LifecycleError) -> Json {
    use LifecycleError as L;
    let (variant, mut fields): (&'static str, Vec<(&'static str, Json)>) = match e {
        L::AuthorityInsufficient { caller, target } => (
            "AuthorityInsufficient",
            vec![
                ("caller", Json::str(caller.clone())),
                ("target", Json::str(target.clone())),
            ],
        ),
        L::AlreadyRevoked { version_id } => (
            "AlreadyRevoked",
            vec![("version_id", Json::str(version_id.clone()))],
        ),
        L::SupersessionAuthorityInsufficient {
            new_authority,
            old_authority,
        } => (
            "SupersessionAuthorityInsufficient",
            vec![
                ("new_authority", Json::str(new_authority.clone())),
                ("old_authority", Json::str(old_authority.clone())),
            ],
        ),
        L::IllegitimateEndorsement { caller } => (
            "IllegitimateEndorsement",
            vec![("caller", Json::str(caller.clone()))],
        ),
        L::EndorserBelowTarget { endorser, target } => (
            "EndorserBelowTarget",
            vec![
                ("endorser", Json::str(endorser.clone())),
                ("target", Json::str(target.clone())),
            ],
        ),
        L::BasisNotAllowed { basis } => {
            ("BasisNotAllowed", vec![("basis", Json::str(basis.clone()))])
        }
        L::PromotionRefused { version_id, reason } => (
            "PromotionRefused",
            vec![
                ("version_id", Json::str(version_id.clone())),
                ("reason", Json::str(reason.clone())),
            ],
        ),
        L::Store(me) => ("Store", vec![("error", memory_error_json(me))]),
    };
    fields.push(("variant", Json::str(variant)));
    Json::obj(fields)
}

/// The refusal envelope — `{ok:false, refusal:{kind, code, detail, error}}`
/// where `kind ∈ {memory, lifecycle}` names the error domain and `error`
/// carries the structured variant the client decodes typed (never a bare
/// string — the refusal is data, not a fault).
fn refusal_memory(e: &crate::memory::MemoryError) -> Json {
    Json::obj([
        ("ok", Json::Bool(false)),
        (
            "refusal",
            Json::obj([
                ("kind", Json::str("memory")),
                ("code", Json::str(memory_error_code(e))),
                ("detail", Json::str(e.to_string())),
                ("error", memory_error_json(e)),
            ]),
        ),
    ])
}

fn refusal_lifecycle(e: &LifecycleError) -> Json {
    Json::obj([
        ("ok", Json::Bool(false)),
        (
            "refusal",
            Json::obj([
                ("kind", Json::str("lifecycle")),
                ("code", Json::str(lifecycle_error_code(e))),
                ("detail", Json::str(e.to_string())),
                ("error", lifecycle_error_json(e)),
            ]),
        ),
    ])
}

fn memory_error_code(e: &crate::memory::MemoryError) -> &'static str {
    use crate::memory::MemoryError as E;
    match e {
        E::MissingProvenance => "MissingProvenance",
        E::MissingContract { .. } => "MissingContract",
        E::ScopeCeilingExceeded { .. } => "ScopeCeilingExceeded",
        E::Fenced { .. } => "Fenced",
        E::UnknownVersion { .. } => "UnknownVersion",
        E::UnknownName { .. } => "UnknownName",
        E::CycleDetected { .. } => "CycleDetected",
        E::KindMismatch { .. } => "KindMismatch",
        E::AuthorityInsufficient { .. } => "AuthorityInsufficient",
        E::TaintedAboveExternal { .. } => "TaintedAboveExternal",
        E::NotPersistable { .. } => "NotPersistable",
        E::UnvalidatedExternalDependency { .. } => "UnvalidatedExternalDependency",
        E::Channel { .. } => "Channel",
    }
}

fn lifecycle_error_code(e: &LifecycleError) -> &'static str {
    match e {
        LifecycleError::AuthorityInsufficient { .. } => "AuthorityInsufficient",
        LifecycleError::AlreadyRevoked { .. } => "AlreadyRevoked",
        LifecycleError::SupersessionAuthorityInsufficient { .. } => {
            "SupersessionAuthorityInsufficient"
        }
        LifecycleError::IllegitimateEndorsement { .. } => "IllegitimateEndorsement",
        LifecycleError::EndorserBelowTarget { .. } => "EndorserBelowTarget",
        LifecycleError::BasisNotAllowed { .. } => "BasisNotAllowed",
        LifecycleError::PromotionRefused { .. } => "PromotionRefused",
        LifecycleError::Store(e) => memory_error_code(e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The op dispatch — every MemoryStorePort member, one spelling.
// ─────────────────────────────────────────────────────────────────────────────

/// `dispatch(store, operation, inputs)` — the `memory_store` contract op
/// surface over canonical JSON (one codec shared by the in-process port
/// and the `hh-memory-store` plugin — CC7). Unknown operations/members/
/// shapes are `CodecError` (decode-side); domain refusals return the typed
/// `{ok:false, refusal:{code, detail}}` output.
pub fn dispatch(
    store: &mut dyn MemoryStorePort,
    operation: &str,
    inputs: &[Json],
) -> Result<Vec<Json>, CodecError> {
    let one = |j: Json| vec![j];
    let input = |i: usize| -> Result<&Json, CodecError> {
        inputs.get(i).ok_or(CodecError::MissingMember {
            member: "inputs",
            record: "memory_store.invoke",
        })
    };
    match operation {
        "put" => {
            let draft = draft_from_json(input(0)?)?;
            let ctx = write_ctx_from_json(input(1)?)?;
            match store.put(draft, &ctx) {
                Ok(o) => Ok(one(put_outcome_json(&o))),
                Err(e) => Ok(one(refusal_memory(&e))),
            }
        }
        "bind" => {
            let m = expect_obj(input(0)?, "bind")?;
            reject_unknown(
                m,
                &[
                    "scope",
                    "name",
                    "version_id",
                    "supersedes",
                    "reason",
                    "at_seq",
                ],
                "bind",
            )?;
            match store.bind(
                scope_at(m, "scope")?,
                str_at(m, "name", "bind")?,
                str_at(m, "version_id", "bind")?,
                opt_str(m, "supersedes")?.as_deref(),
                str_at(m, "reason", "bind")?,
                int_at(m, "at_seq", "bind")? as u64,
            ) {
                Ok(b) => Ok(one(binding_json(&b))),
                Err(e) => Ok(one(refusal_memory(&e))),
            }
        }
        "revoke" => {
            let m = expect_obj(input(0)?, "revoke")?;
            reject_unknown(
                m,
                &["version_id", "reason", "revoker", "replacement", "at_seq"],
                "revoke",
            )?;
            let revoker =
                ProvenanceRecord::from_json(m.get("revoker").ok_or(CodecError::MissingMember {
                    member: "revoker",
                    record: "revoke",
                })?)
                .map_err(|_| CodecError::TypeMismatch {
                    member: "revoker".to_string(),
                    expected: "a ProvenanceRecord",
                })?;
            let reason = RevocationReason::parse(str_at(m, "reason", "revoke")?).ok_or(
                CodecError::TypeMismatch {
                    member: "reason".to_string(),
                    expected: "a RevocationReason spelling",
                },
            )?;
            match store.revoke(
                str_at(m, "version_id", "revoke")?,
                reason,
                &revoker,
                opt_str(m, "replacement")?,
                int_at(m, "at_seq", "revoke")? as u64,
            ) {
                Ok(r) => Ok(one(rev_record_json(&r))),
                Err(e) => Ok(one(refusal_lifecycle(&e))),
            }
        }
        "resolve" => {
            let m = expect_obj(input(0)?, "resolve")?;
            reject_unknown(m, &["scope", "name", "version_id", "mode"], "resolve")?;
            let mode = match str_at(m, "mode", "resolve")? {
                "execute" => ResolveMode::Execute,
                "audit" => ResolveMode::Audit,
                "reproduce" => ResolveMode::Reproduce,
                _ => {
                    return Err(CodecError::TypeMismatch {
                        member: "mode".to_string(),
                        expected: "execute | audit | reproduce",
                    })
                }
            };
            match store.resolve(
                scope_at(m, "scope")?,
                opt_str(m, "name")?.as_deref(),
                opt_str(m, "version_id")?.as_deref(),
                mode,
            ) {
                Ok(o) => Ok(one(resolve_json(&o))),
                Err(e) => Ok(one(refusal_memory(&e))),
            }
        }
        "manifest" => {
            let m = expect_obj(input(0)?, "manifest")?;
            reject_unknown(m, &["scope", "at"], "manifest")?;
            Ok(one(manifest_json(&store.manifest(
                scope_at(m, "scope")?,
                int_at(m, "at", "manifest")? as u64,
            ))))
        }
        "enumerate" => {
            let m = expect_obj(input(0)?, "enumerate")?;
            reject_unknown(m, &["layer"], "enumerate")?;
            let layer =
                Layer::parse(str_at(m, "layer", "enumerate")?).ok_or(CodecError::TypeMismatch {
                    member: "layer".to_string(),
                    expected: "A | E | P | S",
                })?;
            Ok(vec![Json::Arr(
                store
                    .enumerate(layer)
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            )])
        }
        "stale_candidates" => {
            let m = expect_obj(input(0)?, "stale_candidates")?;
            reject_unknown(m, &["scope"], "stale_candidates")?;
            Ok(vec![Json::Arr(
                store
                    .stale_candidates(scope_at(m, "scope")?)
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            )])
        }
        "reads_of" => {
            let m = expect_obj(input(0)?, "reads_of")?;
            reject_unknown(m, &["version_id"], "reads_of")?;
            Ok(vec![Json::Arr(
                store
                    .reads_of(str_at(m, "version_id", "reads_of")?)
                    .iter()
                    .map(|s| Json::Int(*s as i64))
                    .collect(),
            )])
        }
        "writes_by" => {
            let m = expect_obj(input(0)?, "writes_by")?;
            reject_unknown(m, &["writer_ref"], "writes_by")?;
            Ok(vec![Json::Arr(
                store
                    .writes_by(str_at(m, "writer_ref", "writes_by")?)
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            )])
        }
        "record_read" => {
            let m = expect_obj(input(0)?, "record_read")?;
            reject_unknown(m, &["version_id", "seq"], "record_read")?;
            store.record_read(
                str_at(m, "version_id", "record_read")?,
                int_at(m, "seq", "record_read")? as u64,
            );
            Ok(one(Json::obj([("ok", Json::Bool(true))])))
        }
        "set_stamp" => {
            let m = expect_obj(input(0)?, "set_stamp")?;
            reject_unknown(m, &["dep_ref", "stamp"], "set_stamp")?;
            store.set_stamp(
                str_at(m, "dep_ref", "set_stamp")?,
                str_at(m, "stamp", "set_stamp")?,
            );
            Ok(one(Json::obj([("ok", Json::Bool(true))])))
        }
        "set_validator_verdict" => {
            let m = expect_obj(input(0)?, "set_validator_verdict")?;
            reject_unknown(
                m,
                &["version_id", "validator_ref", "ok"],
                "set_validator_verdict",
            )?;
            store.set_validator_verdict(
                str_at(m, "version_id", "set_validator_verdict")?,
                str_at(m, "validator_ref", "set_validator_verdict")?,
                bool_at(m, "ok", "set_validator_verdict")?,
            );
            Ok(one(Json::obj([("ok", Json::Bool(true))])))
        }
        "mark_scope_ended" => {
            let m = expect_obj(input(0)?, "mark_scope_ended")?;
            reject_unknown(m, &["scope"], "mark_scope_ended")?;
            store.mark_scope_ended(scope_at(m, "scope")?);
            Ok(one(Json::obj([("ok", Json::Bool(true))])))
        }
        "publish_replacement" => {
            let m = expect_obj(input(0)?, "publish_replacement")?;
            reject_unknown(m, &["name"], "publish_replacement")?;
            store.publish_replacement(str_at(m, "name", "publish_replacement")?);
            Ok(one(Json::obj([("ok", Json::Bool(true))])))
        }
        "version" => {
            let m = expect_obj(input(0)?, "version")?;
            reject_unknown(m, &["version_id"], "version")?;
            match store.version(str_at(m, "version_id", "version")?) {
                Some(v) => Ok(one(version_json(&v))),
                None => Ok(vec![Json::Null]),
            }
        }
        "version_order" => Ok(vec![Json::Arr(
            store
                .version_order()
                .iter()
                .map(|s| Json::str(s.clone()))
                .collect(),
        )]),
        "edges" => Ok(vec![Json::Arr(
            store.edges().iter().map(edge_json).collect(),
        )]),
        "revocations" => Ok(vec![Json::Arr(
            store
                .revocations()
                .iter()
                .map(memory_revocation_json)
                .collect(),
        )]),
        "applied_seq" => Ok(vec![Json::Int(store.applied_seq() as i64)]),
        "drain_events" => Ok(vec![Json::Arr(
            store
                .drain_events()
                .iter()
                .map(|(c, p)| Json::obj([("class", Json::str(c.clone())), ("payload", p.clone())]))
                .collect(),
        )]),
        "take_lease" => {
            let m = expect_obj(input(0)?, "take_lease")?;
            reject_unknown(m, &["scope", "holder"], "take_lease")?;
            Ok(vec![Json::Int(
                store.take_lease(scope_at(m, "scope")?, str_at(m, "holder", "take_lease")?) as i64,
            )])
        }
        "lease" => {
            let m = expect_obj(input(0)?, "lease")?;
            reject_unknown(m, &["scope"], "lease")?;
            Ok(vec![Json::Int(store.lease(scope_at(m, "scope")?) as i64)])
        }
        _ => Err(CodecError::BadMember {
            member: operation.to_string(),
            record: "memory_store.operation",
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The client side — `RemoteStore` binds `MemoryStorePort` over any channel
// that delivers `(operation, inputs[]) → outputs[]` (the `hh-memory-store`
// plugin's `plugin_abi/1` session is one such channel; an in-process
// `dispatch` loopback is another — the conformance battery runs against both
// interchangeably, T-LCD-12).
//
// The binding model is *deterministic mirroring*: `MemoryStore` is a pure
// fold over its ops, so the client applies every mutation to a local
// `MemoryStore` mirror **and** to the remote store through the channel. The
// two sides must agree record-for-record (same `version_id`s — content
// addressing makes the fold injective); a disagreement is a
// `ChannelFault::Divergence`, never a silent adoption. `&self` reads
// (`resolve`, `manifest`, `version`, `enumerate`, …) are served from the
// mirror — the mirror *is* the client's coherent projection of everything it
// has written, so reads never touch the wire and never invent answers.
// `&mut self` ops go remote-first: a domain refusal returns the typed error
// and leaves the mirror untouched (both sides refused identically); a
// success is folded into the mirror and equality-checked.
//
// A channel fault on a `()`-returning mutator cannot ride the typed error
// path, so it is *latched* (`fault()`/`take_fault()`): the op still applies
// to the mirror (the client's intent is preserved and queries stay coherent)
// and every subsequent fallible op reports `MemoryError::Channel` until the
// fault is acknowledged — observable, never silent, never a panic.
// ─────────────────────────────────────────────────────────────────────────────

/// A transport-level fault — the channel failed before or after a domain
/// answer existed (process exit, framing break, malformed outputs, or a
/// mirror/remote divergence). Distinct from a domain refusal, which decodes
/// typed inside the outputs and surfaces as `Err(MemoryError)`/
/// `Err(LifecycleError)` on the fallible port methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelFault {
    /// The transport itself failed (session broken, spawn refused).
    Transport(String),
    /// The outputs failed strict decode — an ABI violation, never coerced.
    Decode(CodecError),
    /// A refusal arrived for an op whose domain is not the one the caller
    /// expected, or a refusal payload failed decode.
    BadRefusal(String),
    /// The remote answer and the deterministic mirror disagreed — the two
    /// stores are not the same machine (protocol or pin violation).
    Divergence(String),
}

impl std::fmt::Display for ChannelFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelFault::Transport(m) => write!(f, "transport: {m}"),
            ChannelFault::Decode(e) => write!(f, "decode: {e}"),
            ChannelFault::BadRefusal(m) => write!(f, "refusal: {m}"),
            ChannelFault::Divergence(m) => write!(f, "divergence: {m}"),
        }
    }
}

impl std::error::Error for ChannelFault {}

impl From<CodecError> for ChannelFault {
    fn from(e: CodecError) -> ChannelFault {
        ChannelFault::Decode(e)
    }
}

/// The op transport — one `invoke` carries `(operation, inputs[])` to the
/// store and returns `outputs[]` verbatim (refusal envelopes included —
/// `RemoteStore` decodes them). Implementations: the `plugin_abi/1` session
/// adapter (`hh-memory-store`'s counterpart) and test loopbacks.
pub trait StoreChannel {
    /// `invoke(operation, inputs)` → the op's `outputs[]`.
    fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, ChannelFault>;
}

/// The `plugin_abi/1` envelope encoding for `memory_store` outputs (S4.16b;
/// ADR-0078 d2's out-of-process clause). A stored record *is* kernel data —
/// its members legitimately include `label` and `provenance.authority`
/// spellings the V1 inbound screen forbids as *envelope members* (a plugin
/// claiming authority or smuggling a label for the host to honour). The
/// contract therefore crosses the wire as **canonical-JSON bytes**: every
/// op output is `{result: "<canonical-json>"}`, one opaque string member
/// carrying the byte-identical document the store produced (CC1 — the same
/// bytes the store hashes). The screen then sees only the envelope (where
/// channel claims live); the record's authority members stay data inside
/// the string, decoded strictly on the kernel side — never re-interpreted
/// as channel authority.
///
/// Refusal envelopes (`{ok:false, refusal}`) carry only contract vocabulary
/// (codes, ids, scopes — no label/authority members) yet ride the same
/// `{result}` string for uniformity: one output spelling, both placements.
pub fn wrap_output(doc: &Json) -> Json {
    Json::obj([("result", Json::str(doc.to_canonical_string()))])
}

/// `wrap_output`'s mirror — after the host's V1 stamp is removed
/// (`hh_varhost::lower::unstamp_json`), a conforming output is exactly
/// `{result: "<canonical-json>"}`; anything else is an ABI fault.
pub fn unwrap_output(j: &Json) -> Result<Json, ChannelFault> {
    let m = expect_obj(j, "memory_store.output").map_err(ChannelFault::Decode)?;
    reject_unknown(m, &["result"], "memory_store.output").map_err(ChannelFault::Decode)?;
    let s = str_at(m, "result", "memory_store.output").map_err(ChannelFault::Decode)?;
    hh_wire::json::parse(s).map_err(|_| {
        ChannelFault::Decode(CodecError::TypeMismatch {
            member: "result".to_string(),
            expected: "a canonical JSON document",
        })
    })
}

/// `RemoteStore<C>` — the kernel-side `memory_store` binding: a
/// `MemoryStorePort` whose mutating ops lower to one `invoke` through `C`
/// and fold into a deterministic local mirror; reads serve the mirror.
pub struct RemoteStore<C: StoreChannel> {
    /// The deterministic projection of every op this client applied.
    mirror: crate::memory::MemoryStore,
    /// The transport.
    channel: C,
    /// The latched channel fault (`()`-returning ops' only honest surface).
    fault: Option<ChannelFault>,
}

impl<C: StoreChannel> RemoteStore<C> {
    /// Bind the channel over a `memory_store`-id mirror (the plugin's id).
    pub fn new(channel: C) -> RemoteStore<C> {
        RemoteStore {
            mirror: crate::memory::MemoryStore::new("memory_store"),
            channel,
            fault: None,
        }
    }

    /// Bind the channel over a mirror with an explicit store id (must equal
    /// the remote store's id for event payloads to compare equal).
    pub fn with_store_id(channel: C, store_id: impl Into<String>) -> RemoteStore<C> {
        RemoteStore {
            mirror: crate::memory::MemoryStore::new(store_id),
            channel,
            fault: None,
        }
    }

    /// Consume the port, returning the channel.
    pub fn into_inner(self) -> C {
        self.channel
    }

    /// The local projection (read-only — for inspection/tests).
    pub fn mirror(&self) -> &crate::memory::MemoryStore {
        &self.mirror
    }

    /// A channel fault a `()`-returning op could not report, when latched.
    pub fn fault(&self) -> Option<&ChannelFault> {
        self.fault.as_ref()
    }

    /// Acknowledge and clear the latched fault.
    pub fn take_fault(&mut self) -> Option<ChannelFault> {
        self.fault.take()
    }

    fn call(&mut self, op: &str, inputs: Vec<Json>) -> Result<Json, ChannelFault> {
        let mut outs = self.channel.invoke(op, inputs)?;
        if outs.len() != 1 {
            return Err(ChannelFault::Decode(CodecError::TypeMismatch {
                member: "outputs".to_string(),
                expected: "exactly one output",
            }));
        }
        Ok(outs.remove(0))
    }

    /// A `()`-returning op: apply to the mirror, then fire the remote op;
    /// a channel fault latches (the mirror's answer stands — the client saw
    /// the op apply locally; `fault()` reports the divergence).
    fn fire(&mut self, op: &str, inputs: Vec<Json>) {
        if self.fault.is_some() {
            return;
        }
        if let Err(f) = self.channel.invoke(op, inputs) {
            self.fault = Some(f);
        }
    }

    /// The latched fault, or the fallible op's typed refusal of it.
    fn latched(&mut self) -> Option<crate::memory::MemoryError> {
        self.fault.take().map(channel_to_memory)
    }
}

/// The `label_json` spelling's decoder — `{authority, taint[], readers}` with
/// `readers` spelled `"public"` | `{restricted: [participants]}`.
fn label_from_json(j: &Json) -> Result<Label, CodecError> {
    let m = expect_obj(j, "label")?;
    reject_unknown(m, &["authority", "taint", "readers"], "label")?;
    let authority = hh_provenance::AuthorityClass::parse(str_at(m, "authority", "label")?).ok_or(
        CodecError::TypeMismatch {
            member: "authority".to_string(),
            expected: "an AuthorityClass spelling",
        },
    )?;
    let mut taint = std::collections::BTreeSet::new();
    if let Some(Json::Arr(items)) = m.get("taint") {
        for t in items {
            match t {
                Json::Str(s) => {
                    taint.insert(hh_provenance::authority::TaintTag::parse(s).ok_or(
                        CodecError::TypeMismatch {
                            member: "taint[]".to_string(),
                            expected: "a TaintTag spelling",
                        },
                    )?);
                }
                _ => {
                    return Err(CodecError::TypeMismatch {
                        member: "taint[]".to_string(),
                        expected: "string",
                    })
                }
            }
        }
    }
    let readers = match m.get("readers") {
        Some(Json::Str(s)) if s == "public" => hh_provenance::authority::ReaderSet::Public,
        Some(Json::Obj(rm)) => {
            reject_unknown(rm, &["restricted"], "label.readers")?;
            let mut rs = std::collections::BTreeSet::new();
            for p in arr_at(rm, "restricted", "label.readers")? {
                match p {
                    Json::Str(s) => {
                        rs.insert(s.clone());
                    }
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: "restricted[]".to_string(),
                            expected: "string",
                        })
                    }
                }
            }
            hh_provenance::authority::ReaderSet::Restricted(rs)
        }
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "readers".to_string(),
                expected: "public | {restricted: [participants]}",
            })
        }
        None => hh_provenance::authority::ReaderSet::Public,
    };
    Ok(Label {
        authority,
        taint,
        readers,
    })
}

fn content_from_json(j: &Json, provenance: &ProvenanceRecord) -> Result<MemoryContent, CodecError> {
    let m = expect_obj(j, "memory_content")?;
    reject_unknown(m, &["text", "structured"], "memory_content")?;
    match (m.get("text"), m.get("structured")) {
        (Some(Json::Str(t)), None) => Ok(MemoryContent::Text(Box::new(Text::new(
            t.clone(),
            "writer",
            provenance.clone(),
        )))),
        (None, Some(s)) => Ok(MemoryContent::Structured(s.clone())),
        _ => Err(CodecError::TypeMismatch {
            member: "content".to_string(),
            expected: "{text} xor {structured}",
        }),
    }
}

/// `MemoryVersion` decode — the `version_json` mirror (strict).
pub fn version_from_json(j: &Json) -> Result<MemoryVersion, CodecError> {
    let m = expect_obj(j, "memory_version")?;
    reject_unknown(
        m,
        &[
            "version_id",
            "semantic_id",
            "kind",
            "subject_key",
            "content",
            "contract",
            "scope",
            "label",
            "provenance",
            "validity",
            "declared_inputs",
            "justifications",
            "created_at",
            "created_by",
            "supersedes_claim",
            "conflict_set_ref",
            "validator_endorsed",
        ],
        "memory_version",
    )?;
    let provenance =
        ProvenanceRecord::from_json(m.get("provenance").ok_or(CodecError::MissingMember {
            member: "provenance",
            record: "memory_version",
        })?)
        .map_err(|_| CodecError::TypeMismatch {
            member: "provenance".to_string(),
            expected: "a ProvenanceRecord",
        })?;
    let mut declared_inputs = Vec::new();
    if let Some(Json::Arr(a)) = m.get("declared_inputs") {
        for v in a {
            declared_inputs.push(vref_from_json(v)?);
        }
    }
    let mut justifications = Vec::new();
    if let Some(Json::Arr(a)) = m.get("justifications") {
        for v in a {
            justifications.push(justification_from_json(v)?);
        }
    }
    let supersedes_claim = match m.get("supersedes_claim") {
        Some(Json::Obj(sm)) => {
            reject_unknown(
                sm,
                &["version_id", "reason"],
                "memory_version.supersedes_claim",
            )?;
            Some(SupersedeClaim {
                version_id: str_at(sm, "version_id", "memory_version.supersedes_claim")?
                    .to_string(),
                reason: supersede_claim_reason_from_str(str_at(
                    sm,
                    "reason",
                    "memory_version.supersedes_claim",
                )?)?,
            })
        }
        Some(Json::Null) | None => None,
        Some(_) => {
            return Err(CodecError::TypeMismatch {
                member: "supersedes_claim".to_string(),
                expected: "object",
            })
        }
    };
    Ok(MemoryVersion {
        version_id: str_at(m, "version_id", "memory_version")?.to_string(),
        semantic_id: str_at(m, "semantic_id", "memory_version")?.to_string(),
        kind: MemoryKind::parse(str_at(m, "kind", "memory_version")?).ok_or(
            CodecError::TypeMismatch {
                member: "kind".to_string(),
                expected: "a MemoryKind spelling",
            },
        )?,
        subject_key: m
            .get("subject_key")
            .map(subject_key_from_json)
            .transpose()?,
        content: content_from_json(
            m.get("content").ok_or(CodecError::MissingMember {
                member: "content",
                record: "memory_version",
            })?,
            &provenance,
        )?,
        contract: contract_from_json(m.get("contract").ok_or(CodecError::MissingMember {
            member: "contract",
            record: "memory_version",
        })?)?,
        scope: scope_at(m, "scope")?,
        label: label_from_json(m.get("label").ok_or(CodecError::MissingMember {
            member: "label",
            record: "memory_version",
        })?)?,
        provenance,
        validity: m.get("validity").map(validity_from_json).transpose()?,
        declared_inputs,
        justifications,
        created_at: int_at(m, "created_at", "memory_version")? as u64,
        created_by: str_at(m, "created_by", "memory_version")?.to_string(),
        supersedes_claim,
        conflict_set_ref: opt_str(m, "conflict_set_ref")?,
        validator_endorsed: m
            .get("validator_endorsed")
            .map(|_| bool_at(m, "validator_endorsed", "memory_version"))
            .transpose()?
            .unwrap_or(false),
    })
}

fn conflict_from_json(j: &Json) -> Result<ConflictSet, CodecError> {
    let m = expect_obj(j, "conflict_set")?;
    reject_unknown(
        m,
        &[
            "conflict_set_id",
            "subject_key",
            "members",
            "detector",
            "resolution",
            "escalated_to",
        ],
        "conflict_set",
    )?;
    let resolution = match m.get("resolution").ok_or(CodecError::MissingMember {
        member: "resolution",
        record: "conflict_set",
    })? {
        Json::Str(s) if s == "coexist" => ConflictResolution::Coexist,
        Json::Str(s) if s == "withheld" => ConflictResolution::Withheld,
        Json::Obj(rm) => {
            reject_unknown(rm, &["superseded", "escalated"], "conflict_set.resolution")?;
            if let Some(Json::Str(h)) = rm.get("superseded") {
                ConflictResolution::Superseded { head: h.clone() }
            } else if let Some(Json::Str(p)) = rm.get("escalated") {
                ConflictResolution::Escalated {
                    principal_ref: p.clone(),
                }
            } else {
                return Err(CodecError::MissingMember {
                    member: "superseded|escalated",
                    record: "conflict_set.resolution",
                });
            }
        }
        _ => {
            return Err(CodecError::TypeMismatch {
                member: "resolution".to_string(),
                expected: "coexist | withheld | {superseded} | {escalated}",
            })
        }
    };
    let mut members = Vec::new();
    for x in arr_at(m, "members", "conflict_set")? {
        match x {
            Json::Str(s) => members.push(s.clone()),
            _ => {
                return Err(CodecError::TypeMismatch {
                    member: "members[]".to_string(),
                    expected: "string",
                })
            }
        }
    }
    Ok(ConflictSet {
        conflict_set_id: str_at(m, "conflict_set_id", "conflict_set")?.to_string(),
        subject_key: subject_key_from_json(m.get("subject_key").ok_or(
            CodecError::MissingMember {
                member: "subject_key",
                record: "conflict_set",
            },
        )?)?,
        members,
        detector: str_at(m, "detector", "conflict_set")?.to_string(),
        resolution,
        escalated_to: opt_str(m, "escalated_to")?,
    })
}

fn put_outcome_from_json(j: &Json) -> Result<PutOutcome, CodecError> {
    let m = expect_obj(j, "put_outcome")?;
    reject_unknown(m, &["version", "event", "conflict"], "put_outcome")?;
    let em = expect_obj(
        m.get("event").ok_or(CodecError::MissingMember {
            member: "event",
            record: "put_outcome",
        })?,
        "put_outcome.event",
    )?;
    reject_unknown(em, &["class", "payload"], "put_outcome.event")?;
    Ok(PutOutcome {
        version: version_from_json(m.get("version").ok_or(CodecError::MissingMember {
            member: "version",
            record: "put_outcome",
        })?)?,
        event: (
            str_at(em, "class", "put_outcome.event")?.to_string(),
            em.get("payload")
                .ok_or(CodecError::MissingMember {
                    member: "payload",
                    record: "put_outcome.event",
                })?
                .clone(),
        ),
        conflict: m.get("conflict").map(conflict_from_json).transpose()?,
    })
}

fn binding_from_json(j: &Json) -> Result<NameBinding, CodecError> {
    let m = expect_obj(j, "name_binding")?;
    reject_unknown(
        m,
        &[
            "scope",
            "name",
            "version_id",
            "bound_at",
            "reason",
            "supersedes",
        ],
        "name_binding",
    )?;
    Ok(NameBinding {
        scope: scope_at(m, "scope")?,
        name: str_at(m, "name", "name_binding")?.to_string(),
        version_id: str_at(m, "version_id", "name_binding")?.to_string(),
        bound_at: int_at(m, "bound_at", "name_binding")? as u64,
        supersedes: opt_str(m, "supersedes")?,
        reason: str_at(m, "reason", "name_binding")?.to_string(),
    })
}

/// `MemoryManifest` decode — the `manifest_json` mirror (strict).
pub fn manifest_from_json(j: &Json) -> Result<MemoryManifest, CodecError> {
    let m = expect_obj(j, "memory_manifest")?;
    reject_unknown(
        m,
        &["manifest_id", "scope", "entries", "at_seq"],
        "memory_manifest",
    )?;
    let mut entries = BTreeMap::new();
    match m.get("entries").ok_or(CodecError::MissingMember {
        member: "entries",
        record: "memory_manifest",
    })? {
        Json::Obj(em) => {
            for (k, v) in em {
                match v {
                    Json::Str(s) => {
                        entries.insert(k.clone(), s.clone());
                    }
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: format!("entries.{k}"),
                            expected: "string",
                        })
                    }
                }
            }
        }
        _ => {
            return Err(CodecError::TypeMismatch {
                member: "entries".to_string(),
                expected: "object",
            })
        }
    }
    Ok(MemoryManifest {
        manifest_id: str_at(m, "manifest_id", "memory_manifest")?.to_string(),
        scope: scope_at(m, "scope")?,
        entries,
        at_seq: int_at(m, "at_seq", "memory_manifest")? as u64,
    })
}

/// `ResolveOutcome` decode — the `resolve_json` mirror (strict).
pub fn resolve_from_json(j: &Json) -> Result<ResolveOutcome, CodecError> {
    let m = expect_obj(j, "resolve_outcome")?;
    reject_unknown(m, &["live", "unservable", "annotated"], "resolve_outcome")?;
    if let Some(v) = m.get("live") {
        return Ok(ResolveOutcome::Live {
            version: version_from_json(v)?,
        });
    }
    if let Some(u) = m.get("unservable") {
        let um = expect_obj(u, "resolve_outcome.unservable")?;
        reject_unknown(um, &["version_id", "state"], "resolve_outcome.unservable")?;
        return Ok(ResolveOutcome::Unservable {
            version_id: str_at(um, "version_id", "resolve_outcome.unservable")?.to_string(),
            state: LifecycleStateKind::parse(str_at(um, "state", "resolve_outcome.unservable")?)
                .ok_or(CodecError::TypeMismatch {
                    member: "state".to_string(),
                    expected: "a LifecycleStateKind spelling",
                })?,
        });
    }
    let a = m.get("annotated").ok_or(CodecError::MissingMember {
        member: "live|unservable|annotated",
        record: "resolve_outcome",
    })?;
    let am = expect_obj(a, "resolve_outcome.annotated")?;
    reject_unknown(am, &["version", "state"], "resolve_outcome.annotated")?;
    Ok(ResolveOutcome::Annotated {
        version: version_from_json(am.get("version").ok_or(CodecError::MissingMember {
            member: "version",
            record: "resolve_outcome.annotated",
        })?)?,
        state: LifecycleStateKind::parse(str_at(am, "state", "resolve_outcome.annotated")?).ok_or(
            CodecError::TypeMismatch {
                member: "state".to_string(),
                expected: "a LifecycleStateKind spelling",
            },
        )?,
    })
}

fn supersede_reason_from_str(s: &str) -> Result<SupersedeReason, CodecError> {
    match s {
        "edit" => Ok(SupersedeReason::Edit),
        "revocation" => Ok(SupersedeReason::Revocation),
        "expiry" => Ok(SupersedeReason::Expiry),
        "migration" => Ok(SupersedeReason::Migration),
        "consolidation" => Ok(SupersedeReason::Consolidation),
        "fork" => Ok(SupersedeReason::Fork),
        _ => Err(CodecError::TypeMismatch {
            member: "reason".to_string(),
            expected: "a SupersedeReason spelling",
        }),
    }
}

/// `SupersedesEdge` decode — the `edge_json` mirror.
pub fn edge_from_json(j: &Json) -> Result<SupersedesEdge, CodecError> {
    let m = expect_obj(j, "supersedes_edge")?;
    reject_unknown(m, &["newer", "older", "reason"], "supersedes_edge")?;
    Ok(SupersedesEdge {
        newer: str_at(m, "newer", "supersedes_edge")?.to_string(),
        older: str_at(m, "older", "supersedes_edge")?.to_string(),
        reason: supersede_reason_from_str(str_at(m, "reason", "supersedes_edge")?)?,
    })
}

/// `MemoryRevocation` decode — the `memory_revocation_json` mirror.
pub fn memory_revocation_from_json(j: &Json) -> Result<MemoryRevocation, CodecError> {
    let m = expect_obj(j, "memory_revocation")?;
    reject_unknown(
        m,
        &["version_id", "reason", "revoker", "replacement", "at_seq"],
        "memory_revocation",
    )?;
    Ok(MemoryRevocation {
        version_id: str_at(m, "version_id", "memory_revocation")?.to_string(),
        reason: RevocationReason::parse(str_at(m, "reason", "memory_revocation")?).ok_or(
            CodecError::TypeMismatch {
                member: "reason".to_string(),
                expected: "a RevocationReason spelling",
            },
        )?,
        revoker: ProvenanceRecord::from_json(m.get("revoker").ok_or(
            CodecError::MissingMember {
                member: "revoker",
                record: "memory_revocation",
            },
        )?)
        .map_err(|_| CodecError::TypeMismatch {
            member: "revoker".to_string(),
            expected: "a ProvenanceRecord",
        })?,
        replacement: opt_str(m, "replacement")?,
        at_seq: int_at(m, "at_seq", "memory_revocation")? as u64,
    })
}

fn rev_record_from_json(j: &Json) -> Result<RevocationRecord, CodecError> {
    let m = expect_obj(j, "revocation_record")?;
    reject_unknown(
        m,
        &[
            "kind",
            "revokes",
            "reason",
            "authority",
            "provenance",
            "validity_until",
            "replacement",
        ],
        "revocation_record",
    )?;
    let provenance =
        ProvenanceRecord::from_json(m.get("provenance").ok_or(CodecError::MissingMember {
            member: "provenance",
            record: "revocation_record",
        })?)
        .map_err(|_| CodecError::TypeMismatch {
            member: "provenance".to_string(),
            expected: "a ProvenanceRecord",
        })?;
    // `authority` is rendered for human readers; the typed Origin is the
    // provenance record's (`minted from authority at seq` — §8.3 #3).
    Ok(RevocationRecord {
        kind: record_kind_from_tag(str_at(m, "kind", "revocation_record")?)?,
        revokes: str_at(m, "revokes", "revocation_record")?.to_string(),
        reason: supersede_reason_from_str(str_at(m, "reason", "revocation_record")?)?,
        validity_until: opt_int(m, "validity_until")?,
        replacement: opt_str(m, "replacement")?,
        authority: provenance.origin.clone(),
        provenance,
    })
}

/// `MemoryError` decode — the `memory_error_json` mirror.
pub fn memory_error_from_json(j: &Json) -> Result<crate::memory::MemoryError, CodecError> {
    use crate::memory::MemoryError as E;
    let m = expect_obj(j, "memory_error")?;
    let variant = str_at(m, "variant", "memory_error")?;
    let err = match variant {
        "MissingProvenance" => E::MissingProvenance,
        "MissingContract" => E::MissingContract {
            scope: scope_at(m, "scope")?,
        },
        "ScopeCeilingExceeded" => E::ScopeCeilingExceeded {
            scope: scope_at(m, "scope")?,
            writer: str_at(m, "writer", "memory_error")?.to_string(),
        },
        "Fenced" => E::Fenced {
            scope: scope_at(m, "scope")?,
            expected: int_at(m, "expected", "memory_error")? as u64,
            got: int_at(m, "got", "memory_error")? as u64,
        },
        "UnknownVersion" => E::UnknownVersion {
            version_id: str_at(m, "version_id", "memory_error")?.to_string(),
        },
        "UnknownName" => E::UnknownName {
            scope: scope_at(m, "scope")?,
            name: str_at(m, "name", "memory_error")?.to_string(),
        },
        "CycleDetected" => E::CycleDetected {
            newer: str_at(m, "newer", "memory_error")?.to_string(),
            older: str_at(m, "older", "memory_error")?.to_string(),
        },
        "KindMismatch" => E::KindMismatch {
            newer: MemoryKind::parse(str_at(m, "newer", "memory_error")?).ok_or(
                CodecError::TypeMismatch {
                    member: "newer".to_string(),
                    expected: "a MemoryKind spelling",
                },
            )?,
            older: MemoryKind::parse(str_at(m, "older", "memory_error")?).ok_or(
                CodecError::TypeMismatch {
                    member: "older".to_string(),
                    expected: "a MemoryKind spelling",
                },
            )?,
        },
        "AuthorityInsufficient" => E::AuthorityInsufficient {
            new_authority: str_at(m, "new_authority", "memory_error")?.to_string(),
            old_authority: str_at(m, "old_authority", "memory_error")?.to_string(),
        },
        "TaintedAboveExternal" => E::TaintedAboveExternal {
            detail: str_at(m, "detail", "memory_error")?.to_string(),
        },
        "NotPersistable" => E::NotPersistable {
            detail: str_at(m, "detail", "memory_error")?.to_string(),
        },
        "UnvalidatedExternalDependency" => E::UnvalidatedExternalDependency {
            ref_: str_at(m, "ref", "memory_error")?.to_string(),
        },
        "Channel" => E::Channel {
            detail: str_at(m, "detail", "memory_error")?.to_string(),
        },
        _ => {
            return Err(CodecError::TypeMismatch {
                member: "variant".to_string(),
                expected: "a MemoryError variant",
            })
        }
    };
    Ok(err)
}

/// `LifecycleError` decode — the `lifecycle_error_json` mirror.
pub fn lifecycle_error_from_json(j: &Json) -> Result<LifecycleError, CodecError> {
    let m = expect_obj(j, "lifecycle_error")?;
    let variant = str_at(m, "variant", "lifecycle_error")?;
    let err = match variant {
        "AuthorityInsufficient" => LifecycleError::AuthorityInsufficient {
            caller: str_at(m, "caller", "lifecycle_error")?.to_string(),
            target: str_at(m, "target", "lifecycle_error")?.to_string(),
        },
        "AlreadyRevoked" => LifecycleError::AlreadyRevoked {
            version_id: str_at(m, "version_id", "lifecycle_error")?.to_string(),
        },
        "SupersessionAuthorityInsufficient" => LifecycleError::SupersessionAuthorityInsufficient {
            new_authority: str_at(m, "new_authority", "lifecycle_error")?.to_string(),
            old_authority: str_at(m, "old_authority", "lifecycle_error")?.to_string(),
        },
        "IllegitimateEndorsement" => LifecycleError::IllegitimateEndorsement {
            caller: str_at(m, "caller", "lifecycle_error")?.to_string(),
        },
        "EndorserBelowTarget" => LifecycleError::EndorserBelowTarget {
            endorser: str_at(m, "endorser", "lifecycle_error")?.to_string(),
            target: str_at(m, "target", "lifecycle_error")?.to_string(),
        },
        "BasisNotAllowed" => LifecycleError::BasisNotAllowed {
            basis: str_at(m, "basis", "lifecycle_error")?.to_string(),
        },
        "Store" => LifecycleError::Store(memory_error_from_json(m.get("error").ok_or(
            CodecError::MissingMember {
                member: "error",
                record: "lifecycle_error",
            },
        )?)?),
        _ => {
            return Err(CodecError::TypeMismatch {
                member: "variant".to_string(),
                expected: "a LifecycleError variant",
            })
        }
    };
    Ok(err)
}

/// The refusal member of an output, when the op refused.
fn refusal_member(j: &Json) -> Option<&Json> {
    match j {
        Json::Obj(m) if m.get("ok") == Some(&Json::Bool(false)) => m.get("refusal"),
        _ => None,
    }
}

/// Decode the domain refusal inside an op output (memory domain).
fn memory_refusal(j: &Json) -> Result<Option<crate::memory::MemoryError>, ChannelFault> {
    let Some(r) = refusal_member(j) else {
        return Ok(None);
    };
    let rm = expect_obj(r, "refusal").map_err(ChannelFault::Decode)?;
    match rm.get("kind").and_then(Json::as_str) {
        Some("memory") => {
            memory_error_from_json(rm.get("error").ok_or(CodecError::MissingMember {
                member: "error",
                record: "refusal",
            })?)
            .map(Some)
            .map_err(ChannelFault::Decode)
        }
        other => Err(ChannelFault::BadRefusal(format!(
            "expected a memory refusal, got {other:?}"
        ))),
    }
}

/// Decode the domain refusal inside an op output (lifecycle domain —
/// `Store` wraps a memory refusal either side spells).
fn lifecycle_refusal(j: &Json) -> Result<Option<LifecycleError>, ChannelFault> {
    let Some(r) = refusal_member(j) else {
        return Ok(None);
    };
    let rm = expect_obj(r, "refusal").map_err(ChannelFault::Decode)?;
    match rm.get("kind").and_then(Json::as_str) {
        Some("lifecycle") => {
            lifecycle_error_from_json(rm.get("error").ok_or(CodecError::MissingMember {
                member: "error",
                record: "refusal",
            })?)
            .map(Some)
            .map_err(ChannelFault::Decode)
        }
        Some("memory") => {
            memory_error_from_json(rm.get("error").ok_or(CodecError::MissingMember {
                member: "error",
                record: "refusal",
            })?)
            .map(|e| Some(LifecycleError::Store(e)))
            .map_err(ChannelFault::Decode)
        }
        other => Err(ChannelFault::BadRefusal(format!(
            "expected a lifecycle refusal, got {other:?}"
        ))),
    }
}

// ── The input encoders (the client's half of the boundary — mirrors of the
// server-side decoders; one spelling, CC7) ────────────────────────────────────

/// `PersistenceScope` → its canonical spelling.
pub fn scope_json(scope: PersistenceScope) -> Json {
    Json::str(scope.as_str())
}

/// `MemoryDraft` → the `draft_from_json` mirror.
pub fn draft_json(d: &MemoryDraft) -> Json {
    let mut m = vec![
        ("kind", Json::str(d.kind.as_str())),
        (
            "content",
            match &d.content {
                MemoryContent::Text(t) => Json::obj([
                    ("text", Json::str(t.content.clone().unwrap_or_default())),
                    ("owner", Json::str(t.owner.clone())),
                ]),
                MemoryContent::Structured(j) => Json::obj([("structured", j.clone())]),
            },
        ),
        ("scope", scope_json(d.scope)),
        (
            "declared_inputs",
            Json::Arr(d.declared_inputs.iter().map(vref_json).collect()),
        ),
        (
            "justifications",
            Json::Arr(d.justifications.iter().map(justification_json).collect()),
        ),
        ("validator_endorsed", Json::Bool(d.validator_endorsed)),
    ];
    if let Some(sk) = &d.subject_key {
        m.push(("subject_key", subject_key_json(sk)));
    }
    if let Some(c) = &d.contract {
        m.push(("contract", crate::memory::contract_json(c)));
    }
    if let Some(sc) = &d.supersedes {
        m.push((
            "supersedes",
            Json::obj([
                ("version_id", Json::str(sc.version_id.clone())),
                ("reason", Json::str(sc.reason.as_str())),
            ]),
        ));
    }
    if let Some(v) = &d.validity {
        m.push(("validity", validity_json(v)));
    }
    if let Some(p) = &d.provenance {
        m.push(("provenance", p.to_json()));
    }
    if let Some(s) = &d.semantic_id {
        m.push(("semantic_id", Json::str(s.clone())));
    }
    Json::obj(m)
}

/// `WriteContext` → the `write_ctx_from_json` mirror.
pub fn write_ctx_json(c: &WriteContext) -> Json {
    Json::obj([
        ("context_label", label_json(&c.context_label)),
        ("lease_generation", Json::Int(c.lease_generation as i64)),
        ("at_seq", Json::Int(c.at_seq as i64)),
        ("run_id", Json::str(c.run_id.clone())),
    ])
}

// ── RemoteStore's MemoryStorePort impl ───────────────────────────────────────

/// A channel fault on a fallible op surfaces as the typed `Channel` refusal
/// (the refusal domain includes the store backend — typed, never silent).
fn channel_to_memory(f: ChannelFault) -> crate::memory::MemoryError {
    crate::memory::MemoryError::Channel {
        detail: f.to_string(),
    }
}

fn channel_to_lifecycle(f: ChannelFault) -> LifecycleError {
    LifecycleError::Store(channel_to_memory(f))
}

impl<C: StoreChannel> MemoryStorePort for RemoteStore<C> {
    fn put(
        &mut self,
        draft: MemoryDraft,
        ctx: &WriteContext,
    ) -> Result<PutOutcome, crate::memory::MemoryError> {
        if let Some(e) = self.latched() {
            return Err(e);
        }
        let out = self
            .call("put", vec![draft_json(&draft), write_ctx_json(ctx)])
            .map_err(|f| {
                let e = channel_to_memory(f.clone());
                self.fault = Some(f);
                e
            })?;
        if let Some(e) = memory_refusal(&out).map_err(channel_to_memory)? {
            return Err(e);
        }
        let remote =
            put_outcome_from_json(&out).map_err(|e| channel_to_memory(ChannelFault::Decode(e)))?;
        // The deterministic mirror must mint the identical record — a
        // disagreement is a divergence fault, never an adoption.
        let local = self.mirror.put(draft, ctx).map_err(|e| {
            self.fault = Some(ChannelFault::Divergence(format!(
                "remote put succeeded, mirror refused: {e}"
            )));
            crate::memory::MemoryError::Channel {
                detail: format!("divergence: mirror refused: {e}"),
            }
        })?;
        if local.version.version_id != remote.version.version_id {
            let f = ChannelFault::Divergence(format!(
                "put: remote {} != mirror {}",
                remote.version.version_id, local.version.version_id
            ));
            self.fault = Some(f.clone());
            return Err(channel_to_memory(f));
        }
        Ok(remote)
    }

    fn bind(
        &mut self,
        scope: PersistenceScope,
        name: &str,
        version_id: &str,
        supersedes: Option<&str>,
        reason: &str,
        at_seq: u64,
    ) -> Result<NameBinding, crate::memory::MemoryError> {
        if let Some(e) = self.latched() {
            return Err(e);
        }
        let mut args = vec![
            ("scope", scope_json(scope)),
            ("name", Json::str(name)),
            ("version_id", Json::str(version_id)),
            ("reason", Json::str(reason)),
            ("at_seq", Json::Int(at_seq as i64)),
        ];
        if let Some(s) = supersedes {
            args.push(("supersedes", Json::str(s)));
        }
        let out = self.call("bind", vec![Json::obj(args)]).map_err(|f| {
            let e = channel_to_memory(f.clone());
            self.fault = Some(f);
            e
        })?;
        if let Some(e) = memory_refusal(&out).map_err(channel_to_memory)? {
            return Err(e);
        }
        let remote =
            binding_from_json(&out).map_err(|e| channel_to_memory(ChannelFault::Decode(e)))?;
        let local = self
            .mirror
            .bind(scope, name, version_id, supersedes, reason, at_seq)
            .map_err(|e| {
                self.fault = Some(ChannelFault::Divergence(format!(
                    "remote bind succeeded, mirror refused: {e}"
                )));
                crate::memory::MemoryError::Channel {
                    detail: format!("divergence: mirror refused: {e}"),
                }
            })?;
        if local != remote {
            let f = ChannelFault::Divergence("bind: mirror != remote".to_string());
            self.fault = Some(f.clone());
            return Err(channel_to_memory(f));
        }
        Ok(remote)
    }

    fn revoke(
        &mut self,
        version_id: &str,
        reason: RevocationReason,
        revoker: &ProvenanceRecord,
        replacement: Option<String>,
        at_seq: u64,
    ) -> Result<RevocationRecord, LifecycleError> {
        if let Some(f) = self.fault.take() {
            return Err(channel_to_lifecycle(f));
        }
        let mut args = vec![
            ("version_id", Json::str(version_id)),
            ("reason", Json::str(reason.as_str())),
            ("revoker", revoker.to_json()),
            ("at_seq", Json::Int(at_seq as i64)),
        ];
        if let Some(r) = &replacement {
            args.push(("replacement", Json::str(r.clone())));
        }
        let out = self.call("revoke", vec![Json::obj(args)]).map_err(|f| {
            let e = channel_to_lifecycle(f.clone());
            self.fault = Some(f);
            e
        })?;
        if let Some(e) = lifecycle_refusal(&out).map_err(channel_to_lifecycle)? {
            return Err(e);
        }
        let remote = rev_record_from_json(&out)
            .map_err(|e| channel_to_lifecycle(ChannelFault::Decode(e)))?;
        let local = crate::lifecycle::revoke(
            &mut self.mirror,
            version_id,
            reason,
            revoker,
            replacement,
            at_seq,
        )
        .map_err(|e| {
            self.fault = Some(ChannelFault::Divergence(format!(
                "remote revoke succeeded, mirror refused: {e}"
            )));
            LifecycleError::Store(crate::memory::MemoryError::Channel {
                detail: format!("divergence: mirror refused: {e}"),
            })
        })?;
        if local.revokes != remote.revokes {
            let f = ChannelFault::Divergence("revoke: mirror != remote".to_string());
            self.fault = Some(f.clone());
            return Err(channel_to_lifecycle(f));
        }
        Ok(remote)
    }

    // `&self` reads serve the deterministic mirror — the projection of every
    // op this client applied, record-identical to the remote store's fold.
    fn resolve(
        &self,
        scope: PersistenceScope,
        name: Option<&str>,
        version_id: Option<&str>,
        mode: ResolveMode,
    ) -> Result<ResolveOutcome, crate::memory::MemoryError> {
        self.mirror.resolve(scope, name, version_id, mode)
    }

    fn manifest(&self, scope: PersistenceScope, at: u64) -> MemoryManifest {
        self.mirror.manifest(scope, at)
    }

    fn enumerate(&self, layer: Layer) -> Vec<String> {
        self.mirror.enumerate(layer)
    }

    fn stale_candidates(&self, scope: PersistenceScope) -> Vec<String> {
        self.mirror.stale_candidates(scope)
    }

    fn reads_of(&self, version_id: &str) -> Vec<u64> {
        MemoryStorePort::reads_of(&self.mirror, version_id)
    }

    fn writes_by(&self, writer_ref: &str) -> Vec<String> {
        self.mirror.writes_by(writer_ref)
    }

    fn record_read(&mut self, version_id: &str, seq: u64) {
        self.mirror.record_read(version_id, seq);
        self.fire(
            "record_read",
            vec![Json::obj([
                ("version_id", Json::str(version_id)),
                ("seq", Json::Int(seq as i64)),
            ])],
        );
    }

    fn set_stamp(&mut self, dep_ref: &str, stamp: &str) {
        self.mirror.set_stamp(dep_ref, stamp);
        self.fire(
            "set_stamp",
            vec![Json::obj([
                ("dep_ref", Json::str(dep_ref)),
                ("stamp", Json::str(stamp)),
            ])],
        );
    }

    fn set_validator_verdict(&mut self, version_id: &str, validator_ref: &str, ok: bool) {
        self.mirror
            .set_validator_verdict(version_id, validator_ref, ok);
        self.fire(
            "set_validator_verdict",
            vec![Json::obj([
                ("version_id", Json::str(version_id)),
                ("validator_ref", Json::str(validator_ref)),
                ("ok", Json::Bool(ok)),
            ])],
        );
    }

    fn mark_scope_ended(&mut self, scope: PersistenceScope) {
        self.mirror.mark_scope_ended(scope);
        self.fire(
            "mark_scope_ended",
            vec![Json::obj([("scope", scope_json(scope))])],
        );
    }

    fn publish_replacement(&mut self, name: &str) {
        self.mirror.publish_replacement(name);
        self.fire(
            "publish_replacement",
            vec![Json::obj([("name", Json::str(name))])],
        );
    }

    fn version(&self, version_id: &str) -> Option<MemoryVersion> {
        self.mirror.version(version_id).cloned()
    }

    fn version_order(&self) -> Vec<String> {
        self.mirror.version_order().to_vec()
    }

    fn edges(&self) -> Vec<SupersedesEdge> {
        self.mirror.edges().to_vec()
    }

    fn revocations(&self) -> Vec<MemoryRevocation> {
        self.mirror.revocations().to_vec()
    }

    fn applied_seq(&self) -> u64 {
        self.mirror.applied_seq()
    }

    fn drain_events(&mut self) -> Vec<(String, Json)> {
        // Drain the mirror (the client's projection) and the remote; the two
        // lists are record-identical for a conforming pair.
        let local = self.mirror.drain_events();
        match self.channel.invoke("drain_events", vec![]) {
            Ok(mut outs) if outs.len() == 1 => {
                let rows = match outs.remove(0) {
                    Json::Arr(a) => a,
                    _ => vec![],
                };
                let remote: Vec<(String, Json)> = rows
                    .iter()
                    .filter_map(|r| {
                        let m = match r {
                            Json::Obj(m) => m,
                            _ => return None,
                        };
                        let class = m.get("class").and_then(Json::as_str)?;
                        let payload = m.get("payload").cloned()?;
                        Some((class.to_string(), payload))
                    })
                    .collect();
                if remote == local {
                    remote
                } else {
                    self.fault = Some(ChannelFault::Divergence(
                        "drain_events: mirror != remote".to_string(),
                    ));
                    local
                }
            }
            Ok(_) => {
                self.fault = Some(ChannelFault::Decode(CodecError::TypeMismatch {
                    member: "outputs".to_string(),
                    expected: "exactly one output",
                }));
                local
            }
            Err(f) => {
                self.fault = Some(f);
                local
            }
        }
    }

    fn take_lease(&mut self, scope: PersistenceScope, holder: &str) -> u64 {
        let local = self.mirror.take_lease(scope, holder);
        match self.channel.invoke(
            "take_lease",
            vec![Json::obj([
                ("scope", scope_json(scope)),
                ("holder", Json::str(holder)),
            ])],
        ) {
            Ok(outs) => {
                let remote = outs.first().and_then(Json::as_int).map(|i| i as u64);
                match remote {
                    Some(r) if r == local => {}
                    Some(r) => {
                        self.fault = Some(ChannelFault::Divergence(format!(
                            "take_lease: remote {r} != mirror {local}"
                        )));
                    }
                    None => {
                        self.fault = Some(ChannelFault::Decode(CodecError::TypeMismatch {
                            member: "outputs".to_string(),
                            expected: "one integer",
                        }));
                    }
                }
            }
            Err(f) => self.fault = Some(f),
        }
        local
    }

    fn lease(&self, scope: PersistenceScope) -> u64 {
        self.mirror.lease(scope)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The contract conformance battery (§5c.3; T-LCD-12) — the same suite runs
// against every `memory_store` placement: the in-process `MemoryStore` and
// the `hh-memory-store` plugin bound through `RemoteStore` over a
// `plugin_abi/1` session. Kept beside the codec (the contract's shape lives
// here — CC7) so no placement drifts its own copy of the checks.
// ─────────────────────────────────────────────────────────────────────────────

/// The conformance driver — panics on any contract violation (a test-only
/// call surface: the battery is the oracle, not a runtime check).
pub mod conformance {
    use super::*;
    use crate::memory::MemoryStore;
    use hh_provenance::label::Label;
    use hh_provenance::origin::Origin;

    fn battery_draft(content: &str) -> MemoryDraft {
        MemoryDraft {
            kind: MemoryKind::Fact,
            subject_key: None,
            content: MemoryContent::Text(Box::new(Text::new(
                content,
                "owner",
                ProvenanceRecord::minted(
                    Origin::model("m1", "run1", "r1"),
                    PersistenceScope::Run,
                    0,
                ),
            ))),
            contract: Some(InvalidationContract {
                dependencies: vec![],
                cache_hint: CacheHint::Cacheable,
                validator_ref: None,
                freshness: None,
                invalidation_condition: None,
                revalidation: Revalidation::Never,
            }),
            scope: PersistenceScope::Run,
            declared_inputs: vec![],
            justifications: vec![],
            supersedes: None,
            validity: None,
            provenance: Some(ProvenanceRecord::minted(
                Origin::model("m1", "run1", "r1"),
                PersistenceScope::Run,
                0,
            )),
            semantic_id: None,
            validator_endorsed: false,
        }
    }

    /// The op battery — lease/put/enumerate/version/writes_by/record_read/
    /// reads_of/bind/manifest/resolve/revoke/environment ports/
    /// stale_candidates/drain_events/applied_seq/version_order/edges/
    /// revocations, with the typed refusals the contract declares
    /// (`Execute` refuses the revoked head).
    pub fn run(store: &mut dyn MemoryStorePort) {
        let g = store.take_lease(PersistenceScope::Run, "agent");
        let ctx = WriteContext {
            context_label: Label::top(),
            lease_generation: g,
            at_seq: 1,
            run_id: "run1".into(),
        };
        let out = store
            .put(battery_draft("port put"), &ctx)
            .expect("put through the port");
        let vid = out.version.version_id.clone();
        assert!(store.version(&vid).is_some());
        assert!(store
            .enumerate(crate::vocab::Layer::Episodic)
            .contains(&vid));
        assert!(store.writes_by("m1").contains(&vid));
        store.record_read(&vid, 9);
        assert_eq!(store.reads_of(&vid), vec![9]);
        store
            .bind(PersistenceScope::Run, "facts/a", &vid, None, "bind", 2)
            .unwrap();
        let m = store.manifest(PersistenceScope::Run, 2);
        assert_eq!(m.entries.get("facts/a"), Some(&vid));
        let r = store
            .resolve(
                PersistenceScope::Run,
                Some("facts/a"),
                None,
                ResolveMode::Execute,
            )
            .unwrap();
        assert!(matches!(r, ResolveOutcome::Live { .. }));
        let revoker = ProvenanceRecord::minted(
            Origin::human("alice", hh_provenance::origin::HumanRole::Principal),
            PersistenceScope::User,
            3,
        );
        store
            .revoke(&vid, RevocationReason::Contradicted, &revoker, None, 4)
            .unwrap();
        let r = store
            .resolve(
                PersistenceScope::Run,
                Some("facts/a"),
                None,
                ResolveMode::Execute,
            )
            .unwrap();
        assert!(
            matches!(
                r,
                ResolveOutcome::Unservable {
                    state: LifecycleStateKind::Revoked,
                    ..
                }
            ),
            "revoked head must be Unservable{{revoked}} under execute, got {r:?}"
        );
        store.set_stamp("dep/a", "s2");
        store.set_validator_verdict(&vid, "v/x", true);
        store.mark_scope_ended(PersistenceScope::Run);
        store.publish_replacement("facts/a");
        let _ = store.stale_candidates(PersistenceScope::Run);
        assert!(!store.drain_events().is_empty());
        assert!(store.applied_seq() >= 4);
        let _ = store.version_order();
        let _ = store.edges();
        let _ = store.revocations();
    }

    /// An in-process `StoreChannel` over `dispatch` — the loopback every
    /// codec-level check uses (the plugin's channel is the spawned-session
    /// adapter; both implement the same trait).
    pub struct LoopChannel(pub MemoryStore);

    impl StoreChannel for LoopChannel {
        fn invoke(
            &mut self,
            operation: &str,
            inputs: Vec<Json>,
        ) -> Result<Vec<Json>, ChannelFault> {
            dispatch(&mut self.0, operation, &inputs).map_err(ChannelFault::Decode)
        }
    }

    /// `RemoteStore` over the loopback — the mirror/channel/divergence path
    /// without a subprocess (the spawned-plugin session runs the same
    /// battery end-to-end in the `hh-memory-store` package test).
    pub fn run_remote() {
        let channel = LoopChannel(MemoryStore::new("memory_store"));
        let mut remote = RemoteStore::new(channel);
        run(&mut remote);
        assert!(
            remote.fault().is_none(),
            "latched fault: {:?}",
            remote.fault()
        );
    }
}
