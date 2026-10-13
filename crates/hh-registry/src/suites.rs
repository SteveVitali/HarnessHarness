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

/// `routing_policy` — the §5b.2 router class (R-2.7; DF-S1.18-1). The
/// bound variant carries the sealed `RoutingPolicy` document in its slot
/// `params` (`policy` member); the kernel's scripted boundary interprets
/// the offline-decidable arms (`static`/`role_table`/`fallback_chain` +
/// the `error_actions` consult) and answers typed refusals for
/// live-signal-dependent arms — never a fabricated route.
pub fn routing_policy_class() -> ClassRecord {
    ClassRecord {
        class_id: "routing_policy".to_string(),
        contract: vec![
            ContractOperation {
                name: "select".to_string(),
                inputs: Json::obj([
                    ("request", Json::str("RoutingRequest")),
                    ("profile_env", Json::str("SelectorView")),
                ]),
                outputs: Json::obj([("decision", Json::str("RoutingDecision"))]),
                invariants: vec![
                    "reserve-before-return (G-4)".to_string(),
                    "every candidate carries a verdict (R-3)".to_string(),
                    "deterministic for the same inputs".to_string(),
                ],
                failure_modes: vec!["refuse".to_string()],
            },
            ContractOperation {
                name: "on_attempt_failed".to_string(),
                inputs: Json::obj([
                    ("error", Json::str("ModelErrorClass")),
                    ("attempts", Json::str("AttemptState")),
                ]),
                outputs: Json::obj([("disposition", Json::str("AttemptDisposition"))]),
                invariants: vec![
                    "the action reads the sealed error_actions table — never message text"
                        .to_string(),
                ],
                failure_modes: vec!["refuse".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::Optional,
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
            ("properties", Json::obj([("deterministic", Json::Null)])),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec!["control.route".to_string()],
        metrics_declared: vec!["refusals".to_string()],
        slot_key: "router".to_string(),
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
        hosted_entries: Vec::new(),
    }
}

/// `compute_policy` — the value-of-compute scheduler class (spec §5e.4
/// R-2.6.4; ADR-0188/0189/0190; S4.7). `exactly-one`, homed on the control
/// plane, invoked between `decide` and `envelope.check`; binds/tightens
/// unbound parameters or advises model-owned points — never proposes,
/// refuses, widens or owns β (B-1…B-6). The packaged `static` null variant
/// is the default; `uniform`/`rules` are the C3 recipe arms;
/// `bandit`/`surface_prior`/`predictor` land at Stage 5/6.
pub fn compute_policy_class() -> ClassRecord {
    ClassRecord {
        class_id: "compute_policy".to_string(),
        contract: vec![
            ContractOperation {
                name: "capabilities".to_string(),
                inputs: Json::obj([]),
                outputs: Json::obj([(
                    "capabilities",
                    Json::str("{options_supported, decision_points, makes_model_calls, estimator_ref, requires{task_value, priors}, deterministic}"),
                )]),
                invariants: vec![
                    "options_supported ⊆ ComputeOption kinds (closed — ADR-0188 D3)".to_string(),
                    "decision_points ⊆ {propose, delegate, verify, retry, stop}".to_string(),
                ],
                failure_modes: vec![],
            },
            ContractOperation {
                name: "bind".to_string(),
                inputs: Json::obj([
                    ("decision", Json::str("control_decision")),
                    ("ctx", Json::str("compute_context")),
                ]),
                outputs: Json::obj([(
                    "outcome",
                    Json::str("BoundDecision{decision′, record} | Unchanged{record}"),
                )]),
                invariants: vec![
                    "B-1 kind/decision_point/owner unchanged (β untouched)".to_string(),
                    "B-2 every bound parameter was unbound or is tightened".to_string(),
                    "B-3 a binding the envelope would refuse returns Unchanged".to_string(),
                    "B-4 every supported option in options_considered[] with an estimate or typed infeasibility".to_string(),
                    "B-5 no Text leaf, no model_claim as sole basis".to_string(),
                    "B-6 pure over (decision, ctx)".to_string(),
                ],
                failure_modes: vec![
                    "PolicyInvalid".to_string(),
                    "EstimatorBudgetExhausted".to_string(),
                    "PriorUnknown".to_string(),
                    "TaskValueMissing".to_string(),
                ],
            },
            ContractOperation {
                name: "advise".to_string(),
                inputs: Json::obj([
                    ("decision_point", Json::str("model-owned point")),
                    ("ctx", Json::str("compute_context")),
                ]),
                outputs: Json::obj([(
                    "advice",
                    Json::str("ContextItem{kind: compute_advice} | none"),
                )]),
                invariants: vec![
                    "never affects bind".to_string(),
                    "rendered through the Model Profile by a HarnessRule".to_string(),
                ],
                failure_modes: vec![],
            },
            ContractOperation {
                name: "observe".to_string(),
                inputs: Json::obj([("events", Json::str("durable ledger events[]"))]),
                outputs: Json::obj([("priors", Json::str("updated online priors"))]),
                invariants: vec![
                    "durable-ledger-only inputs (ADR-0189 D4)".to_string(),
                    "idempotent; resets cells on supersession/drift".to_string(),
                ],
                failure_modes: vec![],
            },
            ContractOperation {
                name: "explain".to_string(),
                inputs: Json::obj([("record_ref", Json::str("ref"))]),
                outputs: Json::obj([(
                    "explanation",
                    Json::str("projection over control.compute.decided + compute_decision_outcome"),
                )]),
                invariants: vec!["a projection, never a stored score".to_string()],
                failure_modes: vec![],
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
                    ("options_supported", Json::Null),
                    ("decision_points", Json::Null),
                    ("makes_model_calls", Json::Null),
                    ("estimator_ref", Json::Null),
                    ("requires_task_value", Json::Null),
                    ("requires_priors", Json::Null),
                    ("deterministic", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            (
                "required",
                Json::Arr(vec![Json::str("deterministic")]),
            ),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![
            "propose".to_string(),
            "delegate".to_string(),
            "verify".to_string(),
            "retry".to_string(),
            "stop".to_string(),
        ],
        metrics_declared: vec![
            "scheduling.overhead".to_string(),
            "scheduling.decision_count".to_string(),
            "scheduling.option_share".to_string(),
            "scheduling.regret_vs_oracle".to_string(),
            "scheduling.prior_coverage".to_string(),
            "scheduling.advice_compliance".to_string(),
        ],
        slot_key: "compute_policy".to_string(),
        tier: "C3".to_string(),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
            hh_plugin::ContractRef::class_contract("compute_estimator", "*"),
        ],
    }
}

/// `compute_estimator` — the marginal-value estimator class (spec §5e.4
/// R-2.6.4; ADR-0189; S4.7). `exactly-one` inside a `compute_policy`
/// variant's `estimator_ref`; typed inputs only, `MarginalValueEstimate |
/// EstimatorUnavailable` out. `rules` is the C3 baseline (no model calls);
/// `bandit`/`surface_prior`/`predictor` land at Stage 5/6.
pub fn compute_estimator_class() -> ClassRecord {
    ClassRecord {
        class_id: "compute_estimator".to_string(),
        contract: vec![ContractOperation {
            name: "estimate".to_string(),
            inputs: Json::obj([
                ("option", Json::str("compute_option")),
                ("ctx", Json::str("compute_context")),
                ("priors", Json::str("prior_cells[]")),
            ]),
            outputs: Json::obj([(
                "estimate",
                Json::str("MarginalValueEstimate | EstimatorUnavailable{reason}"),
            )]),
            invariants: vec![
                "typed inputs only — no Text leaf, no model_claim as sole basis (B-5)".to_string(),
                "pure over (option, ctx, priors) when deterministic".to_string(),
            ],
            failure_modes: vec!["EstimatorUnavailable".to_string()],
        }],
        cardinality: crate::kinds::Cardinality::ExactlyOne,
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
                    ("makes_model_calls", Json::Null),
                    ("window", Json::Null),
                    ("discount", Json::Null),
                    ("min_n", Json::Null),
                    ("deterministic", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec!["scheduling.overhead".to_string()],
        slot_key: "compute_estimator".to_string(),
        tier: "C3".to_string(),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
    }
}

/// `work_source_adapter` — the C4/Stage-5 fleet work-source component
/// class (§5i.1; R-2.12.6; ADR-0205 D4; S5.6). The `WorkSourceAdapter`
/// seam as a registered class: `capabilities` (the tri-state probe),
/// `occurrences(since)`/`suspended`/`activate_run` (the observation +
/// reconcile verbs), and `list`/`get` (the read minimum). Out-of-process
/// only (ADR-0180/0181 — L5): variants declare
/// `placement = subprocess_confined`; the fixture reference variant is
/// `hh/hh-tracker-fixture`.
pub fn work_source_adapter_class() -> ClassRecord {
    ClassRecord {
        class_id: "work_source_adapter".to_string(),
        contract: vec![
            ContractOperation {
                name: "capabilities".to_string(),
                inputs: Json::obj([]),
                outputs: Json::obj([("capabilities", Json::str("tri-state record"))]),
                invariants: vec![
                    "unknown is never coerced — a member that cannot be answered stays unknown"
                        .to_string(),
                ],
                failure_modes: vec!["source_unavailable".to_string()],
            },
            ContractOperation {
                name: "occurrences".to_string(),
                inputs: Json::obj([("since", Json::str("ms | null"))]),
                outputs: Json::obj([("occurrences", Json::str("SourceOccurrence[]"))]),
                invariants: vec![
                    "occurrence ids are the adapter's own — replays yield the same ids".to_string(),
                    "occurrences carry actor provenance".to_string(),
                ],
                failure_modes: vec!["source_unavailable".to_string()],
            },
            ContractOperation {
                name: "suspended".to_string(),
                inputs: Json::obj([("source_id", Json::str("text"))]),
                outputs: Json::obj([("suspended", Json::str("bool"))]),
                invariants: vec!["a faulted source is unavailability, never false".to_string()],
                failure_modes: vec!["source_unavailable".to_string()],
            },
            ContractOperation {
                name: "activate_run".to_string(),
                inputs: Json::obj([("candidates", Json::str("item_id[]"))]),
                outputs: Json::obj([("dispatchable", Json::str("item_id[]"))]),
                invariants: vec![
                    "the answer intersects the source-declared dispatchable set".to_string()
                ],
                failure_modes: vec!["source_unavailable".to_string()],
            },
            ContractOperation {
                name: "list".to_string(),
                inputs: Json::obj([("states", Json::str("string[]"))]),
                outputs: Json::obj([("records", Json::str("WorkSourceRecord[]"))]),
                invariants: vec![
                    "an omitted record means no longer visible — never a synthetic state"
                        .to_string(),
                ],
                failure_modes: vec!["source_unavailable".to_string()],
            },
            ContractOperation {
                name: "get".to_string(),
                inputs: Json::obj([("native_ids", Json::str("string[]"))]),
                outputs: Json::obj([("records", Json::str("WorkSourceRecord[]"))]),
                invariants: vec![
                    "a malformed requested record is a reported fault, never a silent omission"
                        .to_string(),
                ],
                failure_modes: vec!["source_unavailable".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::OrderedMany,
        required_inputs: BTreeSet::new(),
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
                    ("push_delivery_id", Json::Null),
                    ("poll", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec![],
        slot_key: "work_source_adapter".to_string(),
        tier: "C4".to_string(),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
    }
}

/// `evolution_proposer` — the §05h §2.4 proposer class (R-2.9.5; ADR-0196
/// D1/D2; S6.2). `exactly-one` per campaign, homed on plane 7
/// (measurement), never invoked inside a subject run (`hot_path: false`,
/// `decision_points: ∅`). Contract: `propose`, `select_parent`,
/// `declare` — the propose output is the typed sum `[CandidateProposal ∪
/// FailureHypothesis] | NoAddressableFailure`. Third-party variants run
/// out of process under `plugin_abi/1` (`subprocess_confined`); the
/// four-kind suite below is the §06 ADR-0152 driver surface the variant
/// host doubles as (`registry_ci`).
pub fn evolution_proposer_class() -> ClassRecord {
    ClassRecord {
        class_id: "evolution_proposer".to_string(),
        contract: vec![
            ContractOperation {
                name: "propose".to_string(),
                inputs: Json::obj([
                    ("corpus", Json::str("EvidenceCorpusRef")),
                    ("base", Json::str("DefinitionVersionRef")),
                    ("constraints", Json::str("ProposalConstraints")),
                    ("profile", Json::str("ModelProfile")),
                    ("account", Json::str("ResourceAccount")),
                ]),
                outputs: Json::obj([(
                    "outcomes",
                    Json::str("[CandidateProposal ∪ FailureHypothesis] | NoAddressableFailure"),
                )]),
                invariants: vec![
                    "reads only the corpus and base — no held-out surface".to_string(),
                    "writes only candidate records at scope run".to_string(),
                    "every model call lands on the proposer's budget slice (charged_to = instrument)"
                        .to_string(),
                    "NoAddressableFailure is a typed outcome, never an empty list".to_string(),
                ],
                failure_modes: vec![
                    "CorpusUnreadable".to_string(),
                    "ConstraintUnsatisfiable".to_string(),
                    "BudgetExhausted".to_string(),
                ],
            },
            ContractOperation {
                name: "select_parent".to_string(),
                inputs: Json::obj([
                    ("lineage", Json::str("CandidateRecord[]")),
                    ("policy", Json::str("ParentSelectionPolicy")),
                ]),
                outputs: Json::obj([("parent", Json::str("DefinitionVersionRef"))]),
                invariants: vec![
                    "pure — policies are campaign data".to_string(),
                    "policy ∈ {best, score_proportional, score_child_proportional, pareto_per_task}"
                        .to_string(),
                ],
                failure_modes: vec![],
            },
            ContractOperation {
                name: "declare".to_string(),
                inputs: Json::obj([]),
                outputs: Json::obj([("declaration", Json::str("proposer_declaration"))]),
                invariants: vec![
                    "tri-state fields (T-LCD-07 reflexively)".to_string(),
                    "{family, op_classes_admissible, needs_reference_trajectories, uses_judge, \
                     judge_ref?, maturity}"
                        .to_string(),
                ],
                failure_modes: vec!["IncompleteDeclaration".to_string()],
            },
        ],
        cardinality: crate::kinds::Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "P7".to_string(),
        declaration_schema: Json::obj([
            (
                "properties",
                Json::obj([
                    ("family", Json::Null),
                    ("op_classes_admissible", Json::Null),
                    ("needs_reference_trajectories", Json::Null),
                    ("uses_judge", Json::Null),
                    ("judge_ref", Json::Null),
                    ("maturity", Json::Null),
                    ("conditioned_rules", Json::Null),
                ]),
            ),
            ("additionalProperties", Json::Bool(false)),
            (
                "required",
                Json::Arr(vec![
                    Json::str("family"),
                    Json::str("op_classes_admissible"),
                    Json::str("uses_judge"),
                    Json::str("maturity"),
                ]),
            ),
        ]),
        depends_on: vec![
            hh_plugin::ContractRef::dialect("hir/1", "*"),
            hh_plugin::ContractRef::dialect("registry/1", "*"),
        ],
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec!["cost_of_proposal".to_string()],
        slot_key: "evolution_proposer".to_string(),
        tier: "C4".to_string(),
    }
}

/// The `evolution_proposer` conformance suite — the §05h §2.4 four kinds
/// (ADR-0196 D2): *static* — declaration conforms and every conditioned
/// proposer rule carries an `AssumptionDebtRecord`; *contract* — driven
/// out of process over a golden corpus, every output parses as a
/// `CandidateProposal`/`NoAddressableFailure` and `classify` of every
/// emitted diff is never widening/loosening; *property* — the isolation
/// probes (no read outside the corpus, no held-out path, no network
/// beyond the gateway), never emits a candidate on the exclusion set,
/// `uses_judge = false` variants make no judge call (DRIFT);
/// *executable* — the golden corpus yields ≥ 1 S2-valid hypothesis or a
/// `NoAddressableFailure` with a reason. The driver is
/// `hh_evolution::proposer_conformance` over a `ProposerPort` — the
/// variant host supplies the out-of-process lane (§06 ADR-0152's
/// `registry_ci`).
pub fn evolution_proposer_suite(class_version_id: &str) -> ConformanceSuite {
    let test = |test_id: &str, kind: TestKind, verdict_rule: &str| SuiteTest {
        test_id: test_id.to_string(),
        kind,
        fixture_ref: None,
        driver: TestDriver::OutOfProcess,
        oracle_class: OracleClass::Deterministic,
        budget: Json::obj([("max_ms", Json::Int(5_000))]),
        verdict_rule: verdict_rule.to_string(),
    };
    ConformanceSuite {
        suite_id: "evolution_proposer.c4".to_string(),
        class_ref: class_version_id.to_string(),
        contract_version: "1.0".to_string(),
        tests: vec![
            test(
                "static.declaration",
                TestKind::Static,
                "declare() fields present; conditioned rules carry complete debt records",
            ),
            test(
                "contract.propose",
                TestKind::Contract,
                "every output parses as CandidateProposal|FailureHypothesis|NoAddressableFailure; \
                 no emitted diff classifies widening/loosening",
            ),
            test(
                "property.isolation",
                TestKind::Property,
                "no read outside corpus; no held-out path; no network beyond the gateway; never \
                 emits an excluded target; uses_judge=false ⇒ zero judge calls else DRIFT",
            ),
            test(
                "executable.golden_corpus",
                TestKind::Executable,
                "golden corpus yields ≥ 1 S2-valid hypothesis or NoAddressableFailure{reason}",
            ),
        ],
        required_for_status: BTreeSet::from([
            "static.declaration".to_string(),
            "contract.propose".to_string(),
            "property.isolation".to_string(),
            "executable.golden_corpus".to_string(),
        ]),
    }
}
