//! `R-2.3.3` — the profile **test contract** (§5b.3; ADR-0125; ticket S3.7).
//!
//! Three tests, three owners, one report. This module is the compiler-owned
//! half: `test_profile(profile, fixtures, null_profile) → ProfileTestReport`
//! (validity V1–V9, runnable with no runtime and no model), the `ProbeSpec` /
//! `ConformanceRecord` schema and the C0 probe-verdict evaluation the
//! gateway's `probe` results feed (`probe_profile` — every probe call is a
//! normal `model.call.*` charged `instrument`; `unknown` is never coerced),
//! and the link gate inputs (`profile_untested` / `profile_invalid` /
//! `capability_drift` — see `crate::link`).
//!
//! ## V1–V9 (ADR-0125 d.1/d.2)
//!
//! * **V1** schema + per-kind constraints: required members populated, rule
//!   ids unique, `params` objects, `tests` an object.
//! * **V2** owned-field discipline against the **null profile**:
//!   `member_diff(null, profile) ⊆ owned_fields(profile)`.
//! * **V3** debt completeness: every rule's `debt` and the profile's `expiry`
//!   record satisfy [`ProfileDebtRecord::is_complete`].
//! * **V4** capability consistency: no rule assumes an `unsupported`
//!   capability in its declared dependency set
//!   ([`capability_dependencies`]); `*_required` capabilities *derive*
//!   constraints ([`DerivedConstraint`]) — they are never *ruled*.
//! * **V5** golden surfaces: every compiled tool surface / layout / renderer
//!   spec equals the content address recorded at publish (the fixtures carry
//!   both sides; the comparison is the test).
//! * **V6** determinism: ≥ 2 witness content addresses from separate
//!   compiles, all equal.
//! * **V7** equivalence linkage: every `tool_shape` surface carries
//!   `EquivalenceEvidence` with E1/E2/E3/E7 `pass` (E4 `pass` or
//!   `n/a{open-world}` — ADR-0022).
//! * **V8** error-format E5: the `error_format` rule's renderings are
//!   surjective over `SurfaceFailure::ALL` ∪ the declared capability
//!   `error_classes`, and pairwise distinguishable.
//! * **V9** role placement: `prompt_layout`'s `role_map` is total and
//!   monotone over the seven `AuthorityClass`es, `slots` cover the six
//!   kernel-reserved slot ids, and no demotion wrapper promotes (a wrapper
//!   targeting a section *earlier* than the class's own placement is
//!   refused). A profile with no `prompt_layout` rule passes vacuously — the
//!   kernel default layout satisfies placement by construction.
//!
//! ## The link gate (AC-R-2.3.3-13)
//!
//! `link()` consults `ProfileView::test_report` for every bound non-null
//! profile: no report ⇒ `LinkError{profile_untested}`; a failing validity
//! section ⇒ `LinkError{profile_invalid}`; a `DRIFT` conformance record on a
//! capability in a bound rule's dependency set ⇒ `LinkError{capability_drift}`
//! unless a recorded intent is present (ADR-0125 d.3 DRIFT policy). The
//! kernel-defined null profile is exempt — compiling under it is the
//! compiler's own removal test (AC-R-2.3.3-14), so its validity *is* this
//! suite.

use std::collections::BTreeMap;

use hh_identity::idp::idp_id;
use hh_provenance::authority::AuthorityClass;
use hh_wire::json::Json;

use crate::equiv::{EquivalenceEvidence, EvidenceVerdict};
use crate::profile::{
    member_diff, null_profile, owned_fields, profile_coordinate, CapabilityState, ModelProfile,
    ProfileRuleKind,
};
use crate::surface::SurfaceFailure;

/// `ProfileTestReport/1` — the schema version the report spells.
pub const PROFILE_TEST_REPORT_SCHEMA: &str = "ProfileTestReport/1";

/// `profile_test/1` — the `idp` domain report refs mint under.
pub const PROFILE_TEST_IDP: &str = "profile_test.1";

/// The producer version this implementation stamps (`producer_version`).
pub const PRODUCER_VERSION: &str = "hh-compiler/S3.7";

// ─── ProbeSpec / ConformanceRecord (ADR-0125 d.3) ────────────────────────────

/// The closed `ProbeSpec.kind` sum (§5b.3). C0/Stage 3 implements
/// `schema_accept`, `name_roundtrip`, `id_roundtrip`; the rest are declared
/// for closure (C1/Stage 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeKind {
    /// The provider accepts the profile's strict-schema dialect.
    SchemaAccept,
    /// A tool emitted under the profile's naming scheme round-trips by name.
    NameRoundtrip,
    /// A `tool_call_id` echoes back unchanged.
    IdRoundtrip,
    /// Reasoning signature replay (C1).
    SignatureReplay,
    /// Parallel tool calls (C1).
    ParallelCalls,
    /// The `developer` role (C1).
    DeveloperRole,
    /// Image input (C1).
    ImageInput,
    /// Context-window edge (C1).
    ContextWindowEdge,
    /// Max-output edge (C1).
    MaxOutputEdge,
    /// A `strict_keyword(kw)` probe (C1) — the keyword rides in `params`.
    StrictKeyword {
        /// The keyword under test.
        keyword: String,
    },
    /// Cache-marker acceptance (C1).
    CacheMarkerAccept,
    /// `seed_repeat` — the distributional fingerprint (C1; ADR-0120).
    SeedRepeat,
}

impl ProbeKind {
    /// The canonical spelling.
    pub fn name(&self) -> &'static str {
        match self {
            ProbeKind::SchemaAccept => "schema_accept",
            ProbeKind::NameRoundtrip => "name_roundtrip",
            ProbeKind::IdRoundtrip => "id_roundtrip",
            ProbeKind::SignatureReplay => "signature_replay",
            ProbeKind::ParallelCalls => "parallel_calls",
            ProbeKind::DeveloperRole => "developer_role",
            ProbeKind::ImageInput => "image_input",
            ProbeKind::ContextWindowEdge => "context_window_edge",
            ProbeKind::MaxOutputEdge => "max_output_edge",
            ProbeKind::StrictKeyword { .. } => "strict_keyword",
            ProbeKind::CacheMarkerAccept => "cache_marker_accept",
            ProbeKind::SeedRepeat => "seed_repeat",
        }
    }

    /// The C0/Stage-3 minimal probe set (AC-R-2.3.3-5).
    pub const C0_MINIMAL: &'static [ProbeKind] = &[
        ProbeKind::SchemaAccept,
        ProbeKind::NameRoundtrip,
        ProbeKind::IdRoundtrip,
    ];
}

/// `ProbeSpec{capability, kind, fixture_ref, samples, budget, verdict_rule}`
/// (§5b.3) — the profile's declared conformance test for one capability.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSpec {
    /// The capability axis under test.
    pub capability: String,
    /// The closed probe kind.
    pub kind: ProbeKind,
    /// The fixture the gateway's `probe` executes.
    pub fixture_ref: String,
    /// The sample count.
    pub samples: u32,
    /// The probe budget (`ProbeBudgetExhausted` bounds it).
    pub budget: u32,
    /// The verdict rule (the `(declared, probed) → verdict` table ref).
    pub verdict_rule: String,
}

/// The closed `ConformanceRecord.verdict` sum (§5b.3): `SUPPORTED |
/// UNSUPPORTED | PARTIAL | NOT_APPLICABLE | UNKNOWN | SKIPPED | DRIFT`.
/// `unknown` is a verdict, never a coercion target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceVerdict {
    /// Probed supported — matches a `supported`/`required` declaration.
    Supported,
    /// Probed unsupported — matches an `unsupported` declaration.
    Unsupported,
    /// Partially conforming.
    Partial,
    /// The capability does not apply to this dialect.
    NotApplicable,
    /// The probe was inconclusive — never coerced either way.
    Unknown,
    /// The probe did not run (budget, missing fixture).
    Skipped,
    /// The probe observed the opposite of the declaration.
    Drift,
}

impl ConformanceVerdict {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ConformanceVerdict::Supported => "SUPPORTED",
            ConformanceVerdict::Unsupported => "UNSUPPORTED",
            ConformanceVerdict::Partial => "PARTIAL",
            ConformanceVerdict::NotApplicable => "NOT_APPLICABLE",
            ConformanceVerdict::Unknown => "UNKNOWN",
            ConformanceVerdict::Skipped => "SKIPPED",
            ConformanceVerdict::Drift => "DRIFT",
        }
    }
}

/// The gateway-observed probe outcome the verdict rule consumes (the scripted
/// fake gateway produces exactly these — acceptance/rejection/round-trip
/// fidelity; `Inconclusive` is the `unknown` leg).
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    /// The provider accepted the probe's fixture.
    Accepted,
    /// The provider rejected the probe's fixture.
    Rejected,
    /// A round-trip probe echoed the sent value unchanged.
    RoundtripMatch,
    /// A round-trip probe returned a *different* value (the payload diff is
    /// recorded, never repaired).
    RoundtripMismatch {
        /// What was sent.
        sent: String,
        /// What came back.
        got: String,
    },
    /// The probe could not decide (timeout, ambiguous response, missing
    /// fixture) — the verdict is `UNKNOWN`, never coerced.
    Inconclusive {
        /// Why.
        reason: String,
    },
}

/// `ConformanceRecord{profile_version, capability, declared, probed,
/// verdict, probe_run_id}` (§5b.3) — the per-dimension `results[]` row of a
/// `conformance_report{subject_kind: profile}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceRecord {
    /// The profile coordinate under test.
    pub profile_version: String,
    /// The capability axis.
    pub capability: String,
    /// The declared tri-state spelling.
    pub declared: String,
    /// The probed observation spelling.
    pub probed: String,
    /// The closed verdict.
    pub verdict: ConformanceVerdict,
    /// The probe run the record belongs to.
    pub probe_run_id: String,
}

/// `evaluate_probe(spec, declared, outcome, profile_version, run)` — the C0
/// verdict rule. `declared` is the profile's tri-state (`declared`/`probed`/
/// `unknown` — C0 has no `unsupported` axis value; "unsupported" spells
/// `unknown`, §3.2.3). The mapping is total and closed:
///
/// | outcome ↓ / declared → | `declared`/`probed` | `unknown` |
/// |---|---|---|
/// | accepted / roundtrip match | `SUPPORTED` | `SUPPORTED` (observed) |
/// | rejected / roundtrip mismatch | `DRIFT` | `UNSUPPORTED` (observed) / `UNKNOWN` |
/// | inconclusive | `UNKNOWN` | `UNKNOWN` |
///
/// A probe that *rejects* what the profile declares is **DRIFT** — the DRIFT
/// policy at `link` reads this verdict (ADR-0125 d.3). An `unknown`
/// declaration takes the observation as the record's `probed` — information,
/// never a coercion of the declaration.
pub fn evaluate_probe(
    spec: &ProbeSpec,
    declared: CapabilityState,
    outcome: &ProbeOutcome,
    profile_version: &str,
    probe_run_id: &str,
) -> ConformanceRecord {
    let established = matches!(
        declared,
        CapabilityState::Declared | CapabilityState::Probed
    );
    let (probed, verdict) = match outcome {
        ProbeOutcome::Accepted | ProbeOutcome::RoundtripMatch => {
            ("supported", ConformanceVerdict::Supported)
        }
        ProbeOutcome::Rejected => (
            "unsupported",
            if established {
                ConformanceVerdict::Drift
            } else {
                ConformanceVerdict::Unsupported
            },
        ),
        ProbeOutcome::RoundtripMismatch { .. } => (
            "mismatch",
            if established {
                ConformanceVerdict::Drift
            } else {
                ConformanceVerdict::Partial
            },
        ),
        ProbeOutcome::Inconclusive { .. } => ("unknown", ConformanceVerdict::Unknown),
    };
    ConformanceRecord {
        profile_version: profile_version.to_string(),
        capability: spec.capability.clone(),
        declared: declared.name().to_string(),
        probed: probed.to_string(),
        verdict,
        probe_run_id: probe_run_id.to_string(),
    }
}

// ─── The capability dependency sets (ADR-0125 d.3, OQ-300 note) ─────────────

/// `capability_dependencies(kind)` — each rule kind's declared capability
/// dependency set (the set a `DRIFT`/`unsupported` verdict gates `link`
/// against). Closed, declared per kind — a rule never names a capability
/// outside its set.
pub fn capability_dependencies(kind: ProfileRuleKind) -> &'static [&'static str] {
    match kind {
        ProfileRuleKind::ToolShape => &["native_function_calling", "strict_schema_dialect"],
        ProfileRuleKind::Naming => &["native_function_calling"],
        ProfileRuleKind::SchemaDialect => &["strict_schema_dialect"],
        ProfileRuleKind::DescriptionTemplate => &[],
        ProfileRuleKind::ErrorFormat => &[],
        ProfileRuleKind::ResultRender => &["native_function_calling"],
        ProfileRuleKind::PromptLayout => &["developer_role"],
        ProfileRuleKind::InteractionMode => &["native_function_calling"],
        ProfileRuleKind::TranscriptRender => &[],
        ProfileRuleKind::CompactionReminder => &["context_window"],
        ProfileRuleKind::SamplingDefaults => &["temperature_supported"],
        ProfileRuleKind::CachingMarkers => &["cache_control_convention"],
        ProfileRuleKind::ProcedureTarget => &["native_function_calling"],
    }
}

/// A kernel-derived renderer/constraint step V4 produces from a `*_required`
/// capability (ADR-0125 d.2: `*_required` constraints are *derived, not
/// ruled* — a declared `supported` yields the step with no debt record).
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedConstraint {
    /// The capability the constraint derives from.
    pub capability: String,
    /// The derived renderer step id.
    pub derived: String,
}

/// The `*_required` capabilities V4 derives constraints from, with the
/// renderer step each yields.
pub const REQUIRED_DERIVATIONS: &[(&str, &str)] = &[
    (
        "tool_result_name_required",
        "renderer.emit_tool_result_name",
    ),
    (
        "assistant_required_after_tool_result",
        "renderer.emit_assistant_after_tool_result",
    ),
];

// ─── Validity V1–V9 ─────────────────────────────────────────────────────────

/// The validity section ids, `v1`–`v9`, in report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidityId {
    /// V1 schema + per-kind constraints.
    V1,
    /// V2 owned-field discipline vs the null profile.
    V2,
    /// V3 debt completeness.
    V3,
    /// V4 capability consistency + derived constraints.
    V4,
    /// V5 golden surfaces.
    V5,
    /// V6 determinism across compiles.
    V6,
    /// V7 equivalence linkage.
    V7,
    /// V8 error-format surjectivity + distinguishability.
    V8,
    /// V9 role placement.
    V9,
}

impl ValidityId {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ValidityId::V1 => "v1",
            ValidityId::V2 => "v2",
            ValidityId::V3 => "v3",
            ValidityId::V4 => "v4",
            ValidityId::V5 => "v5",
            ValidityId::V6 => "v6",
            ValidityId::V7 => "v7",
            ValidityId::V8 => "v8",
            ValidityId::V9 => "v9",
        }
    }

    /// `v1..v9` in order.
    pub const ALL: [ValidityId; 9] = [
        ValidityId::V1,
        ValidityId::V2,
        ValidityId::V3,
        ValidityId::V4,
        ValidityId::V5,
        ValidityId::V6,
        ValidityId::V7,
        ValidityId::V8,
        ValidityId::V9,
    ];
}

/// `pass | fail{diagnostics[]}` — one validity section's verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidityVerdict {
    /// The section holds.
    Pass,
    /// The section fails — the complete diagnostic set (never fail-fast).
    Fail {
        /// Every violation the section found.
        diagnostics: Vec<String>,
    },
}

/// `validity{V1..V9: pass | fail{diagnostics[]}}` — one report row.
#[derive(Debug, Clone, PartialEq)]
pub struct ValiditySection {
    /// The section id.
    pub id: ValidityId,
    /// The verdict.
    pub verdict: ValidityVerdict,
}

/// `golden{surfaces: map<(capability, surface_id) → ContentAddress>,
/// layout_hash, renderer_spec_hash}` (§5b.3 `ProfileTestReport`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoldenSurfaces {
    /// `(capability_semantic_id ∥ surface_id) → ContentAddress`.
    pub surfaces: BTreeMap<String, String>,
    /// The compiled layout's content address.
    pub layout_hash: String,
    /// The renderer spec's content address.
    pub renderer_spec_hash: String,
}

/// The fixtures `test_profile` consumes (§5b.3 `test_profile(profile,
/// fixtures, null_profile)` — runnable with no runtime and no model): the
/// *observed* side of V5 (a fresh lowering's content addresses), the recorded
/// publish-time addresses, V6's cross-compile witnesses, V7's equivalence
/// evidence, and the conformance probe outcomes the gateway already ran.
#[derive(Debug, Clone, Default)]
pub struct ProfileTestFixtures {
    /// Observed `(capability ∥ surface_id) → ContentAddress` from a fresh
    /// lowering.
    pub observed_surfaces: BTreeMap<String, String>,
    /// The observed layout content address.
    pub observed_layout_hash: String,
    /// The observed renderer spec content address.
    pub observed_renderer_spec_hash: String,
    /// The addresses recorded at publish (V5's expected side).
    pub recorded: GoldenSurfaces,
    /// Content addresses of the same profile compiled by ≥ 2 hosts/impls (V6).
    pub determinism_witnesses: Vec<String>,
    /// `surface_id → EquivalenceEvidence` (V7).
    pub equivalence: BTreeMap<String, EquivalenceEvidence>,
    /// The probe outcomes for the profile's declared `ProbeSpec`s, in spec
    /// order (the conformance section; the gateway charged each `instrument`).
    pub probe_outcomes: Vec<ProbeOutcome>,
    /// The probe run id the produced `ConformanceRecord`s carry.
    pub probe_run_id: String,
}

/// `ProfileTestReport` (§5b.3; ADR-0125 d.1) — the immutable registry record
/// beside the profile; `CompiledBundle.diagnostics.profile_test_report_ref`
/// carries its ref.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileTestReport {
    /// The profile under test.
    pub profile_ref: String,
    /// `profile_hash` — the profile's content hash at test time.
    pub profile_hash: String,
    /// `ProfileTestReport/1`.
    pub schema_version: String,
    /// `validity{V1..V9}` in id order.
    pub validity: Vec<ValiditySection>,
    /// The golden-surface block.
    pub golden: GoldenSurfaces,
    /// `equivalence: map<surface_id → EquivalenceEvidence>`.
    pub equivalence: BTreeMap<String, EquivalenceEvidence>,
    /// `probes: ConformanceRecord[]` — the conformance section.
    pub probes: Vec<ConformanceRecord>,
    /// `compliance: [{rule_id, metric_ref, design_ref, last_report_ref?}]` —
    /// carried as data at C0 (Lab owns the metric; §5b.3).
    pub compliance: Vec<Json>,
    /// V4's derived constraints (the `*_required` renderer steps).
    pub derived_constraints: Vec<DerivedConstraint>,
    /// `produced_at` — the logical test instant.
    pub produced_at: u64,
    /// `producer_version`.
    pub producer_version: String,
    /// The report's own content address (idp/1 over the canonical members).
    pub report_ref: String,
}

impl ProfileTestReport {
    /// `true` iff every validity section passed (the `link` gate's read).
    pub fn validity_ok(&self) -> bool {
        self.validity
            .iter()
            .all(|s| matches!(s.verdict, ValidityVerdict::Pass))
    }

    /// The `DRIFT` conformance records on `capability` — the DRIFT-policy
    /// read `link` makes (ADR-0125 d.3).
    pub fn drift_on(&self, capability: &str) -> Vec<&ConformanceRecord> {
        self.probes
            .iter()
            .filter(|r| r.capability == capability && r.verdict == ConformanceVerdict::Drift)
            .collect()
    }

    /// A fixture constructor for registry plumbing: an all-pass report under
    /// `profile_ref`/`profile_hash` with the golden block the caller records.
    /// Tests use this where the gate's *plumbing* (not V1–V9) is under test —
    /// the `test_profile` suite proves the verdicts are real.
    pub fn passing_for(profile_ref: &str, profile_hash: &str) -> ProfileTestReport {
        ProfileTestReport {
            profile_ref: profile_ref.to_string(),
            profile_hash: profile_hash.to_string(),
            schema_version: PROFILE_TEST_REPORT_SCHEMA.to_string(),
            validity: ValidityId::ALL
                .iter()
                .map(|id| ValiditySection {
                    id: *id,
                    verdict: ValidityVerdict::Pass,
                })
                .collect(),
            golden: GoldenSurfaces::default(),
            equivalence: BTreeMap::new(),
            probes: Vec::new(),
            compliance: Vec::new(),
            derived_constraints: Vec::new(),
            produced_at: 0,
            producer_version: PRODUCER_VERSION.to_string(),
            report_ref: idp_id(
                PROFILE_TEST_IDP,
                format!("{profile_ref}∥{profile_hash}").as_bytes(),
            ),
        }
    }
}

// ─── `test_profile` ─────────────────────────────────────────────────────────

fn section(id: ValidityId, diagnostics: Vec<String>) -> ValiditySection {
    ValiditySection {
        id,
        verdict: if diagnostics.is_empty() {
            ValidityVerdict::Pass
        } else {
            ValidityVerdict::Fail { diagnostics }
        },
    }
}

/// `test_profile(profile, fixtures, null_profile) → ProfileTestReport`
/// (§5b.3; ADR-0125 d.1/d.2) — the compiler-only validity suite V1–V9 plus
/// the conformance records the gateway's probes produced. Compiler-only: no
/// runtime, no model — every input arrives in `fixtures`.
pub fn test_profile(
    profile: &ModelProfile,
    fixtures: &ProfileTestFixtures,
    produced_at: u64,
) -> ProfileTestReport {
    let null = null_profile();
    let coord = profile_coordinate(profile);
    let mut derived_constraints = Vec::new();

    // V1 — schema + per-kind constraints.
    let mut d = Vec::new();
    if profile.profile_id.is_empty() {
        d.push("profile_id is empty".to_string());
    }
    if profile.version.is_empty() {
        d.push("version is empty".to_string());
    }
    if profile.content_hash.is_empty() {
        d.push("content_hash is empty".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in &profile.rules {
        if r.rule_id.is_empty() {
            d.push("a rule carries an empty rule_id".to_string());
        }
        if !seen.insert(r.rule_id.clone()) {
            d.push(format!("duplicate rule_id {}", r.rule_id));
        }
        if !matches!(r.params, Json::Obj(_) | Json::Null) {
            d.push(format!("rule {} params is not an object", r.rule_id));
        }
        // Per-kind: `interaction_mode` admits `native_fc` only at C0.
        if r.kind == ProfileRuleKind::InteractionMode {
            for member in ["mode", "fallback"] {
                if let Some(m) = r.params.get(member).and_then(Json::as_str) {
                    if m != "native_fc" {
                        d.push(format!(
                            "rule {} interaction_mode.{member} = {m} — C0 admits `native_fc` only",
                            r.rule_id
                        ));
                    }
                }
            }
        }
    }
    if !matches!(profile.tests, Json::Obj(_) | Json::Null) {
        d.push("tests is not an object".to_string());
    }
    let v1 = section(ValidityId::V1, d);

    // V2 — owned-field discipline vs the null profile: `diff ⊆ owned_fields`.
    let owned = owned_fields(profile);
    let mut d: Vec<String> = member_diff(&null, profile)
        .into_iter()
        .filter(|m| !owned.contains(m))
        .map(|m| format!("member /{m} differs from null profile but is not rule-owned"))
        .collect();
    d.sort();
    let v2 = section(ValidityId::V2, d);

    // V3 — debt completeness on every rule and the profile itself.
    let mut d = Vec::new();
    if !profile.expiry.is_complete() {
        d.push("profile expiry debt record is incomplete".to_string());
    }
    for r in &profile.rules {
        if !r.debt.is_complete() {
            d.push(format!("rule {} debt record is incomplete", r.rule_id));
        }
    }
    for (k, ext) in &profile.ext {
        if !ext.debt.is_complete() {
            d.push(format!("ext block {k} debt record is incomplete"));
        }
    }
    let v3 = section(ValidityId::V3, d);

    // V4 — capability consistency + derived constraints. A rule *assuming* a
    // capability requires it established (`declared`/`probed`, or a measured
    // bound present for `context_window`/`max_output`); C0 spells
    // "unsupported" as `unknown` (§3.2.3), and an assumption on an
    // unestablished axis is the capability_contradiction V4 refuses.
    let mut d = Vec::new();
    for r in &profile.rules {
        for dep in capability_dependencies(r.kind) {
            let established = match profile.capabilities.capability_state(dep) {
                Some(CapabilityState::Declared | CapabilityState::Probed) => true,
                Some(CapabilityState::Unknown) => false,
                None => match *dep {
                    "context_window" => profile.capabilities.context_window.is_some(),
                    "max_output" => profile.capabilities.max_output.is_some(),
                    _ => false,
                },
            };
            if !established {
                d.push(format!(
                    "rule {} ({}) assumes capability {dep} which is not established (unsupported at C0 spells `unknown`)",
                    r.rule_id,
                    r.kind.name()
                ));
            }
        }
    }
    for (cap, step) in REQUIRED_DERIVATIONS {
        if matches!(
            profile.capabilities.capability_state(cap),
            Some(CapabilityState::Declared | CapabilityState::Probed)
        ) {
            derived_constraints.push(DerivedConstraint {
                capability: (*cap).to_string(),
                derived: (*step).to_string(),
            });
        }
    }
    let v4 = section(ValidityId::V4, d);

    // V5 — golden surfaces: observed must equal the recorded publish
    // addresses, memberwise.
    let mut d = Vec::new();
    for (k, recorded) in &fixtures.recorded.surfaces {
        match fixtures.observed_surfaces.get(k) {
            None => d.push(format!("golden surface {k} has no observed address")),
            Some(observed) if observed != recorded => d.push(format!(
                "golden surface {k} drifted: recorded {recorded} ≠ observed {observed}"
            )),
            _ => {}
        }
    }
    for k in fixtures.observed_surfaces.keys() {
        if !fixtures.recorded.surfaces.contains_key(k) {
            d.push(format!(
                "observed surface {k} has no recorded golden address"
            ));
        }
    }
    if fixtures.observed_layout_hash != fixtures.recorded.layout_hash {
        d.push("layout_hash differs from the recorded golden".to_string());
    }
    if fixtures.observed_renderer_spec_hash != fixtures.recorded.renderer_spec_hash {
        d.push("renderer_spec_hash differs from the recorded golden".to_string());
    }
    let v5 = section(ValidityId::V5, d);

    // V6 — determinism: ≥ 2 witnesses, all equal.
    let mut d = Vec::new();
    if fixtures.determinism_witnesses.len() < 2 {
        d.push("fewer than two compile witnesses — determinism unproven".to_string());
    } else if fixtures
        .determinism_witnesses
        .iter()
        .any(|w| w != &fixtures.determinism_witnesses[0])
    {
        d.push("compile witnesses differ — lowering is not deterministic".to_string());
    }
    let v6 = section(ValidityId::V6, d);

    // V7 — equivalence linkage: every `tool_shape` surface carries
    // E1/E2/E3/E7 pass (E4 pass-or-`n/a{open-world}`).
    let mut d = Vec::new();
    for r in &profile.rules {
        if r.kind != ProfileRuleKind::ToolShape {
            continue;
        }
        if let Json::Obj(m) = &r.params {
            for (capability_class, family_ref) in m {
                let surface_id = family_ref
                    .get("variant_id")
                    .or_else(|| family_ref.get("family_id"))
                    .and_then(Json::as_str)
                    .unwrap_or(capability_class.as_str());
                match fixtures.equivalence.get(surface_id) {
                    None => d.push(format!(
                        "tool_shape surface {surface_id} carries no equivalence evidence"
                    )),
                    Some(e) => {
                        for (name, v) in [
                            ("e1", &e.e1_effect_equality),
                            ("e2", &e.e2_authority),
                            ("e3", &e.e3_precondition_domain),
                            ("e7", &e.e7_accounting_identity),
                        ] {
                            if !matches!(v, EvidenceVerdict::Pass) {
                                d.push(format!(
                                    "tool_shape surface {surface_id} equivalence {name} = {v:?}"
                                ));
                            }
                        }
                        if let EvidenceVerdict::Fail { reason } = &e.e4_differential {
                            d.push(format!(
                                "tool_shape surface {surface_id} equivalence e4 fail: {reason}"
                            ));
                        }
                    }
                }
            }
        }
    }
    let v7 = section(ValidityId::V7, d);

    // V8 — error-format E5: surjectivity over `SurfaceFailure ∪ error_classes`,
    // pairwise distinguishable renderings.
    let mut d = Vec::new();
    for r in &profile.rules {
        if r.kind != ProfileRuleKind::ErrorFormat {
            continue;
        }
        let renderings = match r.params.get("renderings") {
            Some(Json::Obj(m)) => m,
            _ => {
                d.push(format!(
                    "error_format rule {} carries no renderings map",
                    r.rule_id
                ));
                continue;
            }
        };
        for f in SurfaceFailure::ALL {
            if !renderings.contains_key(f.as_str()) {
                d.push(format!(
                    "error_format rule {} renders no {}",
                    r.rule_id,
                    f.as_str()
                ));
            }
        }
        if let Some(Json::Arr(classes)) = r.params.get("error_classes") {
            for c in classes.iter().filter_map(|c| c.as_str()) {
                if !renderings.contains_key(c) {
                    d.push(format!(
                        "error_format rule {} renders no capability error_class {c}",
                        r.rule_id
                    ));
                }
            }
        }
        for k in renderings.keys() {
            if SurfaceFailure::parse(k).is_none() {
                if let Some(Json::Arr(classes)) = r.params.get("error_classes") {
                    if !classes.iter().any(|c| c.as_str() == Some(k.as_str())) {
                        d.push(format!(
                            "error_format rule {} renders unknown failure class {k}",
                            r.rule_id
                        ));
                    }
                } else {
                    d.push(format!(
                        "error_format rule {} renders unknown failure class {k}",
                        r.rule_id
                    ));
                }
            }
        }
        let mut texts: Vec<String> = Vec::new();
        for v in renderings.values() {
            let t = match v {
                Json::Str(s) => s.clone(),
                Json::Obj(o) => o
                    .get("content")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                _ => String::new(),
            };
            if texts.contains(&t) {
                d.push(format!(
                    "error_format rule {} renderings are not pairwise distinguishable",
                    r.rule_id
                ));
            }
            texts.push(t);
        }
    }
    let v8 = section(ValidityId::V8, d);

    // V9 — role placement: `role_map` total + monotone over the seven
    // authority classes; the six kernel-reserved slots present in `slots`.
    let mut d = Vec::new();
    for r in &profile.rules {
        if r.kind != ProfileRuleKind::PromptLayout {
            continue;
        }
        let params = &r.params;
        let role_map: BTreeMap<String, String> = match params.get("role_map") {
            Some(Json::Obj(m)) => m
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect(),
            _ => BTreeMap::new(),
        };
        // Totality: every authority class maps somewhere.
        for class in AuthorityClass::ALL {
            if !role_map.contains_key(class.as_str()) {
                d.push(format!(
                    "prompt_layout rule {} role_map is not total — missing {}",
                    r.rule_id,
                    class.as_str()
                ));
            }
        }
        // Monotonicity: walking `order[]`, the privilege rank of the classes
        // placed into each section must be non-decreasing — a
        // higher-authority class may never land in an earlier (lower) section
        // than a lower-authority class's placement.
        let order: Vec<String> = match params.get("order") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        let rank = |section: &str| -> Option<usize> { order.iter().position(|s| s == section) };
        for (i, lo) in AuthorityClass::ALL.iter().enumerate() {
            for hi in AuthorityClass::ALL.iter().skip(i + 1) {
                if let (Some(ls), Some(hs)) = (role_map.get(lo.as_str()), role_map.get(hi.as_str()))
                {
                    if let (Some(lr), Some(hr)) = (rank(ls), rank(hs)) {
                        if lr > hr {
                            d.push(format!(
                                "prompt_layout rule {} role_map is not monotone — {} (rank {}) lands after {} (rank {})",
                                r.rule_id,
                                hi.as_str(),
                                hr,
                                lo.as_str(),
                                lr
                            ));
                        }
                    }
                }
            }
        }
        // The six kernel-reserved slots must be present in `slots`.
        if let Some(Json::Obj(slots)) = params.get("slots") {
            for slot in hh_context_slot_ids() {
                if !slots.contains_key(*slot) {
                    d.push(format!(
                        "prompt_layout rule {} slots misses reserved slot {slot}",
                        r.rule_id
                    ));
                }
            }
        }
        // Wrappers never promote: a `demotion_wrappers` entry's target must
        // not rank above the class's own placement.
        if let Some(Json::Obj(wrappers)) = params.get("demotion_wrappers") {
            for (class, w) in wrappers {
                let target = w.get("into").and_then(Json::as_str).unwrap_or("");
                let own = role_map
                    .get(class.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if let (Some(tr), Some(orank)) = (rank(target), rank(own)) {
                    if tr > orank {
                        d.push(format!(
                            "prompt_layout rule {} demotion wrapper for {class} promotes into {target}",
                            r.rule_id
                        ));
                    }
                }
            }
        }
    }
    let v9 = section(ValidityId::V9, d);

    // The conformance section — probe outcomes the gateway already ran.
    let probes: Vec<ConformanceRecord> = profile
        .tests
        .get("probes")
        .and_then(|p| match p {
            Json::Arr(items) => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .zip(fixtures.probe_outcomes.iter())
        .filter_map(|(spec_j, outcome)| {
            let spec = probe_spec_from_json(spec_j)?;
            let declared = profile
                .capabilities
                .capability_state(&spec.capability)
                .unwrap_or(CapabilityState::Unknown);
            Some(evaluate_probe(
                &spec,
                declared,
                outcome,
                &coord,
                &fixtures.probe_run_id,
            ))
        })
        .collect();

    let golden = GoldenSurfaces {
        surfaces: fixtures.observed_surfaces.clone(),
        layout_hash: fixtures.observed_layout_hash.clone(),
        renderer_spec_hash: fixtures.observed_renderer_spec_hash.clone(),
    };

    let validity = vec![v1, v2, v3, v4, v5, v6, v7, v8, v9];
    let preimage = format!(
        "{coord}∥{}∥{}∥{}∥{}∥{produced_at}",
        profile.content_hash,
        validity
            .iter()
            .map(|s| match &s.verdict {
                ValidityVerdict::Pass => format!("{}:pass", s.id.name()),
                ValidityVerdict::Fail { .. } => format!("{}:fail", s.id.name()),
            })
            .collect::<Vec<_>>()
            .join(","),
        probes.len(),
        fixtures
            .determinism_witnesses
            .first()
            .cloned()
            .unwrap_or_default()
    );
    ProfileTestReport {
        profile_ref: coord,
        profile_hash: profile.content_hash.clone(),
        schema_version: PROFILE_TEST_REPORT_SCHEMA.to_string(),
        validity,
        golden,
        equivalence: fixtures.equivalence.clone(),
        probes,
        compliance: Vec::new(),
        derived_constraints,
        produced_at,
        producer_version: PRODUCER_VERSION.to_string(),
        report_ref: idp_id(PROFILE_TEST_IDP, preimage.as_bytes()),
    }
}

/// The six kernel-reserved slot ids V9 checks (`hh_context` is not a compiler
/// dependency — the ids are the spec's closed list, spelled once here; CC7's
/// one-source rule is honoured by the D1 side emitting the same list).
fn hh_context_slot_ids() -> &'static [&'static str] {
    &[
        "kernel",
        "definition",
        "principal",
        "transcript",
        "external",
        "unverified",
    ]
}

/// Decode a `ProbeSpec` from its `tests.probes[]` member (canonical JSON).
/// Unknown kinds/refs decode to `None` — the conformance section then simply
/// carries no record (a missing spec is never coerced to a passing probe).
pub fn probe_spec_from_json(j: &Json) -> Option<ProbeSpec> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return None,
    };
    let kind = match m.get("kind").and_then(Json::as_str)? {
        "schema_accept" => ProbeKind::SchemaAccept,
        "name_roundtrip" => ProbeKind::NameRoundtrip,
        "id_roundtrip" => ProbeKind::IdRoundtrip,
        "signature_replay" => ProbeKind::SignatureReplay,
        "parallel_calls" => ProbeKind::ParallelCalls,
        "developer_role" => ProbeKind::DeveloperRole,
        "image_input" => ProbeKind::ImageInput,
        "context_window_edge" => ProbeKind::ContextWindowEdge,
        "max_output_edge" => ProbeKind::MaxOutputEdge,
        "strict_keyword" => ProbeKind::StrictKeyword {
            keyword: m.get("keyword").and_then(Json::as_str)?.to_string(),
        },
        "cache_marker_accept" => ProbeKind::CacheMarkerAccept,
        "seed_repeat" => ProbeKind::SeedRepeat,
        _ => return None,
    };
    Some(ProbeSpec {
        capability: m.get("capability").and_then(Json::as_str)?.to_string(),
        kind,
        fixture_ref: m.get("fixture_ref").and_then(Json::as_str)?.to_string(),
        samples: m.get("samples").and_then(Json::as_int).unwrap_or(1).max(0) as u32,
        budget: m.get("budget").and_then(Json::as_int).unwrap_or(1).max(0) as u32,
        verdict_rule: m
            .get("verdict_rule")
            .and_then(Json::as_str)
            .unwrap_or("default")
            .to_string(),
    })
}

// ─── Canonical codec (the out-of-process seam carries reports — AC-CP-11) ────

fn verdict_to_json(v: &EvidenceVerdict) -> Json {
    match v {
        EvidenceVerdict::Pass => Json::str("pass"),
        EvidenceVerdict::Fail { reason } => Json::obj([("fail", Json::str(reason.clone()))]),
        EvidenceVerdict::NotApplicable { reason } => Json::obj([("na", Json::str(reason.clone()))]),
    }
}

fn verdict_from_json(j: &Json) -> Option<EvidenceVerdict> {
    match j {
        Json::Str(s) if s == "pass" => Some(EvidenceVerdict::Pass),
        Json::Obj(o) => {
            if let Some(Json::Str(r)) = o.get("fail") {
                return Some(EvidenceVerdict::fail(r.clone()));
            }
            if let Some(Json::Str(r)) = o.get("na") {
                return Some(EvidenceVerdict::na(r.clone()));
            }
            None
        }
        _ => None,
    }
}

fn evidence_to_json(e: &EquivalenceEvidence) -> Json {
    Json::obj([
        ("capability", Json::str(e.capability.clone())),
        ("e1", verdict_to_json(&e.e1_effect_equality)),
        ("e2", verdict_to_json(&e.e2_authority)),
        ("e3", verdict_to_json(&e.e3_precondition_domain)),
        ("e4", verdict_to_json(&e.e4_differential)),
        ("e5", verdict_to_json(&e.e5_error_surjectivity)),
        ("e6", verdict_to_json(&e.e6_result_observation)),
        ("e7", verdict_to_json(&e.e7_accounting_identity)),
        ("surface_name", Json::str(e.surface_name.clone())),
    ])
}

fn evidence_from_json(j: &Json) -> Option<EquivalenceEvidence> {
    Some(EquivalenceEvidence {
        capability: j.get("capability")?.as_str()?.to_string(),
        e1_effect_equality: verdict_from_json(j.get("e1")?)?,
        e2_authority: verdict_from_json(j.get("e2")?)?,
        e3_precondition_domain: verdict_from_json(j.get("e3")?)?,
        e4_differential: verdict_from_json(j.get("e4")?)?,
        e5_error_surjectivity: verdict_from_json(j.get("e5")?)?,
        e6_result_observation: verdict_from_json(j.get("e6")?)?,
        e7_accounting_identity: verdict_from_json(j.get("e7")?)?,
        surface_name: j.get("surface_name")?.as_str()?.to_string(),
    })
}

fn validity_id_parse(s: &str) -> Option<ValidityId> {
    ValidityId::ALL.iter().copied().find(|v| v.name() == s)
}

fn validity_section_to_json(s: &ValiditySection) -> Json {
    let verdict = match &s.verdict {
        ValidityVerdict::Pass => Json::str("pass"),
        ValidityVerdict::Fail { diagnostics } => Json::obj([(
            "fail",
            Json::Arr(diagnostics.iter().map(Json::str).collect()),
        )]),
    };
    Json::obj([("id", Json::str(s.id.name())), ("verdict", verdict)])
}

fn validity_section_from_json(j: &Json) -> Option<ValiditySection> {
    let id = validity_id_parse(j.get("id")?.as_str()?)?;
    let verdict = match j.get("verdict")? {
        Json::Str(s) if s == "pass" => ValidityVerdict::Pass,
        Json::Obj(o) => match o.get("fail") {
            Some(Json::Arr(items)) => ValidityVerdict::Fail {
                diagnostics: items
                    .iter()
                    .filter_map(|d| d.as_str().map(str::to_string))
                    .collect(),
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(ValiditySection { id, verdict })
}

fn golden_to_json(g: &GoldenSurfaces) -> Json {
    Json::obj([
        ("layout_hash", Json::str(g.layout_hash.clone())),
        (
            "renderer_spec_hash",
            Json::str(g.renderer_spec_hash.clone()),
        ),
        (
            "surfaces",
            Json::Obj(
                g.surfaces
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
    ])
}

fn golden_from_json(j: &Json) -> Option<GoldenSurfaces> {
    let surfaces = match j.get("surfaces")? {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect::<Option<BTreeMap<_, _>>>()?,
        _ => return None,
    };
    Some(GoldenSurfaces {
        surfaces,
        layout_hash: j.get("layout_hash")?.as_str()?.to_string(),
        renderer_spec_hash: j.get("renderer_spec_hash")?.as_str()?.to_string(),
    })
}

fn conformance_to_json(r: &ConformanceRecord) -> Json {
    Json::obj([
        ("capability", Json::str(r.capability.clone())),
        ("declared", Json::str(r.declared.clone())),
        ("probed", Json::str(r.probed.clone())),
        ("probe_run_id", Json::str(r.probe_run_id.clone())),
        ("profile_version", Json::str(r.profile_version.clone())),
        ("verdict", Json::str(r.verdict.name())),
    ])
}

fn conformance_from_json(j: &Json) -> Option<ConformanceRecord> {
    let verdict = match j.get("verdict")?.as_str()? {
        "SUPPORTED" => ConformanceVerdict::Supported,
        "UNSUPPORTED" => ConformanceVerdict::Unsupported,
        "PARTIAL" => ConformanceVerdict::Partial,
        "NOT_APPLICABLE" => ConformanceVerdict::NotApplicable,
        "UNKNOWN" => ConformanceVerdict::Unknown,
        "SKIPPED" => ConformanceVerdict::Skipped,
        "DRIFT" => ConformanceVerdict::Drift,
        _ => return None,
    };
    Some(ConformanceRecord {
        capability: j.get("capability")?.as_str()?.to_string(),
        declared: j.get("declared")?.as_str()?.to_string(),
        probed: j.get("probed")?.as_str()?.to_string(),
        probe_run_id: j.get("probe_run_id")?.as_str()?.to_string(),
        profile_version: j.get("profile_version")?.as_str()?.to_string(),
        verdict,
    })
}

/// The canonical JSON of a `ProfileTestReport` (CC1 — the registry record's
/// wire form; the out-of-process compile carries it in `CompileInputs`).
pub fn test_report_json(r: &ProfileTestReport) -> Json {
    Json::obj([
        ("compliance", Json::Arr(r.compliance.to_vec())),
        (
            "derived_constraints",
            Json::Arr(
                r.derived_constraints
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("capability", Json::str(c.capability.clone())),
                            ("derived", Json::str(c.derived.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "equivalence",
            Json::Obj(
                r.equivalence
                    .iter()
                    .map(|(k, v)| (k.clone(), evidence_to_json(v)))
                    .collect(),
            ),
        ),
        ("golden", golden_to_json(&r.golden)),
        (
            "probes",
            Json::Arr(r.probes.iter().map(conformance_to_json).collect()),
        ),
        ("produced_at", Json::Int(r.produced_at as i64)),
        ("producer_version", Json::str(r.producer_version.clone())),
        ("profile_hash", Json::str(r.profile_hash.clone())),
        ("profile_ref", Json::str(r.profile_ref.clone())),
        ("report_ref", Json::str(r.report_ref.clone())),
        ("schema_version", Json::str(r.schema_version.clone())),
        (
            "validity",
            Json::Arr(r.validity.iter().map(validity_section_to_json).collect()),
        ),
    ])
}

/// Parse a `ProfileTestReport` from its canonical JSON; malformed members
/// decode to `None` (never coerced).
pub fn test_report_from_json(j: &Json) -> Option<ProfileTestReport> {
    Some(ProfileTestReport {
        profile_ref: j.get("profile_ref")?.as_str()?.to_string(),
        profile_hash: j.get("profile_hash")?.as_str()?.to_string(),
        schema_version: j.get("schema_version")?.as_str()?.to_string(),
        validity: match j.get("validity")? {
            Json::Arr(items) => items
                .iter()
                .map(validity_section_from_json)
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        },
        golden: golden_from_json(j.get("golden")?)?,
        equivalence: match j.get("equivalence")? {
            Json::Obj(m) => m
                .iter()
                .map(|(k, v)| evidence_from_json(v).map(|e| (k.clone(), e)))
                .collect::<Option<BTreeMap<_, _>>>()?,
            _ => return None,
        },
        probes: match j.get("probes")? {
            Json::Arr(items) => items
                .iter()
                .map(conformance_from_json)
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        },
        compliance: match j.get("compliance")? {
            Json::Arr(items) => items.clone(),
            _ => return None,
        },
        derived_constraints: match j.get("derived_constraints")? {
            Json::Arr(items) => items
                .iter()
                .map(|c| {
                    Some(DerivedConstraint {
                        capability: c.get("capability")?.as_str()?.to_string(),
                        derived: c.get("derived")?.as_str()?.to_string(),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        },
        produced_at: j.get("produced_at")?.as_int()? as u64,
        producer_version: j.get("producer_version")?.as_str()?.to_string(),
        report_ref: j.get("report_ref")?.as_str()?.to_string(),
    })
}
