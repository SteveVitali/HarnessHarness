//! `validators` — the `Validator` component contract (spec §5f.1; ADR-0110,
//! ADR-0111): `ValidatorDeclaration` + `declare`/`bind`, `Verdict`/`Finding`,
//! and the kernel local checks (a) `schema_conformance`, (b)
//! `exit_status_class`, (c) `patch_application` (+ `touched_paths ⊆
//! resource_keys[]`), (d) `diff_sanity` — the Stage-2 conditioned rule with
//! an assumption-debt record (ADR-0111 D1(d); thresholds model/task-dependent,
//! provisional defaults pending OQ-267 — the debt record owns that).

use std::collections::BTreeSet;

use hh_hir::kinds::{EffectDomain, ValidatorKind};
use hh_identity::refs::VersionedRef;
use hh_ontology::participant::Observability;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::Json;

use crate::evidence::{inputs_digest, EvidenceBundle};
use crate::vocab::{
    ChargedTo, CriterionRole, Detector, EvidenceKind, Freshness, InconclusiveReason, Integrity,
    Isolation, OracleCause, OracleClass, SeverityLevel, VerdictPhase, VerdictStatus, VerdictType,
    VerdictValue,
};

// ── EvidenceRequirement / declaration ───────────────────────────────────────

/// `EvidenceRequirement` — a declared evidence input of a `ValidatesRecord` or
/// `ValidatorDeclaration` (ADR-0109 D2): `{kind, min_authority ≥ environment,
/// freshness, integrity, scope?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRequirement {
    /// The required kind.
    pub kind: EvidenceKind,
    /// The minimum authority — never below `environment` (enforced on
    /// construction consumers, checked in [`EvidenceRequirement::validate`]).
    pub min_authority: AuthorityClass,
    /// The freshness arm.
    pub freshness: Freshness,
    /// The integrity arm.
    pub integrity: Integrity,
    /// An optional scope restriction.
    pub scope: Option<String>,
}

impl EvidenceRequirement {
    /// The contract: `min_authority ≥ environment`.
    pub fn validate(&self) -> Result<(), ValidatorError> {
        if self.min_authority < AuthorityClass::Environment {
            return Err(ValidatorError::IncompleteDeclaration {
                reason: "evidence requirement min_authority below environment".into(),
            });
        }
        Ok(())
    }
}

/// `ValidatorDeclaration` — the `Validator` component's declaration
/// (ADR-0110 D1; `Validator` entity semantic fields per ADR-0016 Phase 2 log).
/// `kind` reuses the HIR closed sum [`ValidatorKind`] (CC7 — never a second
/// spelling).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatorDeclaration {
    /// The pinned validator ref.
    pub validator_ref: VersionedRef,
    /// The validator kind (`executable | schema | predicate | judge | human`).
    pub kind: ValidatorKind,
    /// The oracle class (ADR-0047 D1).
    pub oracle_class: OracleClass,
    /// Whether the check is pure in `(inputs_digest, validator version_id)`.
    /// `kind = judge ⇒ deterministic = false`.
    pub deterministic: bool,
    /// The declared evidence inputs — **never empty** (I-V1).
    pub evidence_inputs: Vec<EvidenceRequirement>,
    /// The evidence kinds the verdict emits.
    pub evidence_out: Vec<EvidenceKind>,
    /// The declared verdict type.
    pub verdict_type: VerdictType,
    /// The observability the check requires (`model_io` mandatory for judges —
    /// I-V4).
    pub requires_observability: BTreeSet<Observability>,
    /// The declared cost model ref (a §8.2-owned payload carried opaquely).
    pub cost_model: Option<String>,
    /// The isolation arm.
    pub isolation: Isolation,
    /// Declared side-effect domains — must be a subset of the read-only
    /// domain `{fs_read}` (a validator is effect-free; `ValidatorHasEffects`
    /// at `bind` otherwise).
    pub side_effects: BTreeSet<EffectDomain>,
    /// The judge's profile ref (`kind = judge` ⇒ mandatory).
    pub profile_ref: Option<String>,
    /// The judge's calibration ref.
    pub calibration_ref: Option<String>,
    /// Who the check's spend is charged to.
    pub charged_to: ChargedTo,
    /// The assumption-debt record (`kind = judge` ⇒ mandatory; held-out
    /// suites/threshold-conditioned validators ⇒ mandatory per AC-R-2.7.1-12).
    pub assumption_debt: Option<String>,
}

/// `declare`/`bind`/`collect`/`check` failures (typed — never a warning).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorError {
    /// `evidence_inputs = ∅`, or `kind = judge` without `profile_ref`/
    /// `assumption_debt`, or `deterministic` contradicting `kind`/`oracle_class`
    /// (`IncompleteDeclaration`).
    IncompleteDeclaration {
        /// Why the declaration is incomplete.
        reason: String,
    },
    /// A bound payload's hash mismatches the pinned `ContentAddress`
    /// (`PayloadHashMismatch`).
    PayloadHashMismatch {
        /// The pinned hash.
        expected: String,
        /// The observed hash.
        observed: String,
    },
    /// A fixture the declaration needs is unbound (`UnboundFixture`).
    UnboundFixture {
        /// The fixture ref.
        fixture_ref: String,
    },
    /// The declaration carries non-read-only `side_effects`
    /// (`ValidatorHasEffects` — AC-R-2.7.1-2).
    ValidatorHasEffects {
        /// The offending domains.
        domains: Vec<String>,
    },
    /// A verdict's authority contradicts its detector class (deterministic ⇒
    /// `kernel`; judge ⇒ `delegate`; human ⇒ `principal` — §5f.1 §6).
    VerdictAuthorityMismatch {
        /// The detector class.
        detector: Detector,
        /// The authority the verdict carried.
        authority: AuthorityClass,
    },
}

impl std::fmt::Display for ValidatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorError::IncompleteDeclaration { reason } => {
                write!(f, "IncompleteDeclaration: {reason}")
            }
            ValidatorError::PayloadHashMismatch { expected, observed } => {
                write!(f, "PayloadHashMismatch: {expected} != {observed}")
            }
            ValidatorError::UnboundFixture { fixture_ref } => {
                write!(f, "UnboundFixture: {fixture_ref}")
            }
            ValidatorError::ValidatorHasEffects { domains } => {
                write!(f, "ValidatorHasEffects: {domains:?}")
            }
            ValidatorError::VerdictAuthorityMismatch {
                detector,
                authority,
            } => {
                write!(
                    f,
                    "VerdictAuthorityMismatch: {detector:?} verdict at {authority:?}"
                )
            }
        }
    }
}

impl std::error::Error for ValidatorError {}

/// The read-only side-effect domains a validator may declare (a validator is
/// effect-free; `fs_read` is the only read domain).
pub const READ_ONLY_DOMAINS: [EffectDomain; 1] = [EffectDomain::FsRead];

/// `declare` — the declaration contract (ADR-0110 D1): `evidence_inputs ≠ ∅`
/// (I-V1); `kind = judge ⇒ deterministic = false ∧ profile_ref ∧
/// assumption_debt`; `deterministic ⇒ oracle_class` is a deterministic class;
/// every evidence requirement's `min_authority ≥ environment`.
pub fn declare(decl: &ValidatorDeclaration) -> Result<(), ValidatorError> {
    if decl.evidence_inputs.is_empty() {
        return Err(ValidatorError::IncompleteDeclaration {
            reason: "evidence_inputs is empty (I-V1)".into(),
        });
    }
    if matches!(decl.kind, ValidatorKind::Judge(_)) {
        if decl.deterministic {
            return Err(ValidatorError::IncompleteDeclaration {
                reason: "kind = judge ⇒ deterministic = false".into(),
            });
        }
        if decl.profile_ref.is_none() || decl.assumption_debt.is_none() {
            return Err(ValidatorError::IncompleteDeclaration {
                reason: "kind = judge requires profile_ref and assumption_debt".into(),
            });
        }
        // I-V4: a judged detector requires `model_io` observability.
        if !decl
            .requires_observability
            .contains(&Observability::ModelIo)
        {
            return Err(ValidatorError::IncompleteDeclaration {
                reason: "detector = judged ⇒ requires_observability ∋ model_io (I-V4)".into(),
            });
        }
    }
    if decl.deterministic && !decl.oracle_class.is_c0_headline() {
        return Err(ValidatorError::IncompleteDeclaration {
            reason: "deterministic ⇒ oracle_class ∈ the deterministic set".into(),
        });
    }
    for req in &decl.evidence_inputs {
        req.validate()?;
    }
    Ok(())
}

/// `bind` — the binding contract's Stage-1 half: a validator declared with
/// non-read-only `side_effects` is refused (`ValidatorHasEffects`; effect-
/// bearing checks go through the ordinary effect lifecycle). The payload-hash
/// and fixture-binding halves ride [`crate::evidence::pin_check`] and the
/// caller's resolution.
pub fn bind(decl: &ValidatorDeclaration) -> Result<(), ValidatorError> {
    let bad: Vec<String> = decl
        .side_effects
        .iter()
        .filter(|d| !READ_ONLY_DOMAINS.contains(d))
        .map(domain_name)
        .collect();
    if !bad.is_empty() {
        return Err(ValidatorError::ValidatorHasEffects { domains: bad });
    }
    Ok(())
}

fn domain_name(d: &EffectDomain) -> String {
    format!("{d:?}")
}

// ── Finding / Verdict ───────────────────────────────────────────────────────

/// `Finding` — one typed finding on a verdict (`{code, severity, location?,
/// message}`; `code` registered per detector — OQ-284's taxonomy is Stage 4's).
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// The finding code (the detector's registered kind).
    pub code: String,
    /// The severity.
    pub severity: SeverityLevel,
    /// The location, when the finding has one.
    pub location: Option<String>,
    /// The message (a profile-rendered `Text`, never the verdict).
    pub message: String,
    /// The bundle item the finding cites — mandatory for critic findings
    /// (uncited findings are dropped and counted, ADR-0115 D5).
    pub evidence_ref: Option<String>,
}

/// `Verdict` — the `Observation{source = validator}` payload (ADR-0110 D3).
/// A verdict is a typed value, never a bare score (ADR-0047 D2), and it is a
/// *new* observation — annotate or reject, never rewrite, never un-execute
/// (I-V3).
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// The verdict id.
    pub verdict_id: String,
    /// The validator that produced it (pinned `VersionedRef`).
    pub validator_ref: VersionedRef,
    /// The oracle class.
    pub oracle_class: OracleClass,
    /// The check's target.
    pub target: String,
    /// The criterion, when contract-bound.
    pub criterion_ref: Option<String>,
    /// The task contract id, when contract-bound.
    pub contract_id: Option<String>,
    /// The placement phase.
    pub phase: VerdictPhase,
    /// The criterion role.
    pub role: CriterionRole,
    /// The typed value.
    pub value: VerdictValue,
    /// The status (`decided | inconclusive{reason} | oracle_failure{cause}`).
    pub status: VerdictStatus,
    /// The detector class (one sum for `Verdict` and the chain events —
    /// ADR-0014 as amended, CF-483).
    pub detector: Detector,
    /// The evidence the verdict cites.
    pub evidence_refs: Vec<String>,
    /// The bundle's `inputs_digest` — the R1 replay anchor.
    pub inputs_digest: String,
    /// The ledger head the evidence was read at.
    pub evidence_head_seq: u64,
    /// Whether the freshness requirements held.
    pub freshness_ok: bool,
    /// The findings.
    pub findings: Vec<Finding>,
    /// The measured cost (charged — `dimension = evaluator_calls`).
    pub cost_ppm: u64,
    /// Who the check was charged to.
    pub charged_to: ChargedTo,
    /// The veto invariants this verdict tripped.
    pub veto_tripped: Vec<String>,
    /// The verdict's provenance (`origin ∈ kernel(validator_ref) |
    /// model(judge) | human(rater)`).
    pub provenance: ProvenanceRecord,
    /// The seq the check ran at.
    pub measured_at: u64,
}

impl Verdict {
    /// The detector class ⇒ the verdict's minted authority (deterministic ⇒
    /// `kernel`; judge ⇒ `delegate`; human ⇒ `principal` — §5f.1 §6; verdict
    /// authority is never an evidence weight, ADR-0033 D8).
    pub fn expected_authority(detector: Detector) -> AuthorityClass {
        match detector {
            Detector::Deterministic => AuthorityClass::Kernel,
            Detector::Judged => AuthorityClass::Delegate,
            Detector::Human => AuthorityClass::Principal,
        }
    }

    /// The verdict contract: the minted authority matches the detector class
    /// (no arbitrary authority elevation — CC2) and a `decided` verdict
    /// carries an `inputs_digest`.
    pub fn validate(&self) -> Result<(), ValidatorError> {
        let expected = Verdict::expected_authority(self.detector);
        if self.provenance.authority != expected {
            return Err(ValidatorError::VerdictAuthorityMismatch {
                detector: self.detector,
                authority: self.provenance.authority,
            });
        }
        Ok(())
    }

    /// Whether the verdict is an affirmative decision (a `decided` pass —
    /// `inconclusive`/`oracle_failure` are never passes).
    pub fn is_pass(&self) -> bool {
        self.status == VerdictStatus::Decided && self.value.is_affirmative()
    }
}

// ── Kernel local checks (a)–(c) — ADR-0111 D1 ───────────────────────────────

/// `LocalCheckId` — the kernel local checks ((d) `diff_sanity` is a Stage-2
/// conditioned rule — `DiffSanityRule` carries its assumption-debt record;
/// ablatable: `run_local_checks` takes the rule as an `Option`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCheckId {
    /// (a) `schema_conformance` vs `ToolCapability.output_schema`.
    SchemaConformance,
    /// (b) `exit_status_class ∈ {success, failure(code), timeout, signal}`
    /// for `exec` domains.
    ExitStatusClass,
    /// (c) `patch_application ∈ {applied, rejected(hunks), partial}` and
    /// `touched_paths ⊆ resource_keys[]` for `fs_write`.
    PatchApplication,
    /// (d) `diff_sanity` — file/line/byte bounds and no path outside the
    /// sealed `Permission` scope (ADR-0111 D1(d); a conditioned rule with an
    /// assumption-debt record — AC-R-2.7.1-12).
    DiffSanity,
}

impl LocalCheckId {
    /// The canonical spelling (the built-in validator's semantic tail under
    /// `hir/kernel/<check>`).
    pub fn as_str(self) -> &'static str {
        match self {
            LocalCheckId::SchemaConformance => "schema_conformance",
            LocalCheckId::ExitStatusClass => "exit_status_class",
            LocalCheckId::PatchApplication => "patch_application",
            LocalCheckId::DiffSanity => "diff_sanity",
        }
    }
}

/// The kernel builtin's semantic coordinate prefix — `hir/kernel/<check>`
/// (ADR-0111 D1: kernel-owned `Validator` nodes in the reference dialect).
pub const KERNEL_CHECK_PREFIX: &str = "hir/kernel/";

/// `resolve_builtin_check(semantic_id)` — the `Procedure.Verify`/
/// `postconditions[]` resolver: a `Ref<Validator>` whose semantic id is
/// `hir/kernel/<check>` resolves to the named built-in. Anything else is
/// `None` — an unresolvable declared validator is reported, never silently
/// skipped.
pub fn resolve_builtin_check(semantic_id: &str) -> Option<LocalCheckId> {
    match semantic_id.strip_prefix(KERNEL_CHECK_PREFIX) {
        Some("schema_conformance") => Some(LocalCheckId::SchemaConformance),
        Some("exit_status_class") => Some(LocalCheckId::ExitStatusClass),
        Some("patch_application") => Some(LocalCheckId::PatchApplication),
        Some("diff_sanity") => Some(LocalCheckId::DiffSanity),
        _ => None,
    }
}

/// `PatchStatus` — the capture's patch-application fact for `fs_write`
/// effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchStatus {
    /// `applied`.
    Applied,
    /// `rejected(hunks)`.
    Rejected(u64),
    /// `partial`.
    Partial,
}

/// `TerminalCapture` — what the runtime already holds after a terminal
/// capture (§05d): the facts the kernel local checks read. Pure data — the
/// checks never touch the store or the environment.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCapture {
    /// The effect id.
    pub effect_id: String,
    /// The effect domain (check applicability keys on it).
    pub domain: Option<EffectDomain>,
    /// The captured output value (E6's fields — already lowered).
    pub output: Json,
    /// The capability's `output_schema`, when declared.
    pub output_schema: Option<Json>,
    /// The exit status, when a process ran.
    pub exit_status: Option<i64>,
    /// Whether the capture classified the exit as a timeout.
    pub timed_out: bool,
    /// Whether the capture classified the exit as a signal.
    pub signalled: bool,
    /// The patch-application fact for `fs_write`.
    pub patch_status: Option<PatchStatus>,
    /// The paths the effect touched.
    pub touched_paths: Vec<String>,
    /// The capability's `resource_keys[]`.
    pub resource_keys: Vec<String>,
    /// (d) input: the changed-file count, when the effect produced a diff
    /// (`None` ⇒ no diff facts — the check does not apply).
    pub diff_files: Option<u64>,
    /// (d) input: the changed-line count, when known (`None` ⇒ that bound is
    /// undecidable — skipped, never guessed).
    pub diff_lines: Option<u64>,
    /// (d) input: the diff/artifact byte count, when known (`None` ⇒ that
    /// bound is undecidable — skipped, never guessed).
    pub diff_bytes: Option<u64>,
    /// (d) input: the touched paths the sealed `Permission` scope refused
    /// (kernel-derived from the capture's `inside_writable_roots` — never
    /// executor-reported).
    pub outside_scope: Vec<String>,
}

/// The canonical capture preimage — the `inputs_digest` input that makes a
/// local verdict pure in `(inputs_digest, validator version_id)` (R1).
fn capture_preimage(cap: &TerminalCapture, check: LocalCheckId) -> String {
    Json::obj([
        ("check", Json::str(check.as_str())),
        (
            "domain",
            cap.domain
                .map_or(Json::Null, |d| Json::str(domain_name(&d))),
        ),
        ("effect_id", Json::str(cap.effect_id.clone())),
        ("exit_status", cap.exit_status.map_or(Json::Null, Json::Int)),
        ("output", cap.output.clone()),
        (
            "output_schema",
            cap.output_schema.clone().unwrap_or(Json::Null),
        ),
        (
            "patch_status",
            cap.patch_status.map_or(Json::Null, |p| match p {
                PatchStatus::Applied => Json::str("applied"),
                PatchStatus::Rejected(n) => Json::str(format!("rejected({n})")),
                PatchStatus::Partial => Json::str("partial"),
            }),
        ),
        (
            "resource_keys",
            Json::Arr(cap.resource_keys.iter().map(Json::str).collect()),
        ),
        ("signalled", Json::Bool(cap.signalled)),
        ("timed_out", Json::Bool(cap.timed_out)),
        (
            "touched_paths",
            Json::Arr(cap.touched_paths.iter().map(Json::str).collect()),
        ),
        (
            "diff_files",
            cap.diff_files.map_or(Json::Null, |n| Json::Int(n as i64)),
        ),
        (
            "diff_lines",
            cap.diff_lines.map_or(Json::Null, |n| Json::Int(n as i64)),
        ),
        (
            "diff_bytes",
            cap.diff_bytes.map_or(Json::Null, |n| Json::Int(n as i64)),
        ),
        (
            "outside_scope",
            Json::Arr(cap.outside_scope.iter().map(Json::str).collect()),
        ),
    ])
    .to_canonical_string()
}

/// `ExitClass` — check (b)'s closed class sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    /// `success` — exit status 0.
    Success,
    /// `failure(code)` — a nonzero exit.
    Failure(i64),
    /// `timeout`.
    Timeout,
    /// `signal`.
    Signal,
    /// No determinable class (no exit status — `inconclusive`).
    Missing,
}

/// (b) `exit_status_class` — classify the captured exit.
pub fn exit_status_class(cap: &TerminalCapture) -> ExitClass {
    if cap.timed_out {
        return ExitClass::Timeout;
    }
    if cap.signalled {
        return ExitClass::Signal;
    }
    match cap.exit_status {
        Some(0) => ExitClass::Success,
        Some(code) => ExitClass::Failure(code),
        None => ExitClass::Missing,
    }
}

/// The OQ-219 interim JSON-Schema keyword subset the kernel's
/// `schema_conformance` check admits — **identical** to
/// `hh_compiler::equiv::E3_ADMITTED_KEYWORDS` (the one interim subset,
/// ADR-0212; a test pins the equality so the two sides cannot drift — CC7).
/// A keyword outside this set makes the check `inconclusive{unchecked_keyword}`
/// — the checker never guesses (T-LCD-15).
pub const CONFORMANCE_ADMITTED_KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "const",
    "required",
    "properties",
    "items",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "additionalProperties",
    "description",
    "title",
    "default",
    "$defs",
    "format",
];

/// The conformance check over the OQ-219 subset — `Ok(bool)` when every
/// keyword is admitted, `Err(keyword)` on an unchecked keyword (never a guess).
/// `additionalProperties: false` is enforced; `additionalProperties` schemas
/// are admitted but only `false` constrains (the interim subset's rule).
pub fn conforms(schema: &Json, instance: &Json) -> Result<bool, String> {
    let Json::Obj(m) = schema else {
        return Ok(true); // a non-object schema carries no constraints
    };
    for k in m.keys() {
        if !CONFORMANCE_ADMITTED_KEYWORDS.contains(&k.as_str()) {
            return Err(k.clone());
        }
    }
    if let Some(c) = m.get("const") {
        if c != instance {
            return Ok(false);
        }
    }
    if let Some(Json::Arr(variants)) = m.get("enum") {
        if !variants.contains(instance) {
            return Ok(false);
        }
    }
    if let Some(ty) = m.get("type").and_then(Json::as_str) {
        let ok = match ty {
            "object" => matches!(instance, Json::Obj(_)),
            "array" => matches!(instance, Json::Arr(_)),
            "string" => matches!(instance, Json::Str(_)),
            "integer" | "number" => matches!(instance, Json::Int(_)),
            "boolean" => matches!(instance, Json::Bool(_)),
            "null" => matches!(instance, Json::Null),
            _ => return Err(format!("type:{ty}")),
        };
        if !ok {
            return Ok(false);
        }
    }
    if let Some(Json::Arr(req)) = m.get("required") {
        if let Json::Obj(im) = instance {
            for r in req {
                if let Some(k) = r.as_str() {
                    if !im.contains_key(k) {
                        return Ok(false);
                    }
                }
            }
        }
    }
    if let (Some(Json::Obj(props)), Json::Obj(im)) = (m.get("properties"), instance) {
        for (k, sub) in props {
            if let Some(v) = im.get(k) {
                if !conforms(sub, v)? {
                    return Ok(false);
                }
            }
        }
        if m.get("additionalProperties") == Some(&Json::Bool(false)) {
            for k in im.keys() {
                if !props.contains_key(k) {
                    return Ok(false);
                }
            }
        }
    }
    if let (Some(items), Json::Arr(vals)) = (m.get("items"), instance) {
        for v in vals {
            if !conforms(items, v)? {
                return Ok(false);
            }
        }
    }
    if let Json::Str(s) = instance {
        if let Some(Json::Int(min)) = m.get("minLength") {
            if (s.chars().count() as i64) < *min {
                return Ok(false);
            }
        }
        if let Some(Json::Int(max)) = m.get("maxLength") {
            if (s.chars().count() as i64) > *max {
                return Ok(false);
            }
        }
    }
    if let Json::Int(n) = instance {
        if let Some(Json::Int(min)) = m.get("minimum") {
            if n < min {
                return Ok(false);
            }
        }
        if let Some(Json::Int(max)) = m.get("maximum") {
            if n > max {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// `DiffSanityThresholds` — the conditioned bounds check (d) enforces
/// (ADR-0111 D1(d): "size/line/file-count bounds and no path outside the
/// sealed `Permission` scope"). The defaults are **provisional** pending
/// OQ-267 (the assumption-debt record on [`DiffSanityRule`] owns that — a
/// threshold change is a rule-version change, never a silent kernel edit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSanityThresholds {
    /// Maximum changed-file count per effect.
    pub max_files: u64,
    /// Maximum changed-line count per effect (`None` in the facts ⇒ skipped).
    pub max_lines: u64,
    /// Maximum diff/artifact bytes per effect (`None` in the facts ⇒ skipped).
    pub max_bytes: u64,
}

impl Default for DiffSanityThresholds {
    /// The provisional OQ-267 defaults — recorded on the assumption-debt
    /// record (`hypothesis`: "these bounds discriminate pathological diffs
    /// for the conditioned model/task classes").
    fn default() -> Self {
        DiffSanityThresholds {
            max_files: 64,
            max_lines: 8_192,
            max_bytes: 1 << 20,
        }
    }
}

impl DiffSanityThresholds {
    /// Parse the conditioned params member (`{max_files, max_lines,
    /// max_bytes}` — a member absent defaults; a member present must be a
    /// non-negative integer, never coerced).
    pub fn from_json(j: &Json) -> Result<DiffSanityThresholds, ValidatorError> {
        let mut t = DiffSanityThresholds::default();
        let member = |k: &str| -> Result<Option<u64>, ValidatorError> {
            match j.get(k) {
                Some(Json::Int(n)) if *n >= 0 => Ok(Some(*n as u64)),
                Some(_) => Err(ValidatorError::IncompleteDeclaration {
                    reason: format!("diff_sanity.{k} must be a non-negative integer"),
                }),
                None => Ok(None),
            }
        };
        if let Some(v) = member("max_files")? {
            t.max_files = v;
        }
        if let Some(v) = member("max_lines")? {
            t.max_lines = v;
        }
        if let Some(v) = member("max_bytes")? {
            t.max_bytes = v;
        }
        Ok(t)
    }
}

/// `DiffSanityRule` — check (d) as a **conditioned rule**: the rule's
/// identity (`hir/kernel/diff_sanity`), its thresholds (the conditioned
/// parameters — `ProfileRule`-shaped at a future Stage; here one conditioned
/// rule over the unbound profile = applies to every profile), and the
/// mandatory assumption-debt record (AC-R-2.7.1-12: a threshold-conditioned
/// validator without a debt record is refused — the constructor builds the
/// complete record; there is no debt-less way to construct one).
#[derive(Debug, Clone, PartialEq)]
pub struct DiffSanityRule {
    /// The pinned validator/rule ref (`hir/kernel/diff_sanity@<v>`).
    pub rule_ref: VersionedRef,
    /// The conditioned thresholds.
    pub thresholds: DiffSanityThresholds,
    /// The assumption-debt record (T-LCD-05; mandatory).
    pub debt: hh_hir::records::AssumptionDebtRecord,
}

impl DiffSanityRule {
    /// The kernel default conditioned rule — `hir/kernel/diff_sanity`,
    /// `conditioned_on = unbound` (applies to every profile), provisional
    /// OQ-267 thresholds, complete debt record. `provenance` mints the debt's
    /// `hypothesis`/`created_by`.
    pub fn kernel_default(provenance: &ProvenanceRecord) -> DiffSanityRule {
        let rule_id = "hir/kernel/diff_sanity";
        DiffSanityRule {
            rule_ref: VersionedRef::pinned(
                hh_identity::kinds::RecordKind::Validator,
                "hir/kernel/diff_sanity@1",
                provenance.clone(),
            ),
            thresholds: DiffSanityThresholds::default(),
            debt: hh_hir::records::AssumptionDebtRecord {
                rule_id: rule_id.into(),
                hypothesis: hh_hir::leaves::Text::new(
                    "provisional OQ-267 bounds discriminate pathological diffs for                      the conditioned model/task classes; revisit when OQ-267 lands",
                    "hir/kernel/diff_sanity",
                    provenance.clone(),
                ),
                evidence_refs: vec![hh_hir::EvidenceRef::legacy("OQ-267")],
                owner: hh_hir::OwnerRef::principal("kernel"),
                expiry_condition: hh_hir::ExpiryCondition {
                    kind: hh_hir::ExpiryKind::EvidenceRefreshDue,
                    value: None,
                },
                removal_test_ref: "hir/kernel/diff_sanity/removal_test".into(),
                status: hh_hir::DebtStatus::Active,
                debt_class: Some(hh_hir::DebtClass::Hypothesized),
                hypothesis_typed: None,
                scope: Some(hh_hir::DebtScope::default()),
                expiry: None,
                runway_ms: None,
                revalidation: None,
                removal_test: Some(hh_hir::RemovalTest::new(
                    hh_hir::RemovalTestKind::Inspection,
                )),
                created_by: Some(provenance.clone()),
                created_at: None,
                supersedes: None,
            },
        }
    }

    /// The `conditioned_rules` listing entry (`(rule_id, debt)` — the
    /// `lcd_report`/`VariantRecord` shape; AC-R-2.7.1-12).
    pub fn conditioned_entry(&self) -> (String, hh_hir::records::AssumptionDebtRecord) {
        (self.debt.rule_id.clone(), self.debt.clone())
    }
}

/// Run the applicable kernel local checks over a terminal capture — exactly
/// the applicable verdicts, in check order (a, b, c, d); a check whose trigger
/// fact is absent produces **no** verdict (AC-R-2.7.1-1's "exactly the
/// applicable"). Each verdict is `detector = deterministic`, `phase = local`,
/// `charged_to = subject`, `kernel`-authority-provenanced by the caller —
/// this pure fold stamps a caller-supplied provenance template.
/// `diff_sanity` is check (d)'s conditioned rule — `None` ablates it
/// (T-LCD-02: built-ins are ablatable).
///
/// - (a) applies when `output_schema` is declared; an unchecked keyword ⇒
///   `inconclusive{unchecked_keyword}`, never a guess.
/// - (b) applies to `exec`-domain effects with a determinable exit fact;
///   `failure(code)`/`timeout`/`signal` ⇒ `fail`, `success` ⇒ `pass`, no exit
///   fact ⇒ `inconclusive{missing_evidence}`.
/// - (c) applies to `fs_write`-domain effects with a `patch_status`; `applied`
///   ∧ `touched_paths ⊆ resource_keys` ⇒ `pass`; `rejected`/`partial` or a
///   touched path outside `resource_keys` ⇒ `fail` with findings.
/// - (d) applies to `fs_write`-domain effects with a `diff_files` fact under
///   `Some(rule)`; an over-bound count or an `outside_scope` path ⇒ `fail`
///   with findings (undecidable count members are skipped, never guessed).
pub fn run_local_checks(
    cap: &TerminalCapture,
    validator_version: &VersionedRef,
    diff_sanity: Option<&DiffSanityRule>,
    provenance: ProvenanceRecord,
    evidence_head_seq: u64,
    measured_at: u64,
) -> Vec<Verdict> {
    let mut out = Vec::new();

    // (a) schema_conformance — applies when the capability declares a schema.
    if let Some(schema) = &cap.output_schema {
        let digest = inputs_digest(
            &["capture:".to_string() + &cap.effect_id],
            &cap.effect_id,
            &validator_version.version_id,
        );
        let _ = capture_preimage(cap, LocalCheckId::SchemaConformance); // preimage is the digest input at emit
        let (status, value, findings) = match conforms(schema, &cap.output) {
            Ok(true) => (VerdictStatus::Decided, VerdictValue::Bool(true), vec![]),
            Ok(false) => (
                VerdictStatus::Decided,
                VerdictValue::Bool(false),
                vec![Finding {
                    code: "schema_violation".into(),
                    severity: SeverityLevel::Medium,
                    location: None,
                    message: "output does not conform to output_schema".into(),
                    evidence_ref: None,
                }],
            ),
            Err(keyword) => (
                VerdictStatus::Inconclusive(InconclusiveReason::UncheckedKeyword),
                VerdictValue::ThreeValued(crate::vocab::ThreeValued::Inconclusive),
                vec![Finding {
                    code: "unchecked_keyword".into(),
                    severity: SeverityLevel::Info,
                    location: None,
                    message: format!("schema keyword `{keyword}` is outside the admitted subset"),
                    evidence_ref: None,
                }],
            ),
        };
        out.push(local_verdict(
            cap,
            validator_version,
            LocalCheckId::SchemaConformance,
            status,
            value,
            findings,
            digest,
            provenance.clone(),
            evidence_head_seq,
            measured_at,
        ));
    }

    // (b) exit_status_class — applies to exec-domain effects.
    if cap.domain == Some(EffectDomain::Exec) {
        let digest = inputs_digest(
            &["capture:".to_string() + &cap.effect_id],
            &cap.effect_id,
            &validator_version.version_id,
        );
        let class = exit_status_class(cap);
        let (status, value, findings) = match class {
            ExitClass::Success => (VerdictStatus::Decided, VerdictValue::Bool(true), vec![]),
            ExitClass::Failure(code) => (
                VerdictStatus::Decided,
                VerdictValue::Bool(false),
                vec![Finding {
                    code: format!("exit_status:{code}"),
                    severity: SeverityLevel::Medium,
                    location: None,
                    message: format!("exit status {code}"),
                    evidence_ref: None,
                }],
            ),
            ExitClass::Timeout => (
                VerdictStatus::Decided,
                VerdictValue::Bool(false),
                vec![Finding {
                    code: "exit_status:timeout".into(),
                    severity: SeverityLevel::Medium,
                    location: None,
                    message: "timed out".into(),
                    evidence_ref: None,
                }],
            ),
            ExitClass::Signal => (
                VerdictStatus::Decided,
                VerdictValue::Bool(false),
                vec![Finding {
                    code: "exit_status:signal".into(),
                    severity: SeverityLevel::Medium,
                    location: None,
                    message: "killed by signal".into(),
                    evidence_ref: None,
                }],
            ),
            ExitClass::Missing => (
                VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence),
                VerdictValue::ThreeValued(crate::vocab::ThreeValued::Inconclusive),
                vec![],
            ),
        };
        out.push(local_verdict(
            cap,
            validator_version,
            LocalCheckId::ExitStatusClass,
            status,
            value,
            findings,
            digest,
            provenance.clone(),
            evidence_head_seq,
            measured_at,
        ));
    }

    // (c) patch_application — applies to fs_write effects with a patch fact.
    if cap.domain == Some(EffectDomain::FsWrite) && cap.patch_status.is_some() {
        let digest = inputs_digest(
            &["capture:".to_string() + &cap.effect_id],
            &cap.effect_id,
            &validator_version.version_id,
        );
        let outside: Vec<&String> = cap
            .touched_paths
            .iter()
            .filter(|p| !cap.resource_keys.contains(p))
            .collect();
        let patch_ok = cap.patch_status == Some(PatchStatus::Applied);
        let mut findings = Vec::new();
        if let Some(PatchStatus::Rejected(n)) = cap.patch_status {
            findings.push(Finding {
                code: "patch_rejected".into(),
                severity: SeverityLevel::Medium,
                location: None,
                message: format!("{n} hunks rejected"),
                evidence_ref: None,
            });
        }
        if cap.patch_status == Some(PatchStatus::Partial) {
            findings.push(Finding {
                code: "patch_partial".into(),
                severity: SeverityLevel::Medium,
                location: None,
                message: "patch applied partially".into(),
                evidence_ref: None,
            });
        }
        for p in &outside {
            findings.push(Finding {
                code: "path_outside_resource_keys".into(),
                severity: SeverityLevel::High,
                location: Some((*p).clone()),
                message: format!("touched path {p} outside resource_keys"),
                evidence_ref: None,
            });
        }
        let ok = patch_ok && outside.is_empty();
        out.push(local_verdict(
            cap,
            validator_version,
            LocalCheckId::PatchApplication,
            VerdictStatus::Decided,
            VerdictValue::Bool(ok),
            findings,
            digest,
            provenance.clone(),
            evidence_head_seq,
            measured_at,
        ));
    }

    // (d) `diff_sanity` — the conditioned rule (ablatable: `None` ablates
    // it). Applies to `fs_write` effects with diff facts; the verdict cites
    // the conditioned rule's ref and the debt record rides the emitted
    // `verification.validator.verdict` payload's `conditioned_rule` member.
    if cap.domain == Some(EffectDomain::FsWrite) && cap.diff_files.is_some() {
        if let Some(rule) = diff_sanity {
            let files = cap.diff_files.unwrap_or(0);
            let mut findings = Vec::new();
            if files > rule.thresholds.max_files {
                findings.push(Finding {
                    code: "diff_files_over_bound".into(),
                    severity: SeverityLevel::Medium,
                    location: None,
                    message: format!(
                        "changed files {files} > bound {}",
                        rule.thresholds.max_files
                    ),
                    evidence_ref: None,
                });
            }
            if let Some(lines) = cap.diff_lines {
                if lines > rule.thresholds.max_lines {
                    findings.push(Finding {
                        code: "diff_lines_over_bound".into(),
                        severity: SeverityLevel::Medium,
                        location: None,
                        message: format!(
                            "changed lines {lines} > bound {}",
                            rule.thresholds.max_lines
                        ),
                        evidence_ref: None,
                    });
                }
            }
            if let Some(bytes) = cap.diff_bytes {
                if bytes > rule.thresholds.max_bytes {
                    findings.push(Finding {
                        code: "diff_bytes_over_bound".into(),
                        severity: SeverityLevel::Medium,
                        location: None,
                        message: format!(
                            "diff bytes {bytes} > bound {}",
                            rule.thresholds.max_bytes
                        ),
                        evidence_ref: None,
                    });
                }
            }
            for path in &cap.outside_scope {
                findings.push(Finding {
                    code: "path_outside_permission_scope".into(),
                    severity: SeverityLevel::High,
                    location: Some(path.clone()),
                    message: format!("path {path} outside the sealed Permission scope"),
                    evidence_ref: None,
                });
            }
            let digest = inputs_digest(
                &["capture:".to_string() + &cap.effect_id],
                &cap.effect_id,
                &rule.rule_ref.version_id,
            );
            out.push(local_verdict(
                cap,
                &rule.rule_ref,
                LocalCheckId::DiffSanity,
                VerdictStatus::Decided,
                VerdictValue::Bool(findings.is_empty()),
                findings,
                digest,
                provenance.clone(),
                evidence_head_seq,
                measured_at,
            ));
        }
    }

    out
}

/// Build one local-check verdict (kernel-builtins are `hir/kernel/<check>` —
/// the caller supplies the pinned `validator_version`).
#[allow(clippy::too_many_arguments)]
fn local_verdict(
    cap: &TerminalCapture,
    validator_version: &VersionedRef,
    check: LocalCheckId,
    status: VerdictStatus,
    value: VerdictValue,
    findings: Vec<Finding>,
    digest: String,
    provenance: ProvenanceRecord,
    evidence_head_seq: u64,
    measured_at: u64,
) -> Verdict {
    Verdict {
        verdict_id: format!("verdict:{}:{}", check.as_str(), cap.effect_id),
        validator_ref: validator_version.clone(),
        oracle_class: OracleClass::Executable,
        target: cap.effect_id.clone(),
        criterion_ref: None,
        contract_id: None,
        phase: VerdictPhase::Local,
        role: CriterionRole::Postcondition,
        value,
        status,
        detector: Detector::Deterministic,
        evidence_refs: vec![format!("capture:{}", cap.effect_id)],
        inputs_digest: digest,
        evidence_head_seq,
        freshness_ok: true,
        findings,
        cost_ppm: 0,
        charged_to: ChargedTo::Subject,
        veto_tripped: vec![],
        provenance,
        measured_at,
    }
}

/// The `check` operation's purity contract (R1): a deterministic `check` over
/// the same `EvidenceBundle` yields the same `value` and `inputs_digest`.
/// `oracle_failure` is an outcome class — modelled as a `VerdictStatus`, never
/// a value (ADR-0047 D4).
pub fn oracle_failure(
    cap_target: &str,
    validator_version: &VersionedRef,
    cause: OracleCause,
    provenance: ProvenanceRecord,
    measured_at: u64,
) -> Verdict {
    Verdict {
        verdict_id: format!("verdict:oracle_failure:{cap_target}"),
        validator_ref: validator_version.clone(),
        oracle_class: OracleClass::Executable,
        target: cap_target.to_string(),
        criterion_ref: None,
        contract_id: None,
        phase: VerdictPhase::Local,
        role: CriterionRole::Postcondition,
        value: VerdictValue::ThreeValued(crate::vocab::ThreeValued::Inconclusive),
        status: VerdictStatus::OracleFailure(cause),
        detector: Detector::Deterministic,
        evidence_refs: vec![],
        inputs_digest: String::new(),
        evidence_head_seq: 0,
        freshness_ok: false,
        findings: vec![],
        cost_ppm: 0,
        charged_to: ChargedTo::Subject,
        veto_tripped: vec![],
        provenance,
        measured_at,
    }
}

/// Re-export for callers that hold a bundle (the `check(bound, bundle)`
/// contract — deterministic purity is a property of `inputs_digest`, which
/// [`crate::evidence::build_bundle`] fixes).
pub fn bundle_digest(bundle: &EvidenceBundle) -> &str {
    &bundle.inputs_digest
}

// ── Declared postconditions + `Procedure.Verify` (S2.11; DF-S1.21-1) ───────

/// `run_declared_postcondition(ref, cap, diff_sanity, …)` — bind one
/// `ToolCapability.postconditions[]` / `Procedure.Verify` `Ref<Validator>`
/// to a check and run it over the terminal capture (§5f: "declared local
/// checks — `postconditions: [Ref<Validator>]` bound and run per applicable
/// terminal, feeding `Effect.postcondition_results[]`"). A `Ref` that does
/// not resolve to a kernel built-in yields an `inconclusive{missing_evidence}`
/// verdict naming the ref — declared-but-unbound is reported, never silently
/// skipped (T-LCD-15). `diff_sanity` supplies the conditioned rule when the
/// ref resolves to check (d); a `hir/kernel/diff_sanity` ref with `None`
/// ablates it (no verdict).
#[allow(clippy::too_many_arguments)]
pub fn run_declared_postcondition(
    validator_ref: &hh_hir::refs::Ref,
    cap: &TerminalCapture,
    diff_sanity: Option<&DiffSanityRule>,
    provenance: ProvenanceRecord,
    evidence_head_seq: u64,
    measured_at: u64,
) -> Option<Verdict> {
    let check = match resolve_builtin_check(&validator_ref.semantic_id) {
        Some(c) => c,
        None => {
            // Unbound declared validator — an `inconclusive` verdict that
            // names the ref (never a silent skip, never a guessed pass).
            return Some(Verdict {
                verdict_id: format!(
                    "verdict:postcondition:{}:{}",
                    validator_ref.semantic_id, cap.effect_id
                ),
                validator_ref: VersionedRef::pinned(
                    hh_identity::kinds::RecordKind::Validator,
                    match &validator_ref.version {
                        hh_hir::refs::RefVersion::Pinned(v) => v.clone(),
                        hh_hir::refs::RefVersion::Selector(s) => {
                            format!("{}@{s}", validator_ref.semantic_id)
                        }
                    },
                    provenance.clone(),
                ),
                oracle_class: OracleClass::Executable,
                target: cap.effect_id.clone(),
                criterion_ref: None,
                contract_id: None,
                phase: VerdictPhase::Local,
                role: CriterionRole::Postcondition,
                value: VerdictValue::ThreeValued(crate::vocab::ThreeValued::Inconclusive),
                status: VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence),
                detector: Detector::Deterministic,
                evidence_refs: vec![],
                inputs_digest: String::new(),
                evidence_head_seq,
                freshness_ok: false,
                findings: vec![Finding {
                    code: "unbound_validator".into(),
                    severity: SeverityLevel::High,
                    location: None,
                    message: format!(
                        "declared validator {} is not a bound built-in",
                        validator_ref.semantic_id
                    ),
                    evidence_ref: None,
                }],
                cost_ppm: 0,
                charged_to: ChargedTo::Subject,
                veto_tripped: vec![],
                provenance,
                measured_at,
            });
        }
    };
    if check == LocalCheckId::DiffSanity && diff_sanity.is_none() {
        return None; // the conditioned rule is ablated — no verdict
    }
    // Run exactly the one resolved check — reuse `run_local_checks` by
    // shaping a capture that only triggers that check.
    let mut sub = cap.clone();
    sub.output_schema = None;
    sub.domain = None;
    sub.patch_status = None;
    sub.diff_files = None;
    match check {
        LocalCheckId::SchemaConformance => {
            sub.output_schema = cap.output_schema.clone();
        }
        LocalCheckId::ExitStatusClass => {
            sub.domain = cap.domain;
        }
        LocalCheckId::PatchApplication => {
            sub.domain = cap.domain;
            sub.patch_status = cap.patch_status;
        }
        LocalCheckId::DiffSanity => {
            sub.domain = cap.domain;
            sub.diff_files = cap.diff_files;
        }
    }
    // The verdict stamps the resolved ref — the rule's ref for (d), the
    // declared ref's coordinate for (a)–(c) (a selector reports its resolved
    // spelling verbatim — provenance, never coercion).
    let stamped = match check {
        LocalCheckId::DiffSanity => diff_sanity.unwrap().rule_ref.clone(),
        _ => VersionedRef::pinned(
            hh_identity::kinds::RecordKind::Validator,
            match &validator_ref.version {
                hh_hir::refs::RefVersion::Pinned(v) => v.clone(),
                hh_hir::refs::RefVersion::Selector(s) => {
                    format!("{}@{s}", validator_ref.semantic_id)
                }
            },
            provenance.clone(),
        ),
    };
    run_local_checks(
        &sub,
        &stamped,
        diff_sanity,
        provenance,
        evidence_head_seq,
        measured_at,
    )
    .into_iter()
    .next()
}

/// `Procedure.Verify` — the step runner (§3.1.3 `Verify{validator}`; DF-S1.21-1's
/// Stage-2 half): resolves each `Verify` step's `Ref<Validator>` through
/// [`run_declared_postcondition`] over the terminal capture and returns the
/// verdicts in step order (a `Verify` on an ablated `diff_sanity` yields no
/// verdict — ablation is explicit, never silent).
#[allow(clippy::too_many_arguments)]
pub fn run_verify_steps(
    steps: &[hh_hir::records::ProcedureStep],
    cap: &TerminalCapture,
    diff_sanity: Option<&DiffSanityRule>,
    provenance: ProvenanceRecord,
    evidence_head_seq: u64,
    measured_at: u64,
) -> Vec<Verdict> {
    steps
        .iter()
        .filter_map(|s| match s {
            hh_hir::records::ProcedureStep::Verify { validator } => run_declared_postcondition(
                validator,
                cap,
                diff_sanity,
                provenance.clone(),
                evidence_head_seq,
                measured_at,
            ),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_identity::kinds::RecordKind;
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;

    fn kernel_prov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("hir/kernel/check", 3)
    }

    fn validator_ref() -> VersionedRef {
        VersionedRef::pinned(
            RecordKind::Validator,
            "sha256:v0",
            ProvenanceRecord::minted(
                Origin::kernel("hir/kernel/check"),
                PersistenceScope::Definition,
                1,
            ),
        )
    }

    fn req() -> EvidenceRequirement {
        EvidenceRequirement {
            kind: EvidenceKind::Observation,
            min_authority: AuthorityClass::Environment,
            freshness: Freshness::Any,
            integrity: Integrity::ChainVerified,
            scope: None,
        }
    }

    fn decl(kind: ValidatorKind) -> ValidatorDeclaration {
        ValidatorDeclaration {
            validator_ref: validator_ref(),
            kind,
            oracle_class: OracleClass::Executable,
            deterministic: true,
            evidence_inputs: vec![req()],
            evidence_out: vec![EvidenceKind::Observation],
            verdict_type: VerdictType::Bool,
            requires_observability: BTreeSet::new(),
            cost_model: None,
            isolation: Isolation::Kernel,
            side_effects: BTreeSet::new(),
            profile_ref: None,
            calibration_ref: None,
            charged_to: ChargedTo::Subject,
            assumption_debt: None,
        }
    }

    #[test]
    fn declare_requires_evidence_inputs() {
        let mut d = decl(ValidatorKind::Schema);
        d.evidence_inputs.clear();
        assert!(matches!(
            declare(&d),
            Err(ValidatorError::IncompleteDeclaration { .. })
        ));
    }

    #[test]
    fn judge_requires_profile_debt_and_no_determinism() {
        let judge = ValidatorKind::Judge(Box::new(hh_hir::kinds::JudgeProfile {
            rubric: hh_hir::leaves::Text::new(
                "rubric",
                "test",
                ProvenanceRecord::minted(
                    Origin::kernel("hir/kernel/check"),
                    PersistenceScope::Definition,
                    1,
                ),
            ),
            profile: hh_hir::refs::ProfileRef::unbound(),
            calibration_ref: None,
            charged_to: hh_hir::kinds::ChargedTo::Instrument,
        }));
        let mut d = decl(judge);
        d.deterministic = false;
        // missing observability/profile/debt ⇒ IncompleteDeclaration
        assert!(matches!(
            declare(&d),
            Err(ValidatorError::IncompleteDeclaration { .. })
        ));
        d.profile_ref = Some("profile:j".into());
        d.assumption_debt = Some("debt:j".into());
        d.requires_observability.insert(Observability::ModelIo);
        declare(&d).unwrap();
    }

    #[test]
    fn bind_refuses_effect_bearing_validator() {
        // AC-R-2.7.1-2: a non-read-only side_effect ⇒ ValidatorHasEffects.
        let mut d = decl(ValidatorKind::Schema);
        d.side_effects.insert(EffectDomain::FsWrite);
        assert!(matches!(
            bind(&d),
            Err(ValidatorError::ValidatorHasEffects { .. })
        ));
        let mut d = decl(ValidatorKind::Schema);
        d.side_effects.insert(EffectDomain::FsRead);
        bind(&d).unwrap();
    }

    #[test]
    fn verdict_authority_follows_detector() {
        assert_eq!(
            Verdict::expected_authority(Detector::Deterministic),
            AuthorityClass::Kernel
        );
        assert_eq!(
            Verdict::expected_authority(Detector::Judged),
            AuthorityClass::Delegate
        );
        assert_eq!(
            Verdict::expected_authority(Detector::Human),
            AuthorityClass::Principal
        );
    }

    fn cap(domain: Option<EffectDomain>) -> TerminalCapture {
        TerminalCapture {
            effect_id: "e:1".into(),
            domain,
            output: Json::obj([("ok", Json::Bool(true))]),
            output_schema: None,
            exit_status: None,
            timed_out: false,
            signalled: false,
            patch_status: None,
            touched_paths: vec![],
            resource_keys: vec![],
            diff_files: None,
            diff_lines: None,
            diff_bytes: None,
            outside_scope: vec![],
        }
    }

    #[test]
    fn check_a_schema_conformance() {
        let mut c = cap(None);
        c.output_schema = Some(Json::obj([
            ("type", Json::str("object")),
            ("required", Json::Arr(vec![Json::str("ok")])),
            (
                "properties",
                Json::obj([("ok", Json::obj([("type", Json::str("boolean"))]))]),
            ),
        ]));
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].status, VerdictStatus::Decided);
        assert_eq!(vs[0].value, VerdictValue::Bool(true));
        assert_eq!(vs[0].detector, Detector::Deterministic);
        assert_eq!(vs[0].charged_to, ChargedTo::Subject);

        // A violation ⇒ fail.
        c.output = Json::obj([("other", Json::Bool(true))]);
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));

        // An unchecked keyword ⇒ inconclusive, never a guess (T-LCD-15).
        c.output = Json::obj([("ok", Json::Bool(true))]);
        c.output_schema = Some(Json::obj([("patternProperties", Json::obj([]))]));
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert!(matches!(
            vs[0].status,
            VerdictStatus::Inconclusive(InconclusiveReason::UncheckedKeyword)
        ));
    }

    #[test]
    fn check_b_exit_status_class() {
        let mut c = cap(Some(EffectDomain::Exec));
        c.exit_status = Some(0);
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(true));

        c.exit_status = Some(2);
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));

        c.exit_status = None;
        c.timed_out = true;
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));

        // A non-exec domain does not trigger the check.
        let c = cap(Some(EffectDomain::FsRead));
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert!(vs.is_empty());
    }

    #[test]
    fn check_c_patch_application_and_touched_paths() {
        let mut c = cap(Some(EffectDomain::FsWrite));
        c.patch_status = Some(PatchStatus::Applied);
        c.touched_paths = vec!["src/a.rs".into()];
        c.resource_keys = vec!["src/a.rs".into()];
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(true));

        // A path outside resource_keys ⇒ fail with a finding.
        c.touched_paths = vec!["etc/passwd".into()];
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));
        assert!(vs[0]
            .findings
            .iter()
            .any(|f| f.code == "path_outside_resource_keys"));

        // Rejected hunks ⇒ fail.
        c.touched_paths = vec!["src/a.rs".into()];
        c.patch_status = Some(PatchStatus::Rejected(2));
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));
    }

    #[test]
    fn local_verdicts_are_deterministic() {
        // AC-R-2.7.1-4: same capture ⇒ same value + inputs_digest.
        let mut c = cap(Some(EffectDomain::Exec));
        c.exit_status = Some(0);
        let a = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        let b = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert_eq!(a[0].value, b[0].value);
        assert_eq!(a[0].inputs_digest, b[0].inputs_digest);
    }

    #[test]
    fn the_conformance_subset_is_the_oq219_interim_set() {
        // CC7: the kernel check's admitted keywords ARE the compiler's E3
        // subset (ADR-0212) — a drift between the two fails here.
        assert_eq!(
            CONFORMANCE_ADMITTED_KEYWORDS,
            hh_compiler::equiv::E3_ADMITTED_KEYWORDS
        );
    }

    // ── S2.11 — check (d) `diff_sanity` (conditioned rule) ─────────────────

    fn diff_rule() -> DiffSanityRule {
        DiffSanityRule::kernel_default(&ProvenanceRecord::kernel("hir/kernel/diff_sanity", 0))
    }

    #[test]
    fn check_d_diff_sanity_bounds_and_scope() {
        let rule = diff_rule();
        // Under-bound + in-scope ⇒ decided pass.
        let mut c = cap(Some(EffectDomain::FsWrite));
        c.diff_files = Some(2);
        let vs = run_local_checks(&c, &validator_ref(), Some(&rule), kernel_prov(), 9, 9);
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].verdict_id, "verdict:diff_sanity:e:1");
        assert_eq!(vs[0].value, VerdictValue::Bool(true));
        assert_eq!(vs[0].validator_ref.version_id, "hir/kernel/diff_sanity@1");

        // Over the file bound ⇒ fail with the typed finding.
        c.diff_files = Some(rule.thresholds.max_files + 1);
        let vs = run_local_checks(&c, &validator_ref(), Some(&rule), kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));
        assert!(vs[0]
            .findings
            .iter()
            .any(|f| f.code == "diff_files_over_bound"));

        // A path outside the sealed Permission scope ⇒ fail (the ADR-0111
        // D1(d) second half).
        c.diff_files = Some(1);
        c.outside_scope = vec!["/etc/passwd".into()];
        let vs = run_local_checks(&c, &validator_ref(), Some(&rule), kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(false));
        assert!(vs[0]
            .findings
            .iter()
            .any(|f| f.code == "path_outside_permission_scope"));

        // Ablated ⇒ no verdict (T-LCD-02).
        let vs = run_local_checks(&c, &validator_ref(), None, kernel_prov(), 9, 9);
        assert!(vs.is_empty());
    }

    #[test]
    fn check_d_undecidable_members_are_skipped_never_guessed() {
        // `diff_lines`/`diff_bytes` unknown ⇒ those bounds don't fire; the
        // verdict is still decided on the knowable members.
        let rule = diff_rule();
        let mut c = cap(Some(EffectDomain::FsWrite));
        c.diff_files = Some(3);
        c.diff_lines = None;
        c.diff_bytes = None;
        let vs = run_local_checks(&c, &validator_ref(), Some(&rule), kernel_prov(), 9, 9);
        assert_eq!(vs[0].value, VerdictValue::Bool(true));
    }

    #[test]
    fn check_d_conditioned_rule_carries_complete_debt() {
        // AC-R-2.7.1-12: the conditioned rule's assumption-debt record is
        // complete and self-identifying (rule_id == the rule's coordinate).
        let rule = diff_rule();
        let (rule_id, debt) = rule.conditioned_entry();
        assert_eq!(rule_id, "hir/kernel/diff_sanity");
        assert_eq!(debt.rule_id, rule_id);
        assert_eq!(debt.status, hh_hir::DebtStatus::Active);
        assert!(debt.removal_test.is_some());
        assert!(!debt.removal_test_ref.is_empty());
    }

    #[test]
    fn check_d_declared_postcondition_resolves_builtin() {
        // A `postconditions[]` ref to `hir/kernel/diff_sanity` binds and runs
        // under the conditioned rule; an unknown validator ref yields an
        // `inconclusive` verdict naming it — never a silent skip.
        let rule = diff_rule();
        let mut c = cap(Some(EffectDomain::FsWrite));
        c.diff_files = Some(1);
        let kref = hh_hir::refs::Ref {
            semantic_id: "hir/kernel/diff_sanity".into(),
            version: hh_hir::refs::RefVersion::Pinned("hir/kernel/diff_sanity@1".into()),
        };
        let v = run_declared_postcondition(&kref, &c, Some(&rule), kernel_prov(), 9, 9)
            .expect("resolved");
        assert_eq!(v.validator_ref.version_id, "hir/kernel/diff_sanity@1");
        assert_eq!(v.value, VerdictValue::Bool(true));

        let other = hh_hir::refs::Ref {
            semantic_id: "ext/custom_judge".into(),
            version: hh_hir::refs::RefVersion::Pinned("ext/custom_judge@1".into()),
        };
        let v = run_declared_postcondition(&other, &c, Some(&rule), kernel_prov(), 9, 9)
            .expect("inconclusive verdict");
        assert!(matches!(
            v.status,
            VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence)
        ));
        assert!(v.findings.iter().any(|f| f.code == "unbound_validator"));
    }

    #[test]
    fn check_d_procedure_verify_steps_run_in_order() {
        let rule = diff_rule();
        let mut c = cap(Some(EffectDomain::FsWrite));
        c.diff_files = Some(1);
        let steps = vec![
            hh_hir::records::ProcedureStep::Instruction(hh_hir::leaves::Text::new(
                "step",
                "test",
                ProvenanceRecord::minted(
                    Origin::kernel("hir/kernel/check"),
                    PersistenceScope::Definition,
                    1,
                ),
            )),
            hh_hir::records::ProcedureStep::Verify {
                validator: hh_hir::refs::Ref {
                    semantic_id: "hir/kernel/schema_conformance".into(),
                    version: hh_hir::refs::RefVersion::Pinned(
                        "hir/kernel/schema_conformance@1".into(),
                    ),
                },
            },
            hh_hir::records::ProcedureStep::Verify {
                validator: hh_hir::refs::Ref {
                    semantic_id: "hir/kernel/diff_sanity".into(),
                    version: hh_hir::refs::RefVersion::Pinned("hir/kernel/diff_sanity@1".into()),
                },
            },
        ];
        // `schema_conformance` needs a schema — absent ⇒ no verdict; the
        // `diff_sanity` Verify still lands (exactly the applicable).
        let vs = run_verify_steps(&steps, &c, Some(&rule), kernel_prov(), 9, 9);
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].validator_ref.version_id, "hir/kernel/diff_sanity@1");
    }

    #[test]
    fn inconclusive_and_oracle_failure_are_never_passes() {
        let v = oracle_failure(
            "e:1",
            &validator_ref(),
            OracleCause::Timeout,
            kernel_prov(),
            9,
        );
        assert!(!v.is_pass());
        assert!(matches!(
            v.status,
            VerdictStatus::OracleFailure(OracleCause::Timeout)
        ));
    }
}
