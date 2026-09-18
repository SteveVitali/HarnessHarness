//! The `registry/1` record bodies (spec §6.2 §3; ADR-0151 D3/D5/D8; ADR-0152 D1/D3;
//! ADR-0153 D2): `ClassRecord`/`VariantRecord` keep their ADR-0023 shapes and gain
//! the registry fields additively; the `RegistryEnvelope` wraps every record; the
//! `NamespaceRecord`/`RegistryPolicy` are MUST-data; `RegistryDiagnostic` makes
//! failures records (R10); `RegistrySnapshot` is the ADR-0038 closure form.
//!
//! Identity discipline (N5/N6, CC3): every reference a record embeds is a **pinned**
//! `version_id`/`ContentAddress` (`<algorithm>:<hex>`) — a selector or display name
//! inside a record is refused at `register`/`from_json` (R7). `version_id` covers
//! the *record body*; the envelope is registration metadata around it and never
//! feeds identity.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_identity::idp::ContentAddress;
use hh_identity::repro::InstrumentRecord;
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::eval::OracleDeclaration;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::kinds::{
    Admission, Cardinality, ConformanceVerdict, OracleClass, OwnerRef, Placement, ProducedBy,
    PublishRule, RecordKind, RequireConformance, SubjectKind, TestDriver, TestKind,
};

/// The one mandatory-writable registry dialect.
pub const REGISTRY_DIALECT: &str = "registry/1";

/// A contract operation of a `ClassRecord.contract` (ADR-0023 —
/// `operations[{name, inputs, outputs, invariants, failure_modes}]`; contracts are
/// operation records, never inheritance — T-LCD-12).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractOperation {
    /// The operation name.
    pub name: String,
    /// Input shape (canonical schema payload).
    pub inputs: Json,
    /// Output shape (canonical schema payload).
    pub outputs: Json,
    /// Invariants the operation must hold.
    pub invariants: Vec<String>,
    /// The closed failure-mode spellings.
    pub failure_modes: Vec<String>,
}

/// A declared variant parameter (`param_schema: map<name, ParamDecl{type, domain,
/// default?, unit?, sweepable, budget_relevant, affects[]}>` — ADR-0151 D5). A
/// `sweepable` parameter's `domain` is the sweep engine's enumeration space;
/// `budget_relevant` flags the parameters a `MatchSpec` may not omit when they
/// differ across arms (T-LCD-09/-14).
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    /// The value type (`int`, `decimal`, `bool`, `string`, `enum`).
    pub value_type: String,
    /// The sweepable domain (a canonical set/range payload — e.g. `[1,2,4]` or
    /// `{"min":1,"max":8}`); mandatory when `sweepable`.
    pub domain: Option<Json>,
    /// The default value, when declared.
    pub default: Option<Json>,
    /// The unit, when declared.
    pub unit: Option<String>,
    /// Whether the sweep engine may enumerate this parameter.
    pub sweepable: bool,
    /// Whether the parameter moves budget consumption (a `MatchSpec` may not omit
    /// it when it differs across arms).
    pub budget_relevant: bool,
    /// Declaration fields this parameter affects (causal annotation).
    pub affects: Vec<String>,
}

/// A variant's `implementation` (ADR-0151 D5 as amended — `placement` is primary;
/// `locality` derived). Pinned by `ContentAddress` — a foreign locator without a
/// digest is a claim, not a pin, and is refused `UnpinnedImplementation` at
/// `register` (R7; AC-3 — the registry never loads it).
#[derive(Debug, Clone, PartialEq)]
pub struct Implementation {
    /// The pinned implementation body (never dereferenced by any registry op — R3).
    pub content: ContentAddress,
    /// The declared placement.
    pub placement: Placement,
    /// Host requirements the variant host checks at `hello` (opaque payload).
    pub host_requirements: Json,
}

/// `applies_to` — registry-level scoping, not an HIR amendment (OQ-344/CF-326;
/// ADR-0151 D5). `participant_classes` is `{native}` for native variants; hosted
/// participants are records of kind `participant`, never variants (T-LCD-06).
#[derive(Debug, Clone, PartialEq)]
pub struct AppliesTo {
    /// The participant classes this variant applies to (`native` for Stage-1
    /// variants; `hosted` is a claim on `participant` records, not variants).
    pub participant_classes: BTreeSet<String>,
    /// Environment families the variant applies to, when family-scoped (the
    /// ADR-0143 admissible family-conditioning form).
    pub families: Vec<String>,
}

/// The `ClassRecord` (ADR-0023 shape + ADR-0151 D5 `{contract_version, home,
/// declaration_schema, conformance_suite_ref, decision_points, metrics_declared,
/// slot_key, tier}`). A contract change is a new `ClassRecord` *version* (a new
/// registered record of the same `class_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassRecord {
    /// The component class id (e.g. `control_strategy`).
    pub class_id: String,
    /// The class contract — operation records, never inheritance.
    pub contract: Vec<ContractOperation>,
    /// The slot cardinality.
    pub cardinality: Cardinality,
    /// `required_inputs ⊇ {ModelProfile, ResourceAccount}` (T-LCD-08).
    pub required_inputs: BTreeSet<String>,
    /// The base parameter schema every variant's `param_schema` extends.
    pub base_param_schema: BTreeMap<String, ParamDecl>,
    /// Whether the class sits on the per-step control path (the OQ-075 ceiling's
    /// locality subject).
    pub hot_path: bool,
    /// The dialect the class was introduced in.
    pub dialect_introduced: String,
    /// The contract version this `ClassRecord` version declares (`contract_range ∩
    /// contract_version ≠ ∅` is the variant-admission check).
    pub contract_version: String,
    /// The class's home plane (CC10).
    pub home: String,
    /// The schema every variant `capability_declaration` is checked against
    /// (declared field vocabulary — carried as a canonical payload).
    pub declaration_schema: Json,
    /// The pinned `version_id` of the class's `ConformanceSuite`, when one exists
    /// (a class without a suite is `SuiteMissing`; its variants resolve only under
    /// `require_conformance = none`).
    pub conformance_suite_ref: Option<String>,
    /// The β decision points the class's variants may occupy.
    pub decision_points: Vec<String>,
    /// The metrics the class declares.
    pub metrics_declared: Vec<String>,
    /// The assembly slot key this class binds (`slots` map key).
    pub slot_key: String,
    /// The class tier (ADR-0182 X1 — e.g. `C0`).
    pub tier: String,
}

/// The `VariantRecord` (ADR-0023 shape + ADR-0151 D5 as amended / ADR-0153). The
/// *semantic core* (identity-bearing — N6) is `class_ref`, `contract_range`,
/// `param_schema`, `implementation`, `capability_declaration`, `conditioned_rules`
/// and `applies_to`; the *surface* (rename-stable `semantic_id`, version-only —
/// AC-7) is `variant_id`, `version_label`, `declared_costs` and `summary`.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantRecord {
    /// The open, registry-validated variant tag (an unregistered `variant_id` is
    /// `UnresolvedRef` at the assembly seam, never silently accepted).
    pub variant_id: String,
    /// The pinned `version_id` of the `ClassRecord` this variant implements.
    pub class_ref: String,
    /// The contract-version range this variant implements (`contract_range ∩
    /// class.contract_version ≠ ∅` required at `register`).
    pub contract_range: String,
    /// A SemVer-class label — a claim, never identity (N4).
    pub version_label: Option<String>,
    /// The declared parameter schema.
    pub param_schema: BTreeMap<String, ParamDecl>,
    /// The pinned implementation.
    pub implementation: Implementation,
    /// The capability declaration — checked against the class's
    /// `declaration_schema` at `register`; third-party declarations are claims
    /// entering no vector until a `registry_ci`/`lab` report exists (ADR-0063 L3).
    pub capability_declaration: BTreeMap<String, Json>,
    /// Every conditioned rule carries a **complete** `AssumptionDebtRecord`
    /// (`{rule_id, debt}` — T-LCD-05; `ConditionedRuleIncomplete` at `register`).
    pub conditioned_rules: Vec<(String, AssumptionDebtRecord)>,
    /// Registry-level applicability scoping.
    pub applies_to: AppliesTo,
    /// Advisory declared costs (ADR-0089 — never a charge authority).
    pub declared_costs: Option<Json>,
    /// The human-facing summary — a `Text` leaf whose `authority` is minted, never
    /// declared (`external` for third-party packages; R-TEXT).
    pub summary: Text,
    /// The dialect range the record supports (`DialectIncompatible` when it
    /// excludes `registry/1`).
    pub dialect_range: String,
}

/// A `ConformanceSuite` test (ADR-0152 D1).
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteTest {
    /// The test id (unique within the suite).
    pub test_id: String,
    /// The test kind.
    pub kind: TestKind,
    /// The pinned fixture, when the test has one.
    pub fixture_ref: Option<ContentAddress>,
    /// The declared driver.
    pub driver: TestDriver,
    /// The oracle class (deterministic-only at C0).
    pub oracle_class: OracleClass,
    /// The declared budget for the test (a §8.2-owned payload carried opaquely —
    /// the registry stores it as data, never interprets it).
    pub budget: Json,
    /// The verdict rule (how `results[]` map to the test verdict).
    pub verdict_rule: String,
}

/// The `ConformanceSuite` record — pinned to the class's `contract_version`
/// (ADR-0152 D1). Every `ClassRecord` carries `conformance_suite_ref`; a class
/// without a suite is `SuiteMissing`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceSuite {
    /// The suite id.
    pub suite_id: String,
    /// The pinned `version_id` of the `ClassRecord` this suite tests.
    pub class_ref: String,
    /// The `contract_version` the suite is pinned to (a class `contract_version`
    /// bump makes reports against this suite `stale`).
    pub contract_version: String,
    /// The declared tests.
    pub tests: Vec<SuiteTest>,
    /// The test ids required for a satisfying status.
    pub required_for_status: BTreeSet<String>,
}

/// One `conformance_report.results[]` entry (`{test_id | dimension, verdict,
/// evidence_ref}` — ADR-0152 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct ReportResult {
    /// The suite test id or declaration dimension this result is for.
    pub subject: String,
    /// The observed verdict (the one shared vocabulary).
    pub verdict: ConformanceVerdict,
    /// The pinned evidence (ledger event / blob address).
    pub evidence_ref: Option<String>,
}

/// The host a report ran on (`{placement, isolation, instrument}` — ADR-0152 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct ReportHost {
    /// The placement the subject ran at.
    pub placement: Placement,
    /// The isolation class spelling.
    pub isolation: String,
    /// The instrument's identity (`InstrumentRecord` — R-2.12.1).
    pub instrument: InstrumentRecord,
}

/// The `conformance_report` record (one kind; ADR-0152 D3–D4 as amended).
/// **Immutable** — reports never overwrite. `registry_ci`/`lab` reports must
/// reference a ledgered run with `charged_to = instrument` (the `record_conformance`
/// op that checks that lands at Stage 2; the record shape lands here).
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceReport {
    /// The report id.
    pub report_id: String,
    /// The subject kind.
    pub subject_kind: SubjectKind,
    /// The pinned subject (`version_id`).
    pub subject_ref: String,
    /// The pinned suite (`version_id`).
    pub suite_ref: String,
    /// The host the report ran on.
    pub host: ReportHost,
    /// The producer class (`publisher_claim` never counts toward `probed`).
    pub produced_by: ProducedBy,
    /// The per-test/dimension results.
    pub results: Vec<ReportResult>,
    /// The probed declaration — per declaration field, the observed verdict.
    pub probed_declaration: BTreeMap<String, ConformanceVerdict>,
    /// The ledgered run the report's evidence lives in.
    pub run_id: String,
    /// `true` when the report's suite is not the head suite of the class's current
    /// `contract_version` — stale reports satisfy no floor.
    pub stale: bool,
}

/// The `NamespaceRecord` (ADR-0153 D2) — layered per ADR-0024 (`authority_cap`
/// narrows only). Reserved namespaces: `hh/` (kernel-owned; `ClassRecord`s live
/// here; only the instrument's release process publishes) and `local/` (the run's
/// principal; never exported without re-namespacing). `exp/` and shared
/// reverse-DNS namespaces are Stage 2+/C1 — not this slice.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceRecord {
    /// The namespace spelling (`hh`, `local`).
    pub namespace: String,
    /// The owners.
    pub owners: Vec<OwnerRef>,
    /// Who may publish into the namespace.
    pub who_may_publish: PublishRule,
    /// Who may deprecate entries.
    pub who_may_deprecate: PublishRule,
    /// Who may yank entries.
    pub who_may_yank: PublishRule,
    /// Who may revoke versions.
    pub who_may_revoke: PublishRule,
    /// Whether publishes require a signature (the C1 signer machinery's read —
    /// `false` at Stage 1).
    pub require_signature: bool,
}

/// The `RegistryPolicy` (MUST-data, layered per ADR-0024 — ADR-0151 D8 /
/// ADR-0153 D2). The store, envelope, operations, conformance drivers and
/// namespace enforcement are MUST-code; the policy is data.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryPolicy {
    /// The conformance floor (`declared` at Stage 1).
    pub require_conformance: RequireConformance,
    /// The report producers a `probed` verdict may come from
    /// (`publisher_claim` is never admissible).
    pub admissible_report_producers: BTreeSet<ProducedBy>,
    /// Origin tags for which a `trust_record_ref` is mandatory at `register`
    /// (default: every origin except kernel/definition authority).
    pub require_trust_record_for_origins: BTreeSet<String>,
    /// The kinds requiring a signature (a signature-required kind registers
    /// `quarantined` until a `pin` endorsement — ADR-0063).
    pub require_signature_for_kinds: BTreeSet<RecordKind>,
    /// The freshness bound for name bindings (`NameHistoryEntry.published_at` +
    /// `max_age` — the TUF timestamp role; stale pins refused at `resolve`).
    pub max_age_ms: Option<u64>,
    /// The collision policy (`refuse` — the only value at this stage).
    pub collision_policy: String,
    /// The default admission for `foreign_import` records (`quarantined`).
    pub foreign_import_default_admission: Admission,
    /// The placements admissible per registrar authority (`{kernel, definition}` →
    /// all placements; every other registrar → `{subprocess_confined, container,
    /// remote}` — `in_process` third-party code is `LocalityInadmissible`,
    /// ADR-0153 D1/CF-141).
    pub allowed_localities_by_origin: BTreeMap<String, BTreeSet<Placement>>,
    /// The foreign systems `import` may read (Stage 4 — held as data here).
    pub allowed_foreign_systems: BTreeSet<String>,
}

impl RegistryPolicy {
    /// The Stage-1 default policy (`require_conformance = declared`, reports from
    /// `{registry_ci, lab}` admissible, collisions refused, foreign imports
    /// quarantined, third-party origins confined).
    pub fn stage1_default() -> RegistryPolicy {
        RegistryPolicy {
            require_conformance: RequireConformance::Declared,
            admissible_report_producers: BTreeSet::from([ProducedBy::RegistryCi, ProducedBy::Lab]),
            require_trust_record_for_origins: BTreeSet::new(),
            require_signature_for_kinds: BTreeSet::new(),
            max_age_ms: None,
            collision_policy: "refuse".to_string(),
            foreign_import_default_admission: Admission::Quarantined,
            allowed_localities_by_origin: BTreeMap::new(),
            allowed_foreign_systems: BTreeSet::new(),
        }
    }
}

/// A `foreign_import` record (`{ForeignRef{system, digest?, label}, lifted record,
/// LossReport, admission}` — ADR-0153 D4). Imported text legs are `unverified`
/// (P7); quarantined by default. The `import`/`export` operations are Stage 4 —
/// this is the record shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignImport {
    /// The foreign system (`mcp_registry`, `acp_registry`, `marketplace`, `git`,
    /// `archive`).
    pub system: String,
    /// The foreign locator (a claim, never identity).
    pub locator: String,
    /// The foreign digest when the source carries one (a claim — N8).
    pub digest: Option<String>,
    /// The foreign label.
    pub label: Option<String>,
    /// The lifted record (canonical payload — a `participant`-descriptor candidate
    /// for ACP `agent.json`, never a `VariantRecord`).
    pub lifted_record: Json,
    /// The loss report — what the lift could not carry (`{capabilities,
    /// conformance, trust legs, digest}` at minimum).
    pub loss_report: Vec<String>,
}

/// A `registry_snapshot` record (ADR-0038 — a closure, not a catalog). The
/// `registry_snapshot_id` is the snapshot role of the TUF-shaped freshness model
/// (ADR-0153 D3); `resolve`/`query`/`slot_choices` over it are deterministic (R8).
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrySnapshot {
    /// The content id (`registry_snapshot_id`).
    pub snapshot_id: String,
    /// The pinned member `version_id`s (the referenced closure).
    pub members: BTreeSet<String>,
    /// The `(namespace, name) → version_id` bindings at snapshot time.
    pub name_bindings: BTreeMap<(String, String), String>,
    /// The `RegistryPolicy` digest the snapshot pins.
    pub policy_digest: String,
    /// The transaction-time seq the snapshot was cut at.
    pub created_seq: u64,
}

/// A `RegistryDiagnostic` (`{operation, reason, subject, registrar}` — ADR-0151
/// R10). **Failures are records**: every refused operation appends one here (and a
/// refused-admission event), never an instrument start-up failure.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryDiagnostic {
    /// The operation that failed.
    pub operation: String,
    /// The typed failure reason (the `RegistryError` spelling).
    pub reason: String,
    /// The subject (`version_id`, name or kind) when there is one.
    pub subject: Option<String>,
    /// The registrar whose operation failed.
    pub registrar: ProvenanceRecord,
    /// The transaction-time seq.
    pub seq: u64,
}

/// The `RegistryEnvelope` (ADR-0151 D3 — `{kind, version_id, semantic_id?,
/// registered_at, registrar, admission, name_history_ref?,
/// trust_record_ref?, dialect_range, registry_dialect, ext}`). Wraps every record;
/// `ext` never decides admission, conformance or authority. `registered_at` is a
/// transaction-time seq, never a wall clock.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEnvelope {
    /// The record kind.
    pub kind: RecordKind,
    /// The record's `version_id`.
    pub version_id: String,
    /// The `semantic_id`, for kinds with a declared semantic projection.
    pub semantic_id: Option<String>,
    /// The transaction-time seq the record registered at.
    pub registered_at: u64,
    /// The registrar's canonical provenance record (mandatory — §8.1).
    pub registrar: ProvenanceRecord,
    /// The *base* admission (the `revoked` value is derived from the lineage —
    /// never authored).
    pub admission: Admission,
    /// The latest name-history entry naming this version, when published.
    pub name_history_ref: Option<String>,
    /// The pinned trust record — mandatory when `registrar.authority ∉ {kernel,
    /// definition}` (ADR-0153; `TrustRecordRequired` at `register`).
    pub trust_record_ref: Option<String>,
    /// The dialect range the record supports.
    pub dialect_range: String,
    /// Registered `ext` members — preserved, never deciding (CC3).
    pub ext: BTreeMap<String, Json>,
}

/// The `capability` record body (§5d.1; ADR-0087/0088/0089; S1.17): a
/// `ToolCapability` HIR/1 node registered as the body — semantic, surface and
/// provenance included; the `RegistryEnvelope` stays registration metadata and
/// never feeds identity. Both identity coordinates are **the node's own**
/// (V-E1-9 — `version_id = H(canonical node)`, `semantic_id` over the semantic
/// projection with `exposure_hint`/`cost_model.measured_ref` excluded): the same
/// capability carries one `version_id` wherever it is pinned (CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityRecord {
    /// The `ToolCapability` node (`EntityKind::ToolCapability` — enforced at
    /// `record_from_json` and `register`).
    pub node: hh_hir::document::Node,
}

/// A typed registry record body — the closed Stage-1 schema set (kinds without a
/// landed schema are refused `SchemaViolation{path: "kind"}`, never silently
/// admitted — R2/CC3). `Variant` is boxed: it dwarfs the other variants and the
/// box keeps the enum small (a representation choice, not a schema one).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryRecord {
    /// A `ClassRecord`.
    Class(ClassRecord),
    /// A `VariantRecord`.
    Variant(VariantRecord),
    /// A `ConformanceSuite`.
    Suite(ConformanceSuite),
    /// A `conformance_report`.
    Report(ConformanceReport),
    /// A `NamespaceRecord`.
    Namespace(NamespaceRecord),
    /// A `registry_snapshot` (written by `snapshot()`, not `register`).
    Snapshot(RegistrySnapshot),
    /// A `foreign_import`.
    ForeignImport(ForeignImport),
    /// A `capability` — a registered `ToolCapability` HIR/1 node.
    Capability(CapabilityRecord),
    /// A `metric_declaration` — the full §5h.2 `MetricDeclaration` is the body
    /// (R-2.9.2; the ontology owns the schema — CC7).
    MetricDeclaration(MetricDeclaration),
    /// A `validator` — the §5h.2 `OracleDeclaration` is the body (R-2.9.2;
    /// ADR-0047). A Validator *component* registers as a `variant` of the
    /// `validator` class; this kind is the oracle's declaration record.
    Validator(OracleDeclaration),
}

impl RegistryRecord {
    /// The record's kind.
    pub fn kind(&self) -> RecordKind {
        match self {
            RegistryRecord::Class(_) => RecordKind::Class,
            RegistryRecord::Variant(_) => RecordKind::Variant,
            RegistryRecord::Suite(_) => RecordKind::ConformanceSuite,
            RegistryRecord::Report(_) => RecordKind::ConformanceReport,
            RegistryRecord::Namespace(_) => RecordKind::Namespace,
            RegistryRecord::Snapshot(_) => RecordKind::RegistrySnapshot,
            RegistryRecord::ForeignImport(_) => RecordKind::ForeignImport,
            RegistryRecord::Capability(_) => RecordKind::Capability,
            RegistryRecord::MetricDeclaration(_) => RecordKind::MetricDeclaration,
            RegistryRecord::Validator(_) => RecordKind::Validator,
        }
    }

    /// The pinned `version_id`s this record references (the `snapshot` closure's
    /// edge set — R7: sealed forms reference records only by `version_id`).
    pub fn referenced_version_ids(&self) -> Vec<String> {
        match self {
            RegistryRecord::Class(c) => c.conformance_suite_ref.iter().cloned().collect(),
            RegistryRecord::Variant(v) => vec![v.class_ref.clone()],
            RegistryRecord::Suite(s) => vec![s.class_ref.clone()],
            RegistryRecord::Report(r) => {
                vec![r.subject_ref.clone(), r.suite_ref.clone()]
            }
            RegistryRecord::Namespace(_) | RegistryRecord::ForeignImport(_) => Vec::new(),
            // `applies_to_families` scopes by family *name* (the same treatment
            // `VariantRecord.applies_to.families` gets) — names are never pins.
            RegistryRecord::MetricDeclaration(_) => Vec::new(),
            // `calibration_ref` is a pinned `version_id` of the deterministic
            // oracle the judge calibrates against (ADR-0047(c)(ii)).
            RegistryRecord::Validator(o) => o.calibration_ref.iter().cloned().collect(),
            RegistryRecord::Snapshot(s) => s.members.iter().cloned().collect(),
            // A capability's pinned postcondition refs close over the Validators/
            // Observations it names (the snapshot closure — R7; unpinned selectors
            // cannot appear inside a sealed/registered body).
            RegistryRecord::Capability(c) => {
                if let hh_hir::records::KindRecord::ToolCapability(t) = &c.node.semantic {
                    t.postconditions
                        .iter()
                        .filter_map(|r| match &r.version {
                            hh_hir::refs::RefVersion::Pinned(v) => Some(v.clone()),
                            hh_hir::refs::RefVersion::Selector(_) => None,
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// A `ComponentVariantRef` — the resolve/slot-choices handle (a pinned
/// `version_id` + the registry coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentVariantRef {
    /// The variant tag.
    pub variant_id: String,
    /// The class id.
    pub class_id: String,
    /// The pinned `version_id`.
    pub version_id: String,
    /// The `semantic_id`, when the kind carries one.
    pub semantic_id: Option<String>,
    /// The sweepable parameter domains with `budget_relevant` flags (the
    /// `slot_choices` projection — T-LCD-09/-14).
    pub sweepable_params: BTreeMap<String, ParamDecl>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage1_policy_defaults() {
        let p = RegistryPolicy::stage1_default();
        assert_eq!(p.require_conformance, RequireConformance::Declared);
        assert!(!p
            .admissible_report_producers
            .contains(&ProducedBy::PublisherClaim));
        assert_eq!(p.collision_policy, "refuse");
        assert_eq!(p.foreign_import_default_admission, Admission::Quarantined);
    }
}
