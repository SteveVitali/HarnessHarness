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
                    ("steer_mode", Json::Null),
                    ("concurrent_input", Json::Null),
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
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
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
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
    }
}

/// `validator` — the verification-plane `Validator` component class
/// (spec §5f.1; ADR-0110/0111; S1.21). Ordered-many: several validators may
/// bind at once (the kernel local checks (a)–(c) ride the reference
/// `hir/kernel/local_checks` node; declaration-side `ordered-many` keeps the
/// C1+ slots open). `required_inputs ⊇ {ModelProfile, ResourceAccount}` —
/// AC-R-2.7.1-11 / T-LCD-08.
pub fn validator_class() -> ClassRecord {
    ClassRecord {
        class_id: "validator".to_string(),
        contract: vec![
            ContractOperation {
                name: "declare".to_string(),
                inputs: Json::obj([("validator_ref", Json::str("ref"))]),
                outputs: Json::obj([("declaration", Json::str("ValidatorDeclaration"))]),
                invariants: vec![
                    "evidence_inputs never empty (I-V1)".to_string(),
                    "kind = judge ⇒ ¬deterministic ∧ profile_ref ∧ assumption_debt".to_string(),
                    "threshold-conditioned ⇒ assumption_debt (AC-R-2.7.1-12)".to_string(),
                ],
                failure_modes: vec!["IncompleteDeclaration".to_string()],
            },
            ContractOperation {
                name: "bind".to_string(),
                inputs: Json::obj([
                    ("validator_ref", Json::str("ref")),
                    ("profile", Json::str("model_profile")),
                    ("account", Json::str("resource_account")),
                ]),
                outputs: Json::obj([("bound", Json::str("BoundValidator"))]),
                invariants: vec![
                    "a validator is effect-free — side_effects ⊆ {fs_read}".to_string()
                ],
                failure_modes: vec![
                    "PayloadHashMismatch".to_string(),
                    "UnboundFixture".to_string(),
                    "ValidatorHasEffects".to_string(),
                ],
            },
            ContractOperation {
                name: "collect".to_string(),
                inputs: Json::obj([
                    ("bound", Json::str("BoundValidator")),
                    ("target", Json::str("TargetRef")),
                    ("cursor", Json::str("ledger_cursor")),
                ]),
                outputs: Json::obj([("bundle", Json::str("EvidenceBundle"))]),
                invariants: vec![
                    "the kernel resolves handles — the validator body never fetches".to_string(),
                    "claims are never evidence (ClaimOnlyEvidence)".to_string(),
                ],
                failure_modes: vec![
                    "EvidenceMissing".to_string(),
                    "EvidenceStale".to_string(),
                    "EvidenceUnauthoritative".to_string(),
                    "ClaimOnlyEvidence".to_string(),
                    "EvidenceTampered".to_string(),
                ],
            },
            ContractOperation {
                name: "check".to_string(),
                inputs: Json::obj([
                    ("bound", Json::str("BoundValidator")),
                    ("bundle", Json::str("EvidenceBundle")),
                ]),
                outputs: Json::obj([("verdict", Json::str("Verdict"))]),
                invariants: vec![
                    "pure in (inputs_digest, validator version_id) when deterministic".to_string(),
                    "a verdict never rewrites its inputs".to_string(),
                ],
                failure_modes: vec!["OracleFailure".to_string(), "BudgetExhausted".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::OrderedMany,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("kind", Json::Null),
                    ("oracle_class", Json::Null),
                    ("deterministic", Json::Null),
                    ("evidence_inputs", Json::Null),
                    ("evidence_out", Json::Null),
                    ("verdict_type", Json::Null),
                    ("requires_observability", Json::Null),
                    ("cost_model", Json::Null),
                    ("isolation", Json::Null),
                    ("side_effects", Json::Null),
                    ("profile_ref", Json::Null),
                    ("calibration_ref", Json::Null),
                    ("charged_to", Json::Null),
                    ("assumption_debt", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            (
                "required",
                Json::Arr(vec![
                    Json::str("kind"),
                    Json::str("oracle_class"),
                    Json::str("deterministic"),
                    Json::str("evidence_inputs"),
                    Json::str("verdict_type"),
                    Json::str("isolation"),
                    Json::str("charged_to"),
                ]),
            ),
        ]),
        conformance_suite_ref: None,
        decision_points: vec!["verify".to_string()],
        metrics_declared: vec![
            "claim_state_agreement".to_string(),
            "false_completion_rate".to_string(),
        ],
        slot_key: "validator".to_string(),
        tier: "C0".to_string(),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
    }
}

/// `execution_alignment` — the claim-reconciliation component class
/// (spec §5f.2; ADR-0112/0114; S1.21). The C2 variant adds D1/D4/D7–D10,
/// `probe` mode and judged triage; the Stage-1 registration is the
/// **floors-only default** — the slot exists, the deterministic D2/D3/D5/D6
/// register and the `SeverityRecord` shape are the floor (ADR-0114 D4/D5:
/// "interventions on vs off" stays a matched-budget factor, never a
/// permanent guardrail).
pub fn execution_alignment_class() -> ClassRecord {
    ClassRecord {
        class_id: "execution_alignment".to_string(),
        contract: vec![
            ContractOperation {
                name: "bind".to_string(),
                inputs: Json::obj([("claim", Json::str("Claim"))]),
                outputs: Json::obj([("handles", Json::str("[HandleRef]"))]),
                invariants: vec![
                    "a handle is a record at authority ≥ environment — never the model's own text"
                        .to_string(),
                    "Unbindable ⇒ unverifiable, never agree".to_string(),
                ],
                failure_modes: vec!["Unbindable".to_string()],
            },
            ContractOperation {
                name: "reconcile".to_string(),
                inputs: Json::obj([
                    ("claim", Json::str("Claim")),
                    ("handles", Json::str("[AuthoritativeHandle]")),
                ]),
                outputs: Json::obj([("record", Json::str("ReconciliationRecord"))]),
                invariants: vec![
                    "ledger_only mode is a pure fold over the durable prefix".to_string(),
                    "unachievable claims are the honest-failure channel — never a divergence alone"
                        .to_string(),
                ],
                failure_modes: vec!["Unreconciled".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::Optional,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("divergence_classes", Json::Null),
                    ("modes", Json::Null),
                    ("deterministic", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec!["stop".to_string()],
        metrics_declared: vec![
            "claim_state_agreement".to_string(),
            "execution_alignment_failure_rate".to_string(),
            "false_completion_rate".to_string(),
            "honest_failure_rate".to_string(),
            "gate_hold_count".to_string(),
        ],
        slot_key: "execution_alignment".to_string(),
        tier: "C0".to_string(),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
    }
}

/// `compaction_strategy` — the context-compaction component class (spec §5c
/// R-2.4.2 / §8.4's packaged-variant family; ADR-0075/0146; S2.2). Ordered-many,
/// decision point `compact`, the `{assess, propose, execute, declare}` contract
/// over the closed op sum `{Evict, Offload, Summarize, Restructure}`; the
/// deterministic C0 `evict_oldest` variant is the packaged first-party variant
/// this stage ships.
pub fn compaction_strategy_class() -> ClassRecord {
    ClassRecord {
        class_id: "compaction_strategy".to_string(),
        contract: vec![
            ContractOperation {
                name: "assess".to_string(),
                inputs: Json::obj([
                    ("view", Json::str("context_view")),
                    ("requirement", Json::str("compaction_requirement")),
                ]),
                outputs: Json::obj([("assessment", Json::str("{required, reason}"))]),
                invariants: vec![
                    "a pure fold over the view + requirement (deterministic)".to_string(),
                    "a `hard` requirement never reports `required = false` when occupancy > cap"
                        .to_string(),
                ],
                failure_modes: vec!["refuse".to_string()],
            },
            ContractOperation {
                name: "propose".to_string(),
                inputs: Json::obj([
                    ("assessment", Json::str("{required, reason}")),
                    ("view", Json::str("context_view")),
                ]),
                outputs: Json::obj([("proposal", Json::str("{ops: [CompactionOp]}"))]),
                invariants: vec![
                    "every op names the items it consumes".to_string(),
                    "ops ∈ {Evict, Offload, Summarize, Restructure}".to_string(),
                ],
                failure_modes: vec!["CompactionImpossible".to_string()],
            },
            ContractOperation {
                name: "execute".to_string(),
                inputs: Json::obj([
                    ("proposal", Json::str("{ops: [CompactionOp]}")),
                    ("view", Json::str("context_view")),
                ]),
                outputs: Json::obj([("result", Json::str("compaction_result"))]),
                invariants: vec![
                    "Evict emits one `kernel`-authority omission item per contiguous evicted range"
                        .to_string(),
                    "deterministic variants apply the proposal verbatim".to_string(),
                ],
                failure_modes: vec!["CompactionImpossible".to_string()],
            },
            ContractOperation {
                name: "declare".to_string(),
                inputs: Json::obj([]),
                outputs: Json::obj([("declaration", Json::str("variant_declaration"))]),
                invariants: vec![
                    "deterministic ⇒ no model call anywhere in the variant".to_string(),
                    "op_kinds ⊆ {evict, offload, summarize, restructure}".to_string(),
                ],
                failure_modes: vec!["IncompleteDeclaration".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::OrderedMany,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("deterministic", Json::Null),
                    ("op_kinds", Json::Null),
                    ("control_boundary_compact", Json::Null),
                    ("target_fraction", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            (
                "required",
                Json::Arr(vec![Json::str("deterministic"), Json::str("op_kinds")]),
            ),
        ]),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
        conformance_suite_ref: None,
        decision_points: vec!["compact".to_string()],
        metrics_declared: vec!["reclaimed_tokens".to_string()],
        slot_key: "compaction_strategy".to_string(),
        tier: "C0".to_string(),
    }
}

/// The `compaction_strategy` conformance suite (same deterministic four-test
/// shape as the Stage-1 classes).
pub fn compaction_strategy_suite(class_version_id: &str) -> ConformanceSuite {
    ConformanceSuite {
        suite_id: "compaction_strategy.c0".to_string(),
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
                test_id: "contract.assess".to_string(),
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
            "contract.assess".to_string(),
            "executable.debt".to_string(),
            "property.placement".to_string(),
        ]),
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
                        && !d.evidence_refs.is_empty()
                        && !d.owner.id.is_empty()
                        && !d.removal_test_ref.is_empty()
                        && d.removal_test.is_some()
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
