//! The two Stage-1 `ClassRecord`s — `control_strategy` and `context_policy` — with
//! their pinned `ConformanceSuite`s, plus the minimal in-crate suite driver
//! (ADR-0152 (d): at Stage 1 the out-of-process tests run **in the reference
//! runtime's test harness** — this driver is that harness; the variant host is
//! Stage 2). The driver is deterministic and records-in/records-out: it reads the
//! variant's canonical record (never `implementation.content` — R3) and produces a
//! `conformance_report` body the caller registers.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::kinds::{
    ConformanceVerdict, OracleClass, Placement, ProducedBy, SubjectKind, TestDriver, TestKind,
};
use crate::records::{
    ClassRecord, ConformanceReport, ConformanceSuite, ContractOperation, ReportHost, ReportResult,
    SuiteTest, VariantRecord,
};
use hh_identity::repro::InstrumentRecord;

/// `control_strategy` — the per-step control policy class (hot path).
pub fn control_strategy_class() -> ClassRecord {
    ClassRecord {
        class_id: "control_strategy".to_string(),
        contract: vec![
            ContractOperation {
                name: "decide".to_string(),
                inputs: Json::obj([
                    ("step", Json::str("context")),
                    ("budget", Json::str("resource_account")),
                ]),
                outputs: Json::obj([("decision", Json::str("closed vocabulary"))]),
                invariants: vec![
                    "never spends beyond the account".to_string(),
                    "deterministic for the same inputs".to_string(),
                ],
                failure_modes: vec!["refuse".to_string(), "fallback".to_string()],
            },
            ContractOperation {
                name: "on_budget_exhausted".to_string(),
                inputs: Json::obj([("account", Json::str("resource_account"))]),
                outputs: Json::obj([("decision", Json::str("closed vocabulary"))]),
                invariants: vec!["matched-budget-or-refuse".to_string()],
                failure_modes: vec!["refuse".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: true,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("supports_escalation", Json::Null),
                    ("supports_fallback", Json::Null),
                    ("deterministic", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None, // pinned at registration time by the caller
        decision_points: vec!["control.strategy".to_string()],
        metrics_declared: vec!["refusals".to_string()],
        slot_key: "control_strategy".to_string(),
        tier: "C0".to_string(),
    }
}

/// `context_policy` — the context assembly policy class (hot path).
pub fn context_policy_class() -> ClassRecord {
    ClassRecord {
        class_id: "context_policy".to_string(),
        contract: vec![
            ContractOperation {
                name: "assemble".to_string(),
                inputs: Json::obj([
                    ("turn", Json::str("context")),
                    ("budget", Json::str("resource_account")),
                ]),
                outputs: Json::obj([("context", Json::str("assembled"))]),
                invariants: vec![
                    "never exceeds the declared context window".to_string(),
                    "deterministic for the same inputs".to_string(),
                ],
                failure_modes: vec!["refuse".to_string(), "truncate".to_string()],
            },
            ContractOperation {
                name: "redact".to_string(),
                inputs: Json::obj([("context", Json::str("assembled"))]),
                outputs: Json::obj([("context", Json::str("redacted"))]),
                invariants: vec!["secret-shaped content never crosses".to_string()],
                failure_modes: vec!["refuse".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: true,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("supports_redaction", Json::Null),
                    ("max_window_tokens", Json::Null),
                    ("deterministic", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec!["context.policy".to_string()],
        metrics_declared: vec!["truncations".to_string()],
        slot_key: "context_policy".to_string(),
        tier: "C0".to_string(),
    }
}

/// The `control_strategy` conformance suite — one test per kind (ADR-0152 D1/D2;
/// deterministic oracles only at C0).
pub fn control_strategy_suite(class_version_id: &str) -> ConformanceSuite {
    ConformanceSuite {
        suite_id: "control_strategy.c0".to_string(),
        class_ref: class_version_id.to_string(),
        contract_version: "1.0".to_string(),
        tests: vec![
            SuiteTest {
                test_id: "static.declaration".to_string(),
                kind: TestKind::Static,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "all declared fields present".to_string(),
            },
            SuiteTest {
                test_id: "contract.decide".to_string(),
                kind: TestKind::Contract,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "contract_range covers the class version".to_string(),
            },
            SuiteTest {
                test_id: "executable.debt".to_string(),
                kind: TestKind::Executable,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "conditioned rules carry complete debt records".to_string(),
            },
            SuiteTest {
                test_id: "property.placement".to_string(),
                kind: TestKind::Property,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "placement declared and not reserved".to_string(),
            },
        ],
        required_for_status: BTreeSet::from([
            "static.declaration".to_string(),
            "contract.decide".to_string(),
            "executable.debt".to_string(),
            "property.placement".to_string(),
        ]),
    }
}

/// The `context_policy` conformance suite.
pub fn context_policy_suite(class_version_id: &str) -> ConformanceSuite {
    ConformanceSuite {
        suite_id: "context_policy.c0".to_string(),
        class_ref: class_version_id.to_string(),
        contract_version: "1.0".to_string(),
        tests: vec![
            SuiteTest {
                test_id: "static.declaration".to_string(),
                kind: TestKind::Static,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "all declared fields present".to_string(),
            },
            SuiteTest {
                test_id: "contract.assemble".to_string(),
                kind: TestKind::Contract,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "contract_range covers the class version".to_string(),
            },
            SuiteTest {
                test_id: "executable.debt".to_string(),
                kind: TestKind::Executable,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "conditioned rules carry complete debt records".to_string(),
            },
            SuiteTest {
                test_id: "property.placement".to_string(),
                kind: TestKind::Property,
                fixture_ref: None,
                driver: TestDriver::InProcess,
                oracle_class: OracleClass::Deterministic,
                budget: Json::obj([("max_ms", Json::Int(100))]),
                verdict_rule: "placement declared and not reserved".to_string(),
            },
        ],
        required_for_status: BTreeSet::from([
            "static.declaration".to_string(),
            "contract.assemble".to_string(),
            "executable.debt".to_string(),
            "property.placement".to_string(),
        ]),
    }
}

/// Run a suite over a variant record — the Stage-1 in-crate driver (ADR-0152 (d)).
/// Deterministic: same records in, same verdicts out. Never touches
/// `implementation.content` (R3 — the registry never loads bodies).
pub fn run_suite(
    suite: &ConformanceSuite,
    variant: &VariantRecord,
    variant_version_id: &str,
    class_contract_version: &str,
    instrument: &InstrumentRecord,
    run_id: &str,
) -> ConformanceReport {
    let mut results = Vec::new();
    for t in &suite.tests {
        let verdict = match t.kind {
            TestKind::Static => {
                if variant.capability_declaration.is_empty() {
                    ConformanceVerdict::Unsupported
                } else {
                    ConformanceVerdict::Supported
                }
            }
            TestKind::Contract => {
                if crate::store::range_covers(&variant.contract_range, class_contract_version) {
                    ConformanceVerdict::Supported
                } else {
                    ConformanceVerdict::Unsupported
                }
            }
            TestKind::Executable => {
                let complete = variant.conditioned_rules.iter().all(|(rid, d)| {
                    !rid.is_empty()
                        && !d.rule_id.is_empty()
                        && !d.owner.is_empty()
                        && !d.expiry_condition.is_empty()
                        && !d.removal_test_ref.is_empty()
                });
                if complete {
                    ConformanceVerdict::Supported
                } else {
                    ConformanceVerdict::Unsupported
                }
            }
            TestKind::Property => {
                if variant.implementation.placement != Placement::ComponentModel {
                    ConformanceVerdict::Supported
                } else {
                    ConformanceVerdict::Unsupported
                }
            }
        };
        results.push(ReportResult {
            subject: t.test_id.clone(),
            verdict,
            evidence_ref: None,
        });
    }
    let all_required_ok = suite.required_for_status.iter().all(|tid| {
        results
            .iter()
            .any(|r| r.subject == *tid && r.verdict == ConformanceVerdict::Supported)
    });
    let probed: BTreeMap<String, ConformanceVerdict> = variant
        .capability_declaration
        .keys()
        .map(|k| {
            (
                k.clone(),
                if all_required_ok {
                    ConformanceVerdict::Supported
                } else {
                    ConformanceVerdict::Unsupported
                },
            )
        })
        .collect();
    ConformanceReport {
        report_id: format!("{}:{}", suite.suite_id, variant_version_id),
        subject_kind: SubjectKind::Variant,
        subject_ref: variant_version_id.to_string(),
        suite_ref: String::new(), // filled by the caller (the suite's version_id)
        host: ReportHost {
            placement: variant.implementation.placement,
            isolation: "in_crate_driver".to_string(),
            instrument: instrument.clone(),
        },
        produced_by: ProducedBy::RegistryCi,
        results,
        probed_declaration: probed,
        run_id: run_id.to_string(),
        stale: false,
    }
}
