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
    /// The class's `depends_on` contract refs (ADR-0182 D1 — "`depends_on:
    /// [ContractRef]` on … every `ClassRecord`"; X1 makes `ContractRef` the
    /// only reach — the spec-DAG check and `admit_plugin`'s tier rule consume
    /// it). Empty = substrate-only (the class contract depends on the kernel
    /// substrate, which is C0 by definition).
    pub depends_on: Vec<hh_plugin::ContractRef>,
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

/// Where a hosted conformance entry was observed (§6.6 `observed_in`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedIn {
    /// The adapter's probe surface (a probe run, not a run ledger).
    Probe,
    /// A ledgered hosted run.
    Run,
}

impl ObservedIn {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservedIn::Probe => "probe",
            ObservedIn::Run => "run",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ObservedIn> {
        Some(match s {
            "probe" => ObservedIn::Probe,
            "run" => ObservedIn::Run,
            _ => return None,
        })
    }
}

/// A hosted `ConformanceRecord` — one dimension of a hosted participant's
/// capability declaration compared against what the Lab observed (§6.6;
/// R-2.10.6⁰). Lives on [`ConformanceReport::hosted_entries`] — legal only on a
/// `subject_kind = participant` report.
///
/// `verdict = drift` iff `declared` and `observed` are both *concrete* and
/// differ ([`ConformanceRecord::derive_verdict`] computes it); `unknown`/
/// `skipped` values are never coerced into a verdict (T-LCD-07).
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceRecord {
    /// The participant's `version_identity` (string coordinate — the same
    /// spelling the participant record carries).
    pub participant_version_identity: String,
    /// The adapter's `version_id` pin.
    pub adapter_version_id: String,
    /// The capability dimension this entry covers.
    pub dimension: String,
    /// The declared capability state (a `CapabilityVerdict` spelling or a
    /// structured value — `unknown`/`skipped`/`null` are non-concrete).
    pub declared: Json,
    /// The observed capability state (same value space).
    pub observed: Json,
    /// The verdict — `derive_verdict(declared, observed)` at construction.
    pub verdict: ConformanceVerdict,
    /// Where the observation came from.
    pub observed_in: ObservedIn,
    /// The pinned evidence (ledger event / blob address) when there is one.
    pub evidence_ref: Option<String>,
    /// The observation seq (transaction-time, never a wall clock).
    pub at: u64,
}

impl ConformanceRecord {
    /// `true` for a *concrete* capability-state value — anything but `null`,
    /// `"unknown"` or `"skipped"`.
    fn concrete(j: &Json) -> bool {
        !matches!(j, Json::Null)
            && j.as_str()
                .map(|s| s != "unknown" && s != "skipped")
                .unwrap_or(true)
    }

    /// Verdict-spelling tolerance: the canonical `ConformanceVerdict` spellings
    /// (UPPERCASE) **or** the ontology `CapabilityVerdict` spellings
    /// (lowercase — §6.6's hosted vocabulary). Either maps onto the one
    /// verdict enum (CC1 — `skipped` lands as `Skipped`, never `Unsupported`,
    /// AC-R-2.10.6-3).
    pub(crate) fn verdict_parse(s: &str) -> Option<ConformanceVerdict> {
        ConformanceVerdict::parse(s).or_else(|| {
            Some(match s {
                "supported" => ConformanceVerdict::Supported,
                "unsupported" => ConformanceVerdict::Unsupported,
                "partial" => ConformanceVerdict::Partial,
                "not_applicable" => ConformanceVerdict::NotApplicable,
                "unknown" => ConformanceVerdict::Unknown,
                "skipped" => ConformanceVerdict::Skipped,
                "drift" => ConformanceVerdict::Drift,
                _ => return None,
            })
        })
    }

    /// The §6.6 verdict rule: `drift` iff `declared` and `observed` are both
    /// concrete and differ; `supported` when they agree; otherwise the observed
    /// state projects (`unknown`/`skipped` → that verdict; a non-concrete
    /// declared with a concrete observed is the observed verdict).
    pub fn derive_verdict(declared: &Json, observed: &Json) -> ConformanceVerdict {
        let observed_verdict = match observed.as_str().and_then(Self::verdict_parse) {
            Some(v) => v,
            None if Self::concrete(observed) => ConformanceVerdict::Supported,
            None => ConformanceVerdict::Unknown,
        };
        if Self::concrete(declared) && Self::concrete(observed) {
            if declared == observed {
                ConformanceVerdict::Supported
            } else {
                ConformanceVerdict::Drift
            }
        } else {
            observed_verdict
        }
    }

    /// Build an entry with the verdict derived (the only honest constructor —
    /// an authored `verdict` that disagrees with `derive_verdict` is a lie).
    #[allow(clippy::too_many_arguments)] // the arity is the §6.6 record's.
    pub fn observed_entry(
        participant_version_identity: &str,
        adapter_version_id: &str,
        dimension: &str,
        declared: Json,
        observed: Json,
        observed_in: ObservedIn,
        evidence_ref: Option<String>,
        at: u64,
    ) -> ConformanceRecord {
        ConformanceRecord {
            participant_version_identity: participant_version_identity.to_string(),
            adapter_version_id: adapter_version_id.to_string(),
            dimension: dimension.to_string(),
            verdict: Self::derive_verdict(&declared, &observed),
            declared,
            observed,
            observed_in,
            evidence_ref,
            at,
        }
    }

    /// Stale check — `true` when `current` names a different
    /// `participant_version_identity` (§6.6: records pin the version they
    /// observed; a version change re-derives conformance, never inherits it).
    pub fn is_stale(&self, current_version_identity: &str) -> bool {
        self.participant_version_identity != current_version_identity
    }
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
    /// The hosted conformance entries (§6.6 `ConformanceRecord`s — R-2.10.6⁰).
    /// Legal only on a `subject_kind = participant` report; `report_from_json`
    /// refuses them anywhere else.
    pub hosted_entries: Vec<ConformanceRecord>,
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
    /// The operator/layered `ContractVersionPolicy` records (§8.4 §3;
    /// ADR-0180 D3) — checked by `check_compatibility` inside `resolve`/
    /// `admit_plugin` alongside the kernel table (`hh-plugin`'s
    /// `kernel_contract_policies`). Carried on the policy because a dedicated
    /// `RecordKind` would be a `registry/2` dialect bump (ADR-0262).
    pub contract_version_policies: Vec<hh_plugin::ContractVersionPolicy>,
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
            contract_version_policies: Vec::new(),
        }
    }
}

/// A `foreign_import` record (`{ForeignRef{system, digest?, label}, lifted record,
/// LossReport, admission}` — ADR-0153 D4; S4.1). Imported text legs are
/// `unverified` (P7); quarantined by default.
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
    /// conformance, trust_legs, digest}` partitions + `other`).
    pub loss_report: LossReport,
}

/// The `LossReport` — the partitioned answer to "what the foreign lift could not
/// carry" (§6.2 R7's `structured_loss` for `import`/`export`; ADR-0153 D4).
/// Every member is a list of *named* losses (a path or a member name plus, where
/// the shape admits it, the reason) — nothing is silently dropped (CC3). The
/// closed partitions:
///
/// - `capabilities` — declared capabilities/effects the lift could not map onto
///   a registry declaration member;
/// - `conformance` — conformance/verification claims the foreign shape carries
///   that cannot become admissible `registry_ci`/`lab` evidence (they surface at
///   `review`, never `probed`);
/// - `trust_legs` — signature/attestation/provenance legs the foreign document
///   asserted that no `TrustRootPolicy` anchor verifies (authority defaults to
///   `unverified`);
/// - `digest` — digest/hash legs absent or under a foreign algorithm the lift
///   cannot re-pin under `idp/1`;
/// - `other` — any further loss that fits no partition (the bucket is listed,
///   never silently merged).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LossReport {
    /// Capability-mapping losses.
    pub capabilities: Vec<String>,
    /// Conformance-evidence losses.
    pub conformance: Vec<String>,
    /// Trust-leg losses (unverifiable signatures/claims).
    pub trust_legs: Vec<String>,
    /// Digest-leg losses (absent/unverifiable foreign digests).
    pub digest: Vec<String>,
    /// Losses fitting no named partition.
    pub other: Vec<String>,
}

impl LossReport {
    /// Whether the lift lost nothing at all.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
            && self.conformance.is_empty()
            && self.trust_legs.is_empty()
            && self.digest.is_empty()
            && self.other.is_empty()
    }
}

/// The `model_install` member's closed spelling (§6.2 TrustRootPolicy —
/// `deny | ask | allow_attenuated`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelInstallRule {
    /// Model packages never install from a registry hit.
    Deny,
    /// Install requires an interactive principal confirmation.
    Ask,
    /// Install proceeds under attenuated requests only.
    AllowAttenuated,
}

impl ModelInstallRule {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelInstallRule::Deny => "deny",
            ModelInstallRule::Ask => "ask",
            ModelInstallRule::AllowAttenuated => "allow_attenuated",
        }
    }

    /// The closed-sum parse.
    pub fn parse(s: &str) -> Option<ModelInstallRule> {
        match s {
            "deny" => Some(ModelInstallRule::Deny),
            "ask" => Some(ModelInstallRule::Ask),
            "allow_attenuated" => Some(ModelInstallRule::AllowAttenuated),
            _ => None,
        }
    }
}

/// The `TrustRootPolicy` record (§6.2; ADR-0153 D3/D4; S4.1) — the registry's
/// trust anchor set: which signer coordinates the store accepts
/// (`accepted_signers` names `AttestationAnchor::Signer` values — identity
/// coordinates, never record refs), which record/extension spellings register
/// `quarantined` until a verified signature/pin lifts them
/// (`require_signature_for`), the authority ceiling a hash-only pin may confer
/// (`hash_only_ceiling`, default `external` — integrity, never endorsement),
/// and the drift/scanner/model-install/archive postures carried as data (CC1 —
/// one trust scheme shared with `hh-provenance`'s attestation machinery;
/// registration of this kind requires `principal`+ authority — see `store.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrustRootPolicy {
    /// The attestation kinds this root admits (`AttestationKind` spellings —
    /// `signature`, `pin`, `seal`, …).
    pub allowed_sources: BTreeSet<String>,
    /// The accepted signer coordinates (the `AttestationAnchor::Signer` set —
    /// signer identity refs, never registry version ids).
    pub accepted_signers: BTreeSet<String>,
    /// The predicates an accepted attestation must carry (per-source rules).
    pub required_predicates: BTreeSet<String>,
    /// The record/extension spellings requiring a verified signature at register
    /// (`component_variant`, `plugin`, `mcp_server`, `skill`, …) — a matching
    /// record lands `quarantined` until a `pin` endorsement lifts it.
    pub require_signature_for: BTreeSet<String>,
    /// The authority ceiling a hash-only pin may confer (default `external`).
    pub hash_only_ceiling: hh_provenance::AuthorityClass,
    /// The freshness bound for signature-required records (transaction-time).
    pub max_age: Option<u64>,
    /// The signer-set drift posture (`quarantine` — a removed signer's records
    /// keep their admission; the *pin* path is the re-verification op).
    pub drift_policy: String,
    /// The scanner posture (advisory — scanners are not part of this slice).
    pub scanner_policy: String,
    /// The model-package install posture (closed sum).
    pub model_install: ModelInstallRule,
    /// The archive-trust posture (advisory — archives lift to candidates only).
    pub archive_policy: String,
    /// Extension members (never authority-relevant).
    pub ext: BTreeMap<String, Json>,
}

/// A `registry_snapshot` record (ADR-0038 — a closure, not a catalog). The
/// `registry_snapshot_id` is the snapshot role of the TUF timestamp-role freshness
/// model (ADR-0153 D3); `resolve`/`query`/`slot_choices` over it are deterministic (R8).
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
    /// A `trust_root_policy` — the registry's trust anchor set (S4.1; §6.2;
    /// ADR-0153 D3/D4). Registering one requires `principal`+ authority.
    TrustRootPolicy(TrustRootPolicy),
    /// A `capability` — a registered `ToolCapability` HIR/1 node.
    Capability(CapabilityRecord),
    /// A `metric_declaration` — the full §5h.2 `MetricDeclaration` is the body
    /// (R-2.9.2; the ontology owns the schema — CC7).
    MetricDeclaration(MetricDeclaration),
    /// A `validator` — the §5h.2 `OracleDeclaration` is the body (R-2.9.2;
    /// ADR-0047). A Validator *component* registers as a `variant` of the
    /// `validator` class; this kind is the oracle's declaration record.
    Validator(OracleDeclaration),
    /// An `extension` — the §5g.5 `ExtensionRecord` (R-2.8.5⁰): declared-source
    /// locator + content pin + trust record; a registry-layer versioned record,
    /// never a HIR entity.
    Extension(crate::extension::ExtensionRecord),
    /// An `environment_family` — the §5h.4 `EnvironmentFamilyRecord` is the
    /// body (R-2.9.4⁰ᵃ; the ontology owns the schema — CC7; S1.24).
    EnvironmentFamily(hh_ontology::lab::EnvironmentFamilyRecord),
    /// An `environment_record` — the §5a.5 record body is owned by the
    /// environment plane (`hh-env`, which sits *above* this crate — the
    /// dependency direction forbids a typed arm). The registry stores the
    /// record's canonical `hh_identity::record::Record::canonical_full()` Json
    /// verbatim (records-in/records-out — R3) so `version_id =
    /// idp("environment", H(body))` mints the same coordinate `hh-env`
    /// computes (CC1). The registry projects only the pinned
    /// `semantic.containment_policy.version_id` into the dependency/stale
    /// index and snapshot closure — everything else is opaque.
    EnvironmentRecord(Json),
    /// A `participant` record — the §6.6 `ParticipantRecord` (R-2.10.6⁰). The
    /// schema owner is `hh-hosting` (above this crate — same layering as
    /// `EnvironmentRecord`): the registry stores the canonical body verbatim and
    /// gates only on the `kind: "participant"` tag (`record_from_json`).
    Participant(Json),
    /// An `adapter` record — the §6.6 `AdapterRecord` (R-2.10.6⁰). Same opaque
    /// layering as `Participant`: `hh-hosting` owns the schema; the registry
    /// stores the canonical body verbatim (`kind: "adapter"` tag gated).
    Adapter(Json),
    /// A `leaderboard_definition` — the §6.5 named `LeaderboardDefinition`
    /// record (R-2.10.5 C1; ADR-0294). `hh-results` owns the schema (the same
    /// opaque layering as `Participant`/`Adapter`): the registry stores the
    /// canonical body verbatim and gates the `kind: "leaderboard_definition"`
    /// tag (`record_from_json`); the definition's `definition_id` is its
    /// `version_id` and its name binds through `publish` under ADR-0153
    /// namespace authority (OQ-372).
    LeaderboardDefinition(Json),
    /// A `sealed_definition` — a sealed `hir/1` document plus its own identity
    /// (`R-2.10.1`; `hh-hir` owns the schema — `hh_hir::wire::sealed_*` is the
    /// one codec, CC7). The record's `version_id`/`semantic_id` ARE the
    /// definition's own `definition_ref` coordinates (the same treatment
    /// `Capability` gives a `ToolCapability` node — one coordinate wherever the
    /// definition is pinned).
    SealedDefinition(hh_hir::SealedDefinition),
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
            RegistryRecord::TrustRootPolicy(_) => RecordKind::TrustRootPolicy,
            RegistryRecord::Capability(_) => RecordKind::Capability,
            RegistryRecord::MetricDeclaration(_) => RecordKind::MetricDeclaration,
            RegistryRecord::Validator(_) => RecordKind::Validator,
            RegistryRecord::Extension(_) => RecordKind::Extension,
            RegistryRecord::EnvironmentFamily(_) => RecordKind::EnvironmentFamily,
            RegistryRecord::EnvironmentRecord(_) => RecordKind::EnvironmentRecord,
            RegistryRecord::Participant(_) => RecordKind::Participant,
            RegistryRecord::Adapter(_) => RecordKind::Adapter,
            RegistryRecord::LeaderboardDefinition(_) => RecordKind::LeaderboardDefinition,
            RegistryRecord::SealedDefinition(_) => RecordKind::SealedDefinition,
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
            RegistryRecord::Namespace(_)
            | RegistryRecord::ForeignImport(_)
            // `accepted_signers` names signer identity coordinates (anchor
            // spellings), never registry `version_id`s — no closure edges.
            | RegistryRecord::TrustRootPolicy(_) => Vec::new(),
            // The opaque participant/adapter/leaderboard-definition bodies
            // carry no registry-level pins (C1 — their internal refs live
            // inside the body; `hh-hosting`/`hh-results` own any projection
            // that reads them).
            RegistryRecord::Participant(_)
            | RegistryRecord::Adapter(_)
            | RegistryRecord::LeaderboardDefinition(_) => Vec::new(),
            // `applies_to_families` scopes by family *name* (the same treatment
            // `VariantRecord.applies_to.families` gets) — names are never pins.
            RegistryRecord::MetricDeclaration(_) => Vec::new(),
            // `family_id` is a closed-sum value, never a pin (same treatment
            // as `applies_to_families`).
            RegistryRecord::EnvironmentFamily(_) => Vec::new(),
            // The pinned `semantic.containment_policy.version_id` is the only
            // registry-level edge the opaque env body carries (the snapshot
            // closure + stale index consume it — R7). Blob refs under `refs[]`
            // are content addresses, never registry `version_id`s.
            RegistryRecord::EnvironmentRecord(body) => body
                .get("semantic")
                .and_then(|s| s.get("containment_policy"))
                .and_then(|c| c.get("version_id"))
                .and_then(|v| v.as_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
            // `calibration_ref` is a pinned `version_id` of the deterministic
            // oracle the judge calibrates against (ADR-0047(c)(ii)).
            RegistryRecord::Validator(o) => o.calibration_ref.iter().cloned().collect(),
            // The extension's snapshot-closure edge set: contributed components
            // + conferred grant pins (R7 — both are `VersionedRef`s).
            RegistryRecord::Extension(e) => e
                .contributes
                .iter()
                .chain(e.trust.grants.iter())
                .map(|r| r.version_id.clone())
                .collect(),
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
            // A sealed definition's registry-level pins: the resolved slot-variant
            // `version_id`s, the declared extension pins and the resolve snapshot —
            // the closure a stale/revoked variant or a superseded snapshot must
            // reach (R7; the snapshot row keeps `resolve`'s confinement auditable).
            RegistryRecord::SealedDefinition(s) => {
                let mut out: Vec<String> = Vec::new();
                if let Some(Json::Obj(a)) = &s.document.assembly {
                    // `slots` + `extensions` carry the only pins that can name
                    // registry records (variant and extension `version_id`s);
                    // `resolved.registry_snapshot_id` is the resolve's
                    // confinement record. `entities`/`values` pins are
                    // doc-internal coordinates — never registry members.
                    for member in ["slots", "extensions"] {
                        if let Some(v) = a.get(member) {
                            collect_version_pins(v, &mut out);
                        }
                    }
                    if let Some(v) = a
                        .get("resolved")
                        .and_then(|r| r.get("registry_snapshot_id"))
                        .and_then(Json::as_str)
                    {
                        out.push(v.to_string());
                    }
                }
                out
            }
        }
    }
}

/// Collect every `version_id` member under `j` (the pinned refs a sealed
/// assembly section's `slots`/`extensions` members carry).
fn collect_version_pins(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if k == "version_id" && v.as_str().is_some() {
                    out.push(v.as_str().unwrap_or_default().to_string());
                } else {
                    collect_version_pins(v, out);
                }
            }
        }
        Json::Arr(items) => {
            for v in items {
                collect_version_pins(v, out);
            }
        }
        _ => {}
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
