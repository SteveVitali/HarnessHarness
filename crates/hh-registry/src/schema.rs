//! The `registry/1` canonical encode/decode (CC7 — the one schema source). Every
//! record body encodes in **document** form (identity + surface fields —
//! `Text::to_json` carries authority) for storage/verification, and in
//! **semantic** form (the identity projection — `semantic_json` strips authority
//! from text legs) for `version_id`/`semantic_id` minting (N6, ADR-0239):
//!
//! - `version_id = idp("registry.<kind>" | dedicated tag, H(body — surface fields))`
//!   — the semantic *core* only; a rename never rewinds the version DAG.
//! - `semantic_id = idp("semantic.registry.<kind>", H(semantic core — variant_id))`
//!   — declared where the kind has a projection; `variant_id` removed so a rename
//!   shares the coordinate (AC-7).
//!
//! Decoders are path-strict: a member outside the declared schema or a value
//! outside the closed vocabularies is `SchemaViolation{path}` / `UnknownRecordKind`
//! (R2/R5 — `ext` never carries a status or an identity-bearing value, R9).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_hir::{debt_from_json, debt_json};
use hh_identity::idp::ContentAddress;
use hh_identity::names::NameStatus;
use hh_identity::repro::InstrumentRecord;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::{self, Json};

use crate::errors::RegistryError;
use crate::kinds::{
    Admission, Cardinality, ConformanceVerdict, OracleClass, OwnerRef, Placement, ProducedBy,
    PublishRule, RecordKind, RequireConformance, SubjectKind, TestDriver, TestKind,
};
use crate::records::{
    AppliesTo, CapabilityRecord, ClassRecord, ConformanceReport, ConformanceSuite,
    ContractOperation, ForeignImport, Implementation, NamespaceRecord, ParamDecl,
    RegistryDiagnostic, RegistryEnvelope, RegistryPolicy, RegistryRecord, RegistrySnapshot,
    ReportHost, ReportResult, SuiteTest, VariantRecord, REGISTRY_DIALECT,
};

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    Json::Obj(m)
}

fn strs(v: &BTreeSet<String>) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect())
}

fn str_vec(v: &[String]) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect())
}

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, RegistryError> {
    j.get(k).ok_or_else(|| RegistryError::SchemaViolation {
        path: format!("{path}.{k}"),
        detail: "missing".to_string(),
    })
}

fn req_str(j: &Json, k: &str, path: &str) -> Result<String, RegistryError> {
    match req(j, k, path)? {
        Json::Str(s) => Ok(s.clone()),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected string".to_string(),
        }),
    }
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    match j.get(k) {
        Some(Json::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn req_bool(j: &Json, k: &str, path: &str) -> Result<bool, RegistryError> {
    match req(j, k, path)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected bool".to_string(),
        }),
    }
}

fn req_arr<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a [Json], RegistryError> {
    match req(j, k, path)? {
        Json::Arr(a) => Ok(a),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected array".to_string(),
        }),
    }
}

fn req_u64(j: &Json, k: &str, path: &str) -> Result<u64, RegistryError> {
    match req(j, k, path)? {
        Json::Int(i) if *i >= 0 => Ok(*i as u64),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected u64".to_string(),
        }),
    }
}

fn obj_entries<'a>(
    j: &'a Json,
    k: &str,
    path: &str,
) -> Result<&'a BTreeMap<String, Json>, RegistryError> {
    match req(j, k, path)? {
        Json::Obj(m) => Ok(m),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected object".to_string(),
        }),
    }
}

/// Order-preserving decode for `Vec<String>` fields — `str_set` sorts and would
/// break the canonical round-trip (verify() re-mints `version_id` over the
/// decoded body — CC3).
fn str_vec_dec(j: &Json, k: &str, path: &str) -> Result<Vec<String>, RegistryError> {
    let mut out = Vec::new();
    for e in req_arr(j, k, path)? {
        match e {
            Json::Str(s) => out.push(s.clone()),
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.{k}"),
                    detail: "expected string".to_string(),
                })
            }
        }
    }
    Ok(out)
}

fn str_set(j: &Json, k: &str, path: &str) -> Result<BTreeSet<String>, RegistryError> {
    let mut out = BTreeSet::new();
    for e in req_arr(j, k, path)? {
        match e {
            Json::Str(s) => {
                out.insert(s.clone());
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.{k}"),
                    detail: "expected string".to_string(),
                })
            }
        }
    }
    Ok(out)
}

fn content_addr_json(a: &ContentAddress) -> Json {
    obj(vec![
        ("algorithm", Json::str(a.algorithm)),
        ("digest", Json::str(a.digest.clone())),
        ("idp", Json::str(a.idp)),
        ("media_type", Json::str(a.media_type.clone())),
        ("size", Json::Int(a.size as i64)),
    ])
}

fn pinned_addr(j: &Json, path: &str) -> Result<ContentAddress, RegistryError> {
    let bad = || RegistryError::SchemaViolation {
        path: path.to_string(),
        detail: "expected pinned content address object".to_string(),
    };
    let algorithm = match req_str(j, "algorithm", path)?.as_str() {
        "sha256" => "sha256",
        _ => return Err(bad()),
    };
    Ok(ContentAddress {
        idp: "idp/1",
        algorithm,
        digest: req_str(j, "digest", path)?,
        media_type: req_str(j, "media_type", path)?,
        size: req_u64(j, "size", path)?,
    })
}

/// Reject an envelope/record member carrying a status-shaped value the three axes
/// do not own — `ext` never introduces a status vocabulary (R5).
fn reject_unknown_status(s: &str, path: &str) -> Result<(), RegistryError> {
    if matches!(
        s,
        "resolved" | "sealed" | "quarantined" | "revoked" | "active" | "deprecated" | "yanked"
    ) {
        return Ok(());
    }
    Err(RegistryError::SchemaViolation {
        path: format!("{path}.status"),
        detail: format!("status spelling outside the three R5 vocabularies: {s}"),
    })
}

// ── ParamDecl ────────────────────────────────────────────────────────────────

fn param_json(p: &ParamDecl) -> Json {
    obj(vec![
        ("affects", str_vec(&p.affects)),
        ("budget_relevant", Json::Bool(p.budget_relevant)),
        ("default", p.default.clone().unwrap_or(Json::Null)),
        ("domain", p.domain.clone().unwrap_or(Json::Null)),
        ("sweepable", Json::Bool(p.sweepable)),
        ("type", Json::str(p.value_type.clone())),
        ("unit", p.unit.clone().map_or(Json::Null, Json::Str)),
    ])
}

fn param_from_json(j: &Json, path: &str) -> Result<ParamDecl, RegistryError> {
    let domain = match req(j, "domain", path)? {
        Json::Null => None,
        d => Some(d.clone()),
    };
    let default = match req(j, "default", path)? {
        Json::Null => None,
        d => Some(d.clone()),
    };
    Ok(ParamDecl {
        value_type: req_str(j, "type", path)?,
        domain,
        default,
        unit: opt_str(j, "unit"),
        sweepable: req_bool(j, "sweepable", path)?,
        budget_relevant: req_bool(j, "budget_relevant", path)?,
        affects: str_vec_dec(j, "affects", path)?,
    })
}

fn params_json(p: &BTreeMap<String, ParamDecl>) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in p {
        m.insert(k.clone(), param_json(v));
    }
    Json::Obj(m)
}

fn params_from_json(
    j: &Json,
    key: &str,
    path: &str,
) -> Result<BTreeMap<String, ParamDecl>, RegistryError> {
    let mut out = BTreeMap::new();
    for (k, v) in obj_entries(j, key, path)? {
        out.insert(k.clone(), param_from_json(v, &format!("{path}.{key}.{k}"))?);
    }
    Ok(out)
}

// ── contract ─────────────────────────────────────────────────────────────────

fn contract_op_json(o: &ContractOperation) -> Json {
    obj(vec![
        ("failure_modes", str_vec(&o.failure_modes)),
        ("inputs", o.inputs.clone()),
        ("invariants", str_vec(&o.invariants)),
        ("name", Json::str(o.name.clone())),
        ("outputs", o.outputs.clone()),
    ])
}

fn contract_from_json(j: &Json, path: &str) -> Result<Vec<ContractOperation>, RegistryError> {
    let mut out = Vec::new();
    for (i, e) in req_arr(j, "contract", path)?.iter().enumerate() {
        let p = format!("{path}.contract[{i}]");
        out.push(ContractOperation {
            name: req_str(e, "name", &p)?,
            inputs: req(e, "inputs", &p)?.clone(),
            outputs: req(e, "outputs", &p)?.clone(),
            invariants: str_vec_dec(e, "invariants", &p)?,
            failure_modes: str_vec_dec(e, "failure_modes", &p)?,
        });
    }
    Ok(out)
}

// ── ClassRecord ──────────────────────────────────────────────────────────────

/// `ContractRef` list member → JSON (empty lists encode absent — CC8: the
/// pre-S1.27 canonical bytes are unchanged for a record with no `depends_on`).
fn depends_on_json(deps: &[hh_plugin::ContractRef]) -> Json {
    Json::Arr(deps.iter().map(|d| d.to_json()).collect())
}

fn depends_on_from_json(
    j: &Json,
    path: &str,
) -> Result<Vec<hh_plugin::ContractRef>, RegistryError> {
    match j.get("depends_on") {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Arr(a)) => {
            a.iter()
                .enumerate()
                .map(|(i, v)| {
                    hh_plugin::ContractRef::from_json(v, &format!("{path}.depends_on[{i}]"))
                        .map_err(|e| match e {
                            hh_plugin::ContractRefError::SchemaViolation { path, detail } => {
                                RegistryError::SchemaViolation { path, detail }
                            }
                        })
                })
                .collect()
        }
        Some(_) => Err(RegistryError::SchemaViolation {
            path: format!("{path}.depends_on"),
            detail: "expected array of ContractRef".to_string(),
        }),
    }
}

fn class_body_json(c: &ClassRecord) -> Json {
    let mut members = vec![
        ("base_param_schema", params_json(&c.base_param_schema)),
        ("cardinality", Json::str(c.cardinality.as_str())),
        ("class_id", Json::str(c.class_id.clone())),
        (
            "conformance_suite_ref",
            c.conformance_suite_ref
                .clone()
                .map_or(Json::Null, Json::Str),
        ),
        (
            "contract",
            Json::Arr(c.contract.iter().map(contract_op_json).collect()),
        ),
        ("contract_version", Json::str(c.contract_version.clone())),
        ("decision_points", str_vec(&c.decision_points)),
        ("declaration_schema", c.declaration_schema.clone()),
        (
            "dialect_introduced",
            Json::str(c.dialect_introduced.clone()),
        ),
        ("home", Json::str(c.home.clone())),
        ("hot_path", Json::Bool(c.hot_path)),
        ("metrics_declared", str_vec(&c.metrics_declared)),
        ("required_inputs", strs(&c.required_inputs)),
        ("slot_key", Json::str(c.slot_key.clone())),
        ("tier", Json::str(c.tier.clone())),
    ];
    if !c.depends_on.is_empty() {
        members.push(("depends_on", depends_on_json(&c.depends_on)));
    }
    obj(members)
}

fn class_from_json(j: &Json, path: &str) -> Result<ClassRecord, RegistryError> {
    Ok(ClassRecord {
        class_id: req_str(j, "class_id", path)?,
        contract: contract_from_json(j, path)?,
        cardinality: Cardinality::parse(&req_str(j, "cardinality", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.cardinality"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        required_inputs: str_set(j, "required_inputs", path)?,
        base_param_schema: params_from_json(j, "base_param_schema", path)?,
        hot_path: req_bool(j, "hot_path", path)?,
        dialect_introduced: req_str(j, "dialect_introduced", path)?,
        contract_version: req_str(j, "contract_version", path)?,
        home: req_str(j, "home", path)?,
        declaration_schema: req(j, "declaration_schema", path)?.clone(),
        conformance_suite_ref: opt_str(j, "conformance_suite_ref"),
        decision_points: str_vec_dec(j, "decision_points", path)?,
        metrics_declared: str_vec_dec(j, "metrics_declared", path)?,
        slot_key: req_str(j, "slot_key", path)?,
        tier: req_str(j, "tier", path)?,
        depends_on: depends_on_from_json(j, path)?,
    })
}

// ── VariantRecord ────────────────────────────────────────────────────────────

fn applies_to_json(a: &AppliesTo) -> Json {
    obj(vec![
        ("families", str_vec(&a.families)),
        ("participant_classes", strs(&a.participant_classes)),
    ])
}

fn applies_to_from_json(j: &Json, path: &str) -> Result<AppliesTo, RegistryError> {
    Ok(AppliesTo {
        participant_classes: str_set(j, "participant_classes", path)?,
        families: str_vec_dec(j, "families", path)?,
    })
}

fn implementation_json(i: &Implementation) -> Json {
    obj(vec![
        ("content", content_addr_json(&i.content)),
        ("host_requirements", i.host_requirements.clone()),
        ("placement", Json::str(i.placement.as_str())),
    ])
}

fn implementation_from_json(j: &Json, path: &str) -> Result<Implementation, RegistryError> {
    let p = path.to_string();
    let content = pinned_addr(req(j, "content", &p)?, &format!("{p}.content"))?;
    Ok(Implementation {
        content,
        placement: Placement::parse(&req_str(j, "placement", &p)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{p}.placement"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        host_requirements: req(j, "host_requirements", &p)?.clone(),
    })
}

fn conditioned_rules_json(rs: &[(String, AssumptionDebtRecord)], semantic: bool) -> Json {
    Json::Arr(
        rs.iter()
            .map(|(rule_id, d)| {
                obj(vec![
                    ("rule_id", Json::str(rule_id.clone())),
                    ("debt", debt_json(d, semantic)),
                ])
            })
            .collect(),
    )
}

fn conditioned_rules_from_json(
    j: &Json,
    path: &str,
) -> Result<Vec<(String, AssumptionDebtRecord)>, RegistryError> {
    let mut out = Vec::new();
    for (i, e) in req_arr(j, "conditioned_rules", path)?.iter().enumerate() {
        let p = format!("{path}.conditioned_rules[{i}]");
        let debt = debt_from_json(req(e, "debt", &p)?, &format!("{p}.debt")).map_err(|e| {
            RegistryError::SchemaViolation {
                path: format!("{p}.debt"),
                detail: format!("{e:?}"),
            }
        })?;
        out.push((req_str(e, "rule_id", &p)?, debt));
    }
    Ok(out)
}

/// The variant body — the full canonical form (the `version_id` projection —
/// every declared field covered). `semantic = true` is the `semantic_id`
/// projection: the tag and surface fields drop out (AC-7 — a rename or summary
/// edit keeps the coordinate).
fn variant_body_json(v: &VariantRecord, semantic: bool) -> Json {
    let mut m = BTreeMap::new();
    if !semantic {
        m.insert(
            "declared_costs".to_string(),
            v.declared_costs.clone().unwrap_or(Json::Null),
        );
        m.insert("summary".to_string(), v.summary.to_json());
        m.insert("variant_id".to_string(), Json::str(v.variant_id.clone()));
        if let Some(l) = &v.version_label {
            m.insert("version_label".to_string(), Json::str(l.clone()));
        }
    }
    m.insert("applies_to".to_string(), applies_to_json(&v.applies_to));
    m.insert(
        "capability_declaration".to_string(),
        obj(v
            .capability_declaration
            .iter()
            .map(|(k, x)| (k.as_str(), x.clone()))
            .collect::<Vec<(&str, Json)>>()),
    );
    m.insert("class_ref".to_string(), Json::str(v.class_ref.clone()));
    m.insert(
        "conditioned_rules".to_string(),
        conditioned_rules_json(&v.conditioned_rules, semantic),
    );
    m.insert(
        "contract_range".to_string(),
        Json::str(v.contract_range.clone()),
    );
    m.insert(
        "dialect_range".to_string(),
        Json::str(v.dialect_range.clone()),
    );
    m.insert(
        "implementation".to_string(),
        implementation_json(&v.implementation),
    );
    m.insert("param_schema".to_string(), params_json(&v.param_schema));
    Json::Obj(m)
}

fn variant_from_json(j: &Json, path: &str) -> Result<VariantRecord, RegistryError> {
    let summary =
        Text::from_json(req(j, "summary", path)?, &format!("{path}.summary")).map_err(|e| {
            RegistryError::SchemaViolation {
                path: format!("{path}.summary"),
                detail: format!("{e:?}"),
            }
        })?;
    Ok(VariantRecord {
        variant_id: req_str(j, "variant_id", path)?,
        class_ref: req_str(j, "class_ref", path)?,
        contract_range: req_str(j, "contract_range", path)?,
        version_label: opt_str(j, "version_label"),
        param_schema: params_from_json(j, "param_schema", path)?,
        implementation: implementation_from_json(
            req(j, "implementation", path)?,
            &format!("{path}.implementation"),
        )?,
        capability_declaration: match req(j, "capability_declaration", path)? {
            Json::Obj(m) => m.clone(),
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.capability_declaration"),
                    detail: "expected object".to_string(),
                })
            }
        },
        conditioned_rules: conditioned_rules_from_json(j, path)?,
        applies_to: applies_to_from_json(
            req(j, "applies_to", path)?,
            &format!("{path}.applies_to"),
        )?,
        declared_costs: match j.get("declared_costs") {
            Some(Json::Null) | None => None,
            Some(d) => Some(d.clone()),
        },
        summary,
        dialect_range: req_str(j, "dialect_range", path)?,
    })
}

/// The semantic-coordinate projection of a `VariantRecord` — the core **minus
/// `variant_id`** (AC-7: the comparison coordinate survives a rename).
fn variant_semantic_projection(v: &VariantRecord) -> Json {
    let mut core = variant_body_json(v, true);
    if let Json::Obj(m) = &mut core {
        // `variant_id`/`version_label`/`summary`/`declared_costs` are already out
        // of the semantic body; nothing further to strip.
        let _ = m;
    }
    core
}

// ── ConformanceSuite / ConformanceReport ─────────────────────────────────────

fn suite_test_json(t: &SuiteTest) -> Json {
    obj(vec![
        ("budget", t.budget.clone()),
        ("driver", Json::str(t.driver.as_str())),
        (
            "fixture_ref",
            t.fixture_ref
                .as_ref()
                .map(content_addr_json)
                .unwrap_or(Json::Null),
        ),
        ("kind", Json::str(t.kind.as_str())),
        ("oracle_class", Json::str(t.oracle_class.as_str())),
        ("test_id", Json::str(t.test_id.clone())),
        ("verdict_rule", Json::str(t.verdict_rule.clone())),
    ])
}

fn suite_test_from_json(j: &Json, path: &str) -> Result<SuiteTest, RegistryError> {
    let fixture_ref = match req(j, "fixture_ref", path)? {
        Json::Null => None,
        Json::Obj(_) => Some(pinned_addr(
            req(j, "fixture_ref", path)?,
            &format!("{path}.fixture_ref"),
        )?),
        _ => {
            return Err(RegistryError::SchemaViolation {
                path: format!("{path}.fixture_ref"),
                detail: "expected pinned address or null".to_string(),
            })
        }
    };
    Ok(SuiteTest {
        test_id: req_str(j, "test_id", path)?,
        kind: TestKind::parse(&req_str(j, "kind", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.kind"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        fixture_ref,
        driver: TestDriver::parse(&req_str(j, "driver", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.driver"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        oracle_class: OracleClass::parse(&req_str(j, "oracle_class", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.oracle_class"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        budget: req(j, "budget", path)?.clone(),
        verdict_rule: req_str(j, "verdict_rule", path)?,
    })
}

fn suite_body_json(s: &ConformanceSuite) -> Json {
    obj(vec![
        ("class_ref", Json::str(s.class_ref.clone())),
        ("contract_version", Json::str(s.contract_version.clone())),
        ("required_for_status", strs(&s.required_for_status)),
        ("suite_id", Json::str(s.suite_id.clone())),
        (
            "tests",
            Json::Arr(s.tests.iter().map(suite_test_json).collect()),
        ),
    ])
}

fn suite_from_json(j: &Json, path: &str) -> Result<ConformanceSuite, RegistryError> {
    let mut tests = Vec::new();
    for (i, e) in req_arr(j, "tests", path)?.iter().enumerate() {
        tests.push(suite_test_from_json(e, &format!("{path}.tests[{i}]"))?);
    }
    Ok(ConformanceSuite {
        suite_id: req_str(j, "suite_id", path)?,
        class_ref: req_str(j, "class_ref", path)?,
        contract_version: req_str(j, "contract_version", path)?,
        tests,
        required_for_status: str_set(j, "required_for_status", path)?,
    })
}

fn instrument_json(i: &InstrumentRecord) -> Json {
    obj(vec![
        (
            "component_versions",
            Json::Arr(
                i.component_versions
                    .iter()
                    .map(|(a, b)| Json::Arr(vec![Json::str(a.clone()), Json::str(b.clone())]))
                    .collect(),
            ),
        ),
        ("dirty", Json::Bool(i.dirty)),
        ("idp", Json::str(i.idp)),
        ("source_commit", Json::str(i.source_commit.clone())),
        ("version", Json::str(i.version.clone())),
    ])
}

fn instrument_from_json(j: &Json, path: &str) -> Result<InstrumentRecord, RegistryError> {
    let mut cv = Vec::new();
    for e in req_arr(j, "component_versions", path)? {
        match e {
            Json::Arr(pair) if pair.len() == 2 => {
                if let (Json::Str(a), Json::Str(b)) = (&pair[0], &pair[1]) {
                    cv.push((a.clone(), b.clone()));
                }
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.component_versions"),
                    detail: "expected [name, version] pairs".to_string(),
                })
            }
        }
    }
    Ok(InstrumentRecord {
        version: req_str(j, "version", path)?,
        source_commit: req_str(j, "source_commit", path)?,
        dirty: req_bool(j, "dirty", path)?,
        component_versions: cv,
        idp: "idp/1",
    })
}

fn report_host_json(h: &ReportHost) -> Json {
    obj(vec![
        ("instrument", instrument_json(&h.instrument)),
        ("isolation", Json::str(h.isolation.clone())),
        ("placement", Json::str(h.placement.as_str())),
    ])
}

fn report_result_json(r: &ReportResult) -> Json {
    obj(vec![
        (
            "evidence_ref",
            r.evidence_ref.clone().map_or(Json::Null, Json::Str),
        ),
        ("subject", Json::str(r.subject.clone())),
        ("verdict", Json::str(r.verdict.as_str())),
    ])
}

fn report_body_json(r: &ConformanceReport) -> Json {
    obj(vec![
        ("host", report_host_json(&r.host)),
        (
            "probed_declaration",
            obj(r
                .probed_declaration
                .iter()
                .map(|(k, v)| (k.as_str(), Json::str(v.as_str())))
                .collect()),
        ),
        ("produced_by", Json::str(r.produced_by.as_str())),
        ("report_id", Json::str(r.report_id.clone())),
        (
            "results",
            Json::Arr(r.results.iter().map(report_result_json).collect()),
        ),
        ("run_id", Json::str(r.run_id.clone())),
        ("stale", Json::Bool(r.stale)),
        ("subject_kind", Json::str(r.subject_kind.as_str())),
        ("subject_ref", Json::str(r.subject_ref.clone())),
        ("suite_ref", Json::str(r.suite_ref.clone())),
    ])
}

fn report_from_json(j: &Json, path: &str) -> Result<ConformanceReport, RegistryError> {
    let mut results = Vec::new();
    for (i, e) in req_arr(j, "results", path)?.iter().enumerate() {
        let p = format!("{path}.results[{i}]");
        results.push(ReportResult {
            subject: req_str(e, "subject", &p)?,
            verdict: ConformanceVerdict::parse(&req_str(e, "verdict", &p)?).ok_or_else(|| {
                RegistryError::SchemaViolation {
                    path: format!("{p}.verdict"),
                    detail: "closed vocabulary".to_string(),
                }
            })?,
            evidence_ref: opt_str(e, "evidence_ref"),
        });
    }
    let mut probed = BTreeMap::new();
    for (k, v) in obj_entries(j, "probed_declaration", path)? {
        match v {
            Json::Str(s) => {
                probed.insert(
                    k.clone(),
                    ConformanceVerdict::parse(s).ok_or_else(|| RegistryError::SchemaViolation {
                        path: format!("{path}.probed_declaration.{k}"),
                        detail: "closed vocabulary".to_string(),
                    })?,
                );
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.probed_declaration.{k}"),
                    detail: "expected verdict string".to_string(),
                })
            }
        }
    }
    let hp = format!("{path}.host");
    let host = req(j, "host", path)?;
    Ok(ConformanceReport {
        report_id: req_str(j, "report_id", path)?,
        subject_kind: SubjectKind::parse(&req_str(j, "subject_kind", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.subject_kind"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        subject_ref: req_str(j, "subject_ref", path)?,
        suite_ref: req_str(j, "suite_ref", path)?,
        host: ReportHost {
            placement: Placement::parse(&req_str(host, "placement", &hp)?).ok_or_else(|| {
                RegistryError::SchemaViolation {
                    path: format!("{hp}.placement"),
                    detail: "closed vocabulary".to_string(),
                }
            })?,
            isolation: req_str(host, "isolation", &hp)?,
            instrument: instrument_from_json(
                req(host, "instrument", &hp)?,
                &format!("{hp}.instrument"),
            )?,
        },
        produced_by: ProducedBy::parse(&req_str(j, "produced_by", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.produced_by"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        results,
        probed_declaration: probed,
        run_id: req_str(j, "run_id", path)?,
        stale: req_bool(j, "stale", path)?,
    })
}

// ── NamespaceRecord / RegistryPolicy ─────────────────────────────────────────

fn owner_json(o: &OwnerRef) -> Json {
    match o {
        OwnerRef::Signer(s) => obj(vec![("signer", Json::str(s.clone()))]),
        OwnerRef::Principal(p) => obj(vec![("principal", Json::str(p.clone()))]),
    }
}

fn owner_from_json(j: &Json, path: &str) -> Result<OwnerRef, RegistryError> {
    if let Some(Json::Str(s)) = j.get("signer") {
        return Ok(OwnerRef::Signer(s.clone()));
    }
    if let Some(Json::Str(p)) = j.get("principal") {
        return Ok(OwnerRef::Principal(p.clone()));
    }
    Err(RegistryError::SchemaViolation {
        path: format!("{path}.owners"),
        detail: "owner must be signer or principal".to_string(),
    })
}

fn namespace_body_json(n: &NamespaceRecord) -> Json {
    obj(vec![
        ("namespace", Json::str(n.namespace.clone())),
        (
            "owners",
            Json::Arr(n.owners.iter().map(owner_json).collect()),
        ),
        ("require_signature", Json::Bool(n.require_signature)),
        ("who_may_deprecate", Json::str(n.who_may_deprecate.as_str())),
        ("who_may_publish", Json::str(n.who_may_publish.as_str())),
        ("who_may_revoke", Json::str(n.who_may_revoke.as_str())),
        ("who_may_yank", Json::str(n.who_may_yank.as_str())),
    ])
}

fn namespace_from_json(j: &Json, path: &str) -> Result<NamespaceRecord, RegistryError> {
    let rule = |k: &str| -> Result<PublishRule, RegistryError> {
        PublishRule::parse(&req_str(j, k, path)?).ok_or_else(|| RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "closed vocabulary".to_string(),
        })
    };
    let mut owners = Vec::new();
    for e in req_arr(j, "owners", path)? {
        owners.push(owner_from_json(e, path)?);
    }
    Ok(NamespaceRecord {
        namespace: req_str(j, "namespace", path)?,
        owners,
        who_may_publish: rule("who_may_publish")?,
        who_may_deprecate: rule("who_may_deprecate")?,
        who_may_yank: rule("who_may_yank")?,
        who_may_revoke: rule("who_may_revoke")?,
        require_signature: req_bool(j, "require_signature", path)?,
    })
}

fn foreign_import_body_json(fi: &ForeignImport) -> Json {
    obj(vec![
        ("digest", fi.digest.clone().map_or(Json::Null, Json::Str)),
        ("label", fi.label.clone().map_or(Json::Null, Json::Str)),
        ("lifted_record", fi.lifted_record.clone()),
        ("locator", Json::str(fi.locator.clone())),
        ("loss_report", str_vec(&fi.loss_report)),
        ("system", Json::str(fi.system.clone())),
    ])
}

fn foreign_import_from_json(j: &Json, path: &str) -> Result<ForeignImport, RegistryError> {
    Ok(ForeignImport {
        system: req_str(j, "system", path)?,
        locator: req_str(j, "locator", path)?,
        digest: opt_str(j, "digest"),
        label: opt_str(j, "label"),
        lifted_record: req(j, "lifted_record", path)?.clone(),
        loss_report: str_vec_dec(j, "loss_report", path)?,
    })
}

/// The canonical `RegistryPolicy` encoding (the snapshot pins its digest — AC-8).
/// `contract_version_policies` encodes only when non-empty — the pre-S1.27
/// canonical bytes are unchanged for a policy carrying no layered policies
/// (CC8 additive discipline).
pub fn policy_json(p: &RegistryPolicy) -> Json {
    let mut localities = BTreeMap::new();
    for (k, v) in &p.allowed_localities_by_origin {
        localities.insert(
            k.clone(),
            Json::Arr(v.iter().map(|x| Json::str(x.as_str())).collect()),
        );
    }
    let mut members = vec![
        (
            "admissible_report_producers",
            Json::Arr(
                p.admissible_report_producers
                    .iter()
                    .map(|x| Json::str(x.as_str()))
                    .collect(),
            ),
        ),
        ("allowed_foreign_systems", strs(&p.allowed_foreign_systems)),
        ("allowed_localities_by_origin", Json::Obj(localities)),
        ("collision_policy", Json::str(p.collision_policy.clone())),
        (
            "foreign_import_default_admission",
            Json::str(p.foreign_import_default_admission.as_str()),
        ),
        (
            "max_age_ms",
            p.max_age_ms.map_or(Json::Null, |v| Json::Int(v as i64)),
        ),
        (
            "require_conformance",
            Json::str(p.require_conformance.as_str()),
        ),
        (
            "require_signature_for_kinds",
            Json::Arr(
                p.require_signature_for_kinds
                    .iter()
                    .map(|k| Json::str(k.as_str()))
                    .collect(),
            ),
        ),
        (
            "require_trust_record_for_origins",
            strs(&p.require_trust_record_for_origins),
        ),
    ];
    if !p.contract_version_policies.is_empty() {
        members.push((
            "contract_version_policies",
            Json::Arr(
                p.contract_version_policies
                    .iter()
                    .map(|c| c.to_json())
                    .collect(),
            ),
        ));
    }
    obj(members)
}

/// Decode a `RegistryPolicy`.
pub fn policy_from_json(j: &Json, path: &str) -> Result<RegistryPolicy, RegistryError> {
    let mut producers = BTreeSet::new();
    for e in req_arr(j, "admissible_report_producers", path)? {
        match e {
            Json::Str(s) => {
                producers.insert(ProducedBy::parse(s).ok_or_else(|| {
                    RegistryError::SchemaViolation {
                        path: format!("{path}.admissible_report_producers"),
                        detail: format!("closed vocabulary: {s}"),
                    }
                })?);
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.admissible_report_producers"),
                    detail: "expected string".to_string(),
                })
            }
        }
    }
    let mut sig_kinds = BTreeSet::new();
    for e in req_arr(j, "require_signature_for_kinds", path)? {
        match e {
            Json::Str(s) => {
                sig_kinds.insert(
                    RecordKind::parse(s)
                        .ok_or_else(|| RegistryError::UnknownRecordKind { kind: s.clone() })?,
                );
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.require_signature_for_kinds"),
                    detail: "expected string".to_string(),
                })
            }
        }
    }
    let mut localities = BTreeMap::new();
    for (k, v) in obj_entries(j, "allowed_localities_by_origin", path)? {
        let mut set = BTreeSet::new();
        match v {
            Json::Arr(a) => {
                for e in a {
                    match e {
                        Json::Str(s) => {
                            set.insert(Placement::parse(s).ok_or_else(|| {
                                RegistryError::SchemaViolation {
                                    path: format!("{path}.allowed_localities_by_origin.{k}"),
                                    detail: format!("closed vocabulary: {s}"),
                                }
                            })?);
                        }
                        _ => {
                            return Err(RegistryError::SchemaViolation {
                                path: format!("{path}.allowed_localities_by_origin.{k}"),
                                detail: "expected string".to_string(),
                            })
                        }
                    }
                }
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.allowed_localities_by_origin"),
                    detail: "expected object of arrays".to_string(),
                })
            }
        }
        localities.insert(k.clone(), set);
    }
    Ok(RegistryPolicy {
        require_conformance: RequireConformance::parse(&req_str(j, "require_conformance", path)?)
            .ok_or_else(|| RegistryError::SchemaViolation {
            path: format!("{path}.require_conformance"),
            detail: "closed vocabulary".to_string(),
        })?,
        admissible_report_producers: producers,
        require_trust_record_for_origins: str_set(j, "require_trust_record_for_origins", path)?,
        require_signature_for_kinds: sig_kinds,
        max_age_ms: match req(j, "max_age_ms", path)? {
            Json::Null => None,
            Json::Int(i) if *i >= 0 => Some(*i as u64),
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.max_age_ms"),
                    detail: "expected u64 or null".to_string(),
                })
            }
        },
        collision_policy: req_str(j, "collision_policy", path)?,
        foreign_import_default_admission: Admission::parse(&req_str(
            j,
            "foreign_import_default_admission",
            path,
        )?)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: format!("{path}.foreign_import_default_admission"),
            detail: "closed vocabulary".to_string(),
        })?,
        allowed_localities_by_origin: localities,
        allowed_foreign_systems: str_set(j, "allowed_foreign_systems", path)?,
        contract_version_policies: match j.get("contract_version_policies") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    hh_plugin::ContractVersionPolicy::from_json(
                        v,
                        &format!("{path}.contract_version_policies[{i}]"),
                    )
                    .map_err(|e| match e {
                        hh_plugin::ContractRefError::SchemaViolation { path, detail } => {
                            RegistryError::SchemaViolation { path, detail }
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.contract_version_policies"),
                    detail: "expected array of ContractVersionPolicy".to_string(),
                })
            }
        },
    })
}

// ── RegistrySnapshot ─────────────────────────────────────────────────────────

fn snapshot_body_json(s: &RegistrySnapshot) -> Json {
    let mut bindings = BTreeMap::new();
    for ((ns, name), v) in &s.name_bindings {
        bindings.insert(format!("{ns}/{name}"), Json::str(v.clone()));
    }
    obj(vec![
        ("created_seq", Json::Int(s.created_seq as i64)),
        ("members", strs(&s.members)),
        ("name_bindings", Json::Obj(bindings)),
        ("policy_digest", Json::str(s.policy_digest.clone())),
        ("snapshot_id", Json::str(s.snapshot_id.clone())),
    ])
}

fn snapshot_from_json(j: &Json, path: &str) -> Result<RegistrySnapshot, RegistryError> {
    let mut bindings = BTreeMap::new();
    for (k, v) in obj_entries(j, "name_bindings", path)? {
        match v {
            Json::Str(id) => {
                let (ns, name) =
                    k.split_once('/')
                        .ok_or_else(|| RegistryError::SchemaViolation {
                            path: format!("{path}.name_bindings.{k}"),
                            detail: "expected <namespace>/<name>".to_string(),
                        })?;
                bindings.insert((ns.to_string(), name.to_string()), id.clone());
            }
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.name_bindings"),
                    detail: "expected version_id strings".to_string(),
                })
            }
        }
    }
    Ok(RegistrySnapshot {
        snapshot_id: req_str(j, "snapshot_id", path)?,
        members: str_set(j, "members", path)?,
        name_bindings: bindings,
        policy_digest: req_str(j, "policy_digest", path)?,
        created_seq: req_u64(j, "created_seq", path)?,
    })
}

// ── RegistryDiagnostic ───────────────────────────────────────────────────────

/// The canonical `RegistryDiagnostic` encoding.
pub fn diagnostic_json(d: &RegistryDiagnostic) -> Json {
    obj(vec![
        ("operation", Json::str(d.operation.clone())),
        ("reason", Json::str(d.reason.clone())),
        ("registrar", d.registrar.to_json()),
        ("seq", Json::Int(d.seq as i64)),
        ("subject", d.subject.clone().map_or(Json::Null, Json::Str)),
    ])
}

/// Decode a `RegistryDiagnostic`.
pub fn diagnostic_from_json(j: &Json, path: &str) -> Result<RegistryDiagnostic, RegistryError> {
    Ok(RegistryDiagnostic {
        operation: req_str(j, "operation", path)?,
        reason: req_str(j, "reason", path)?,
        subject: opt_str(j, "subject"),
        registrar: ProvenanceRecord::from_json(req(j, "registrar", path)?).map_err(|e| {
            RegistryError::SchemaViolation {
                path: format!("{path}.registrar"),
                detail: format!("{e:?}"),
            }
        })?,
        seq: req_u64(j, "seq", path)?,
    })
}

// ── RegistryEnvelope ─────────────────────────────────────────────────────────

/// The canonical `RegistryEnvelope` encoding.
pub fn envelope_json(e: &RegistryEnvelope) -> Json {
    let mut m = BTreeMap::new();
    m.insert("admission".to_string(), Json::str(e.admission.as_str()));
    m.insert(
        "dialect_range".to_string(),
        Json::str(e.dialect_range.clone()),
    );
    m.insert("kind".to_string(), Json::str(e.kind.as_str()));
    m.insert(
        "name_history_ref".to_string(),
        e.name_history_ref.clone().map_or(Json::Null, Json::Str),
    );
    m.insert(
        "registered_at".to_string(),
        Json::Int(e.registered_at as i64),
    );
    m.insert("registrar".to_string(), e.registrar.to_json());
    m.insert("registry_dialect".to_string(), Json::str(REGISTRY_DIALECT));
    m.insert(
        "semantic_id".to_string(),
        e.semantic_id.clone().map_or(Json::Null, Json::Str),
    );
    m.insert(
        "trust_record_ref".to_string(),
        e.trust_record_ref.clone().map_or(Json::Null, Json::Str),
    );
    m.insert("version_id".to_string(), Json::str(e.version_id.clone()));
    if !e.ext.is_empty() {
        m.insert(
            "ext".to_string(),
            Json::Obj(e.ext.clone().into_iter().collect()),
        );
    }
    Json::Obj(m)
}

/// Decode a `RegistryEnvelope` — `ext` members are preserved verbatim but a
/// status-shaped `ext.status` outside the three vocabularies fails (R5/R9).
pub fn envelope_from_json(j: &Json, path: &str) -> Result<RegistryEnvelope, RegistryError> {
    let kind_s = req_str(j, "kind", path)?;
    let kind =
        RecordKind::parse(&kind_s).ok_or(RegistryError::UnknownRecordKind { kind: kind_s })?;
    if let Some(Json::Str(s)) = j.get("status") {
        reject_unknown_status(s, path)?;
    }
    let mut ext = BTreeMap::new();
    if let Some(Json::Obj(x)) = j.get("ext") {
        for (k, v) in x {
            if k == "status" {
                if let Json::Str(s) = v {
                    reject_unknown_status(s, &format!("{path}.ext"))?;
                }
            }
            ext.insert(k.clone(), v.clone());
        }
    }
    Ok(RegistryEnvelope {
        kind,
        version_id: req_str(j, "version_id", path)?,
        semantic_id: opt_str(j, "semantic_id"),
        registered_at: req_u64(j, "registered_at", path)?,
        registrar: ProvenanceRecord::from_json(req(j, "registrar", path)?).map_err(|e| {
            RegistryError::SchemaViolation {
                path: format!("{path}.registrar"),
                detail: format!("{e:?}"),
            }
        })?,
        admission: Admission::parse(&req_str(j, "admission", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.admission"),
                detail: "closed vocabulary".to_string(),
            }
        })?,
        name_history_ref: opt_str(j, "name_history_ref"),
        trust_record_ref: opt_str(j, "trust_record_ref"),
        dialect_range: req_str(j, "dialect_range", path)?,
        ext,
    })
}

// ── The record-body dispatcher ───────────────────────────────────────────────

/// The canonical body encoding of a record — `semantic = true` drops the
/// identity-exempt surface fields (the `version_id` projection — N6).
pub fn body_json(r: &RegistryRecord, semantic: bool) -> Json {
    match r {
        RegistryRecord::Class(c) => class_body_json(c),
        RegistryRecord::Variant(v) => variant_body_json(v, semantic),
        RegistryRecord::Suite(s) => suite_body_json(s),
        RegistryRecord::Report(r) => report_body_json(r),
        RegistryRecord::Namespace(n) => namespace_body_json(n),
        RegistryRecord::Snapshot(s) => snapshot_body_json(s),
        RegistryRecord::ForeignImport(f) => foreign_import_body_json(f),
        // The capability body IS the canonical HIR node (V-E1-9 — the `semantic`
        // flag is irrelevant: the node's own codec already splits identity).
        RegistryRecord::Capability(c) => hh_hir::wire::node_to_json(&c.node),
        // The eval declarations' canonical bodies are the ontology codecs (CC7 —
        // the registry re-encodes, never re-schemas).
        RegistryRecord::MetricDeclaration(m) => m.to_json(),
        RegistryRecord::Validator(o) => o.to_json(),
        RegistryRecord::Extension(e) => crate::extension::extension_body_json(e),
        RegistryRecord::EnvironmentFamily(f) => f.to_json(),
    }
}

/// The semantic-coordinate body (for `semantic_id`) — identity core minus the
/// rename-stable `variant_id`.
pub fn semantic_projection_json(r: &RegistryRecord) -> Option<Json> {
    match r {
        RegistryRecord::Variant(v) => Some(variant_semantic_projection(v)),
        RegistryRecord::Class(c) => {
            let mut j = class_body_json(c);
            if let Json::Obj(m) = &mut j {
                m.remove("class_id");
            }
            Some(j)
        }
        RegistryRecord::Capability(c) => Some(hh_hir::wire::node_semantic_projection(&c.node)),
        _ => None,
    }
}

/// Decode a record body of `kind` (R2 — an unlisted kind is `UnknownRecordKind`;
/// a kind without a landed Stage-1 schema is `SchemaViolation{path: "kind"}`).
pub fn record_from_json(kind: RecordKind, j: &Json) -> Result<RegistryRecord, RegistryError> {
    let path = "record";
    match kind {
        RecordKind::Class => Ok(RegistryRecord::Class(class_from_json(j, path)?)),
        RecordKind::Variant => Ok(RegistryRecord::Variant(variant_from_json(j, path)?)),
        RecordKind::ConformanceSuite => Ok(RegistryRecord::Suite(suite_from_json(j, path)?)),
        RecordKind::ConformanceReport => Ok(RegistryRecord::Report(report_from_json(j, path)?)),
        RecordKind::Namespace => Ok(RegistryRecord::Namespace(namespace_from_json(j, path)?)),
        RecordKind::RegistrySnapshot => Ok(RegistryRecord::Snapshot(snapshot_from_json(j, path)?)),
        RecordKind::ForeignImport => Ok(RegistryRecord::ForeignImport(foreign_import_from_json(
            j, path,
        )?)),
        RecordKind::Capability => {
            let node =
                hh_hir::wire::node_from_json(j).map_err(|e| RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: format!("capability node: {e}"),
                })?;
            if node.kind != hh_hir::kinds::EntityKind::ToolCapability {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.kind"),
                    detail: format!(
                        "capability body must be a tool_capability node, not {}",
                        node.kind.name()
                    ),
                });
            }
            Ok(RegistryRecord::Capability(CapabilityRecord { node }))
        }
        RecordKind::MetricDeclaration => Ok(RegistryRecord::MetricDeclaration(
            hh_ontology::compliance::MetricDeclaration::from_json(j).map_err(|d| {
                RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: d,
                }
            })?,
        )),
        RecordKind::Validator => Ok(RegistryRecord::Validator(
            hh_ontology::eval::OracleDeclaration::from_json(j).map_err(|e| {
                RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: format!("{e:?}"),
                }
            })?,
        )),
        RecordKind::EnvironmentFamily => Ok(RegistryRecord::EnvironmentFamily(
            hh_ontology::lab::EnvironmentFamilyRecord::from_json(j).map_err(|e| {
                RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: format!("{e:?}"),
                }
            })?,
        )),
        RecordKind::Extension => {
            let rec = crate::extension::extension_from_json(j, path)?;
            // L1–L3 checks are the schema gate (LocationElevation/LegCrossing
            // refuse at decode + register, never at first use).
            crate::extension::validate_extension_record(&rec)?;
            Ok(RegistryRecord::Extension(rec))
        }
        other if !other.has_stage1_schema() => Err(RegistryError::SchemaViolation {
            path: "kind".to_string(),
            detail: format!(
                "kind {} is in the closed registry/1 list but has no Stage-1 record schema",
                other.as_str()
            ),
        }),
        _ => unreachable!("all Stage-1 schema kinds handled"),
    }
}

// ── The canonical log line ────────────────────────────────────────────────────

/// A canonical store log line `{envelope, record}` — the whole store serializes
/// as the sorted sequence of these (one canonical form, verified by `verify()`).
pub fn log_line_json(e: &RegistryEnvelope, r: &RegistryRecord) -> Json {
    obj(vec![
        ("envelope", envelope_json(e)),
        ("record", body_json(r, false)),
    ])
}

/// Decode a log line.
pub fn log_line_from_json(j: &Json) -> Result<(RegistryEnvelope, RegistryRecord), RegistryError> {
    let env = envelope_from_json(req(j, "envelope", "line")?, "envelope")?;
    let rec = record_from_json(env.kind, req(j, "record", "line")?)?;
    Ok((env, rec))
}

/// Encode the whole canonical log: `[{envelope, record}]*` sorted by `version_id`.
pub fn encode_log(records: &BTreeMap<String, (RegistryEnvelope, RegistryRecord)>) -> String {
    let lines: Vec<Json> = records
        .iter()
        .map(|(_, (e, r))| log_line_json(e, r))
        .collect();
    Json::Arr(lines).to_canonical_string()
}

/// Decode the whole canonical log.
pub fn decode_log(
    bytes: &[u8],
) -> Result<BTreeMap<String, (RegistryEnvelope, RegistryRecord)>, RegistryError> {
    let text = std::str::from_utf8(bytes).map_err(|e| RegistryError::SchemaViolation {
        path: "log".to_string(),
        detail: format!("{e}"),
    })?;
    let j = json::parse(text).map_err(|e| RegistryError::SchemaViolation {
        path: "log".to_string(),
        detail: format!("{e:?}"),
    })?;
    let mut out = BTreeMap::new();
    match j {
        Json::Arr(lines) => {
            for l in &lines {
                let (e, r) = log_line_from_json(l)?;
                out.insert(e.version_id.clone(), (e, r));
            }
        }
        _ => {
            return Err(RegistryError::SchemaViolation {
                path: "log".to_string(),
                detail: "expected array".to_string(),
            })
        }
    }
    Ok(out)
}

/// The `NameStatus` canonical spellings — the **reused** publication vocabulary
/// (CC1 — the store never defines a second one).
pub fn name_status_str(s: NameStatus) -> &'static str {
    match s {
        NameStatus::Active => "active",
        NameStatus::Deprecated => "deprecated",
        NameStatus::Yanked => "yanked",
    }
}
