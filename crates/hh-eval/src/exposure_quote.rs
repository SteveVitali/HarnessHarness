//! `exposure_quote` — per-stage readiness and full exposure quote
//! (evidence spec §4.5/§9.2; REQ-D-105/110/309, REQ-C-004/105/106;
//! AC-4-4/AC-9-1; methods partition M-I; R5.42).
//!
//! `build_quote` consumes the **supplied** binding inputs — every
//! value an external gated input bound by HUMAN-H8 and the stage
//! gates, never chosen or fabricated here — and yields the
//! deterministic `exposure-quote/1` packet: per-stage readiness
//! (`ready_for_gate`/`blocked`/`infeasible`), the full exposure
//! worksheet (subjects `N = 2TR ≥ 10T`, retries/regrades/
//! instruments/unknown exposure/prices/resources/full archive/
//! versions/backups/retention — quoted across both runners,
//! policies, endpoints and strata), the structurally distinct
//! `ab_subjects`/`ho_pairs` counts (300 A/B subjects are **not**
//! 300 H/O pairs), and attainable-precision diagnostics at η = .025
//! (the M-E `kl_directional_bound` — attainable widths, never power
//! or expected agreement).
//!
//! Readiness is honest: unset currency prices $0 and blocks the
//! dependent action (`funding`); unbound pins/map/backend/rights and
//! unpriced/unbounded exposure cells block only the dependent action
//! of the stage that lacks them — the packet still builds and the
//! unresolved inputs are echoed verbatim. There is no automatic
//! top-up, pooled stage capacity, guessed margin, new cloud fallback
//! or favorable cohort shopping — each is a typed refusal, never a
//! warning. Dispatch is never authorized: the best outcome is
//! `gate_pending` naming the owed gate.
//!
//! No spend, no provider/backend calls, no scientific claim —
//! engineered quote machinery only; USD stays $0.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::idp::idp_digest;
use hharness_wire::Json;

use crate::directional_kl::ETA_TOST_PPM;
use crate::parity_admission::DIMENSIONS;
use crate::population::MethodMetadata;
use crate::stats::kl_directional_bound;

/// The quote name (the S.1 `estimator` member).
pub const KIND: &str = "exposure_quote";
/// The durable payload version.
pub const VERSION: &str = "exposure-quote/1";
/// The five quoted stages (§9.2 quote row — `diagnostic` is not a
/// quoted stage).
pub const STAGES: [&str; 5] = [
    "activation",
    "pilot",
    "parity",
    "confirmation",
    "replication",
];
/// The packet-level prerequisite vocabulary (M-I item 1 —
/// treatment/cap/created→finished/oracle/detector/parity/method).
pub const PREREQUISITES: [&str; 7] = [
    "treatment",
    "caps",
    "created_to_finished",
    "oracle",
    "detectors",
    "parity",
    "method",
];
/// The binding-input vocabulary every stage declares exactly once
/// (M-I item 2 + the §9.2 tasks/endpoint/rights/backend rows).
/// `endpoint_map`/`backend`/`rights` map onto the AC-2 blocker
/// vocabulary `map`/`backend`/`rights`; the rest block as `pins`.
pub const PINS: [&str; 18] = [
    "participant",
    "policies",
    "manifest",
    "split",
    "strata",
    "model_roles",
    "requested_served",
    "oracle",
    "image",
    "runner",
    "rights",
    "isolation",
    "cold_start",
    "drift",
    "backend",
    "endpoint_map",
    "independence",
    "protocol",
];
/// The resource-worksheet component vocabulary (§9.2: compute
/// `N_subject = 2TR ≥ 10T`, then separately add pilots, parity
/// original/HH-runner attempts, retries, reproduction and all
/// instrument/regrades; price every requested/served role and
/// failure, transport attempt, cache behavior, provider
/// billed/unknown exposure, environment/image acquisition,
/// storage/egress/versions/backups and retention).
pub const COMPONENTS: [&str; 23] = [
    "subjects_first",
    "reattempts",
    "regrades",
    "instruments",
    "reproduction",
    "cancellation",
    "role_calls_requested",
    "role_calls_served",
    "failure_costs",
    "transport_attempts",
    "cache_behavior",
    "provider_billed",
    "provider_unknown_exposure",
    "environment_image",
    "cpu_core_hours",
    "ram_gib_hours",
    "wall_minutes",
    "storage_gib",
    "egress_gib",
    "archive_full",
    "versions",
    "backups",
    "retention",
];
/// The declared capability-bound vocabulary (`stage.envelope`
/// members — enforceable exposure bounds, quoted not priced).
pub const ENVELOPE_COMPONENTS: [&str; 11] = [
    "calls_per_subject",
    "input_tokens_per_subject",
    "output_tokens_per_subject",
    "wall_minutes_per_subject",
    "verifier_minutes",
    "concurrent_subjects",
    "cores_per_subject",
    "ram_gib_per_subject",
    "cpu_core_hours",
    "storage_gib",
    "wall_days",
];
/// The §9.2 per-subject caps (pilot; confirmation under the same
/// individual caps).
const PER_SUBJECT_CEILINGS: [(&str, u64); 8] = [
    ("calls_per_subject", 100),
    ("input_tokens_per_subject", 1_000_000),
    ("output_tokens_per_subject", 100_000),
    ("wall_minutes_per_subject", 30),
    ("verifier_minutes", 15),
    ("concurrent_subjects", 2),
    ("cores_per_subject", 4),
    ("ram_gib_per_subject", 8),
];
/// Pilot tranche ceilings (§9.2).
const PILOT_TRANCHE: [(&str, u64); 2] = [("cpu_core_hours", 200), ("storage_gib", 50)];
/// Confirmation tranche ceilings (§9.2: ≤300 subjects — checked on
/// the `subjects_first` resource line — plus these).
const CONFIRM_TRANCHE: [(&str, u64); 3] = [
    ("cpu_core_hours", 900),
    ("storage_gib", 200),
    ("wall_days", 14),
];
/// The pilot first-subject ceiling (§9.2).
pub const PILOT_MAX_SUBJECTS: u64 = 60;
/// The confirmation first-subject ceiling (§9.2).
pub const CONFIRM_MAX_SUBJECTS: u64 = 300;
/// The owed gate per stage (never an authorization).
const STAGE_GATES: [(&str, &str); 5] = [
    ("activation", "GATE-G12"),
    ("pilot", "GATE-G12"),
    ("parity", "GATE-G13"),
    ("confirmation", "GATE-G14"),
    ("replication", "unscheduled"),
];

const DOMAIN: &str = "exposure_quote";

/// The envelope ceilings a stage's declared quantities are checked
/// against (parity/replication quote with no allocated tranche;
/// activation is the separate one-task quote).
fn ceilings(stage: &str) -> Vec<(&'static str, u64)> {
    let mut v: Vec<(&'static str, u64)> = Vec::new();
    match stage {
        "pilot" => {
            v.extend(PER_SUBJECT_CEILINGS);
            v.extend(PILOT_TRANCHE);
        }
        "confirmation" => {
            v.extend(PER_SUBJECT_CEILINGS);
            v.extend(CONFIRM_TRANCHE);
        }
        _ => {}
    }
    v
}

/// The components a stage must quote — `subjects_first` is the
/// subject-stage count line and never appears on the pairs stage;
/// `reproduction` binds the replication stage alone.
fn required_components(stage: &str) -> Vec<&'static str> {
    COMPONENTS
        .iter()
        .copied()
        .filter(|c| match *c {
            "subjects_first" => stage != "parity",
            "reproduction" => stage == "replication",
            _ => true,
        })
        .collect()
}

// ── declared inputs (records-in; the caller binds the records) ────

/// The state of one binding input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputState {
    /// Bound to an actual record — the ref is echoed verbatim.
    Bound(String),
    /// Not yet bound — stays unresolved, never fabricated.
    Unresolved,
    /// Unknown — the owner could not even assert unbound.
    Unknown,
}

impl InputState {
    fn name(&self) -> &'static str {
        match self {
            InputState::Bound(_) => "bound",
            InputState::Unresolved => "unresolved",
            InputState::Unknown => "unknown",
        }
    }
}

/// One named binding input (`name ∈ PINS`).
#[derive(Debug, Clone, PartialEq)]
pub struct BindingInput {
    /// The pin name.
    pub name: String,
    /// Its bound/unresolved/unknown state.
    pub state: InputState,
}

/// The state of one packet prerequisite.
#[derive(Debug, Clone, PartialEq)]
pub enum PrereqStatus {
    /// Satisfied — carries the native evidence identity.
    Satisfied(String),
    /// Declared unmet.
    Unmet,
    /// Declared unknown.
    Unknown,
}

/// One named prerequisite (`name ∈ PREREQUISITES`).
#[derive(Debug, Clone, PartialEq)]
pub struct Prerequisite {
    /// The prerequisite name.
    pub name: String,
    /// Its state.
    pub status: PrereqStatus,
}

/// One priced-or-blocked resource line (`component ∈ COMPONENTS`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceLine {
    /// The component.
    pub component: String,
    /// Optional axis target — a runner/policy/endpoint/stratum the
    /// line prices (e.g. each parity runner is quoted separately).
    pub target: Option<String>,
    /// The declared quantity (`None` = unquoted → `exposure`).
    pub quantity: Option<u64>,
    /// The declared unit price in currency micros (`None` =
    /// unpriced → `exposure`); negative is malformed.
    pub unit_price_micros: Option<i64>,
    /// `Some(source)` declares the line draws on another stage's
    /// capacity — pooled stage capacity refuses.
    pub pool_from: Option<String>,
    /// `true` declares an automatic top-up — refuses.
    pub top_up: bool,
}

/// One endpoint margin entry (`dimension ∈ parity DIMENSIONS`).
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointQuote {
    /// The contract dimension this entry quotes.
    pub dimension: String,
    /// The bound margin ε (ppm — `None` stays unresolved, never a
    /// guessed margin).
    pub margin_ppm: Option<i64>,
    /// `true` asserts an assumed/estimated margin — refuses.
    pub assumed_margin: bool,
}

/// One declared capability bound (`component ∈ ENVELOPE_COMPONENTS`).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeQty {
    /// The bound component.
    pub component: String,
    /// The declared quantity — over the frozen ceiling refuses.
    pub quantity: u64,
}

/// One stage's quote declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct StageDecl {
    /// The stage (`∈ STAGES`).
    pub stage: String,
    /// The declared fixed task count (`0` = unfixed — replication).
    pub tasks: u32,
    /// The declared equal repeats (`0` = unfixed).
    pub repeats: u32,
    /// The parity stage's declared H/O pair count — must equal
    /// `tasks · repeats`; forbidden on subject stages.
    pub declared_pairs: Option<u64>,
    /// The binding inputs — each `PINS` member declared exactly
    /// once (undeclared members refuse `MissingKey`; the states
    /// carry bound/unresolved/unknown).
    pub inputs: Vec<BindingInput>,
    /// The quoted endpoint margins (parity requires ≥1).
    pub endpoints: Vec<EndpointQuote>,
    /// The quoted strata (non-empty on pilot/parity/confirmation).
    pub strata: Vec<String>,
    /// The quoted policies (exactly two on pilot/parity/
    /// confirmation — both actual policies).
    pub policies: Vec<String>,
    /// The quoted runners (exactly two on parity — both runners;
    /// ≥1 elsewhere).
    pub runners: Vec<String>,
    /// The declared capability bounds.
    pub envelope: Vec<EnvelopeQty>,
    /// The priced resource worksheet lines.
    pub resources: Vec<ResourceLine>,
    /// The declared equal-directional discordance counts for the
    /// precision diagnostic (`0` is always included).
    pub discordances: Vec<u64>,
    /// `true` declares a new-cloud fallback — refuses.
    pub cloud_fallback: bool,
    /// `true` declares outcome-conditioned selection — favorable
    /// cohort shopping refuses.
    pub outcome_conditioned: bool,
    /// `true` records the dependent dispatch action — the outcome is
    /// `refused{blocker}` while blocked and `gate_pending{gate}` at
    /// best; never an authorization.
    pub dispatch_requested: bool,
}

/// The packet declaration (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct QuoteDecl {
    /// The declared currency — `None` prices $0 and blocks the
    /// dependent action (`funding`).
    pub currency: Option<String>,
    /// The packet prerequisites — each `PREREQUISITES` member
    /// declared exactly once.
    pub prerequisites: Vec<Prerequisite>,
    /// The stage declarations — each `STAGES` member at most once.
    pub stages: Vec<StageDecl>,
}

// ── the quote record ──────────────────────────────────────────────

/// One attainable-precision row (parity stage): the directional-KL
/// bound at `x = c/N`, η = .025, and the equal-directional
/// half-width `U − L` — a diagnostic, never power.
#[derive(Debug, Clone, PartialEq)]
pub struct PrecisionRow {
    /// The discordance count the row inverts.
    pub c: u64,
    /// `c/N` in ppm.
    pub x_ppm: i64,
    /// `K_η(c/N, N)` lower endpoint (ppm, outward-rounded).
    pub lo_ppm: i64,
    /// `K_η(c/N, N)` upper endpoint (ppm).
    pub hi_ppm: i64,
    /// `hi − lo` — the attainable Δ half-width.
    pub half_width_ppm: i64,
}

/// One stage's computed packet.
#[derive(Debug, Clone, PartialEq)]
pub struct StagePacket {
    /// The stage.
    pub stage: String,
    /// `ready_for_gate` | `blocked` | `infeasible`.
    pub state: String,
    /// The sorted typed blockers (empty when `ready_for_gate`).
    pub blockers: Vec<String>,
    /// The dispatch outcome (`not_requested` |
    /// `refused{first blocker}` | `gate_pending{gate}`).
    pub dispatch: String,
    /// The computed counts (`ab_subjects` vs `ho_pairs` are
    /// structurally distinct members).
    pub counts: BTreeMap<String, u64>,
    /// The echoed binding-input states (`name → bound:{ref} |
    /// unresolved | unknown`) — absent values stay unresolved,
    /// never fabricated.
    pub inputs: Vec<(String, String)>,
    /// The echoed quoted axes.
    pub strata: Vec<String>,
    /// The quoted policies.
    pub policies: Vec<String>,
    /// The quoted runners.
    pub runners: Vec<String>,
    /// The echoed endpoint margins (`dimension → margin_ppm |
    /// null`).
    pub endpoints: Vec<(String, Option<i64>)>,
    /// The echoed capability bounds (`component → quantity`).
    pub envelope: Vec<(String, u64)>,
    /// The echoed worksheet lines (canonical-JSON rows —
    /// `exposure_micros` | `unpriced` | `unbounded` per line).
    pub resources: Vec<Json>,
    /// The stage's summed exposure in currency micros (`0` when
    /// currency is unset).
    pub exposure_micros: i64,
    /// The attainable-precision state (`attainable` | `infeasible` |
    /// `unresolved` | `n/a` — non-parity stages).
    pub precision_state: String,
    /// The precision rows (parity stage only).
    pub precision: Vec<PrecisionRow>,
}

/// The quote packet — `quote_id` is the `idp_digest` commitment the
/// later binding gates (HUMAN-H8, GATE-G12/13/14) reference.
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureQuote {
    /// `H(canonical(record minus quote_id))` under `exposure_quote`.
    pub quote_id: String,
    /// The declared currency (`None` ⇒ totals are $0).
    pub currency: Option<String>,
    /// The packet total exposure in currency micros (`0` unset).
    pub total_micros: i64,
    /// The echoed prerequisite states (`name → satisfied{ref} |
    /// unmet | unknown`).
    pub prerequisites: Vec<(String, String)>,
    /// The per-stage packets.
    pub stages: Vec<StagePacket>,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

fn opt_str(o: &Option<String>) -> Json {
    match o {
        Some(s) => Json::str(s),
        None => Json::Null,
    }
}

impl StagePacket {
    fn to_json(&self) -> Json {
        Json::obj([
            ("stage", Json::str(&self.stage)),
            ("state", Json::str(&self.state)),
            (
                "blockers",
                Json::Arr(self.blockers.iter().map(Json::str).collect()),
            ),
            ("dispatch", Json::str(&self.dispatch)),
            (
                "counts",
                Json::Obj(
                    self.counts
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                        .collect(),
                ),
            ),
            (
                "inputs",
                Json::Obj(
                    self.inputs
                        .iter()
                        .map(|(n, s)| (n.clone(), Json::str(s)))
                        .collect(),
                ),
            ),
            (
                "strata",
                Json::Arr(self.strata.iter().map(Json::str).collect()),
            ),
            (
                "policies",
                Json::Arr(self.policies.iter().map(Json::str).collect()),
            ),
            (
                "runners",
                Json::Arr(self.runners.iter().map(Json::str).collect()),
            ),
            (
                "endpoints",
                Json::Obj(
                    self.endpoints
                        .iter()
                        .map(|(d, m)| (d.clone(), m.map_or(Json::Null, Json::Int)))
                        .collect(),
                ),
            ),
            (
                "envelope",
                Json::Obj(
                    self.envelope
                        .iter()
                        .map(|(c, q)| (c.clone(), Json::Int(*q as i64)))
                        .collect(),
                ),
            ),
            ("resources", Json::Arr(self.resources.clone())),
            ("exposure_micros", Json::Int(self.exposure_micros)),
            ("precision_state", Json::str(&self.precision_state)),
            (
                "precision",
                Json::Arr(
                    self.precision
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("c", Json::Int(r.c as i64)),
                                ("x_ppm", Json::Int(r.x_ppm)),
                                ("lo_ppm", Json::Int(r.lo_ppm)),
                                ("hi_ppm", Json::Int(r.hi_ppm)),
                                ("half_width_ppm", Json::Int(r.half_width_ppm)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

impl ExposureQuote {
    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("quote_id");
        }
        self.quote_id == idp_digest(DOMAIN, j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("quote_id", Json::str(&self.quote_id)),
            ("currency", opt_str(&self.currency)),
            ("total_micros", Json::Int(self.total_micros)),
            (
                "prerequisites",
                Json::Obj(
                    self.prerequisites
                        .iter()
                        .map(|(n, s)| (n.clone(), Json::str(s)))
                        .collect(),
                ),
            ),
            (
                "stages",
                Json::Arr(self.stages.iter().map(StagePacket::to_json).collect()),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

// ── the closed refusal set ────────────────────────────────────────

/// `build_quote`'s refusal vocabulary — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum QuoteError {
    /// A required member is absent/empty.
    MissingKey { member: &'static str },
    /// A stage outside `STAGES`.
    StageVocabulary { stage: String },
    /// A stage declared twice.
    DuplicateStage { stage: String },
    /// A binding input outside `PINS`.
    PinVocabulary { stage: String, name: String },
    /// A binding input declared twice.
    DuplicatePin { stage: String, name: String },
    /// A prerequisite outside `PREREQUISITES`.
    PrereqVocabulary { name: String },
    /// A prerequisite declared twice.
    DuplicatePrereq { name: String },
    /// A resource line outside `COMPONENTS`.
    ComponentVocabulary { stage: String, component: String },
    /// A `(component, target)` line declared twice.
    DuplicateComponent { stage: String, component: String },
    /// An envelope quantity outside `ENVELOPE_COMPONENTS` or
    /// declared twice.
    EnvelopeVocabulary { stage: String, component: String },
    /// A declared bound/line over the frozen §9.2 ceiling — the
    /// tranche stops/requotes, never silently widens.
    EnvelopeExceeded {
        stage: String,
        component: String,
        declared: u64,
        ceiling: u64,
    },
    /// `2TR < 10T` — the subject floor binds (R ≥ 5).
    SubjectFloor { stage: String, t: u32, r: u32 },
    /// A declared count disagrees with the computed one.
    CountMismatch {
        stage: String,
        member: &'static str,
        declared: u64,
        computed: u64,
    },
    /// A subjects count on the pairs stage or a pairs count on a
    /// subject stage — 300 A/B subjects are not 300 H/O pairs.
    CountConflation { stage: String, member: &'static str },
    /// Activation is the separate one-task quote.
    ActivationShape { detail: String },
    /// Parity quotes exactly both runners; others ≥1.
    RunnerSet { stage: String, detail: String },
    /// The study stages quote exactly both policies.
    PolicySet { stage: String, detail: String },
    /// The parity stage carries ≥1 endpoint margin entry.
    EndpointVocabulary { stage: String, detail: String },
    /// An automatic top-up was declared.
    AutomaticTopUp { stage: String, component: String },
    /// A line draws on another stage's capacity.
    PooledCapacity {
        stage: String,
        component: String,
        source: String,
    },
    /// A new-cloud fallback was declared.
    CloudFallback { stage: String },
    /// An assumed/estimated margin — never a guessed margin.
    GuessedMargin { stage: String, dimension: String },
    /// Outcome-conditioned selection — favorable cohort shopping.
    FavorableShopping { stage: String },
    /// A negative price or otherwise malformed line.
    ExposureMalformed {
        stage: String,
        component: String,
        detail: String,
    },
}

impl std::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use QuoteError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::StageVocabulary { stage } => write!(f, "unknown stage {stage}"),
            E::DuplicateStage { stage } => write!(f, "stage {stage} declared twice"),
            E::PinVocabulary { stage, name } => write!(f, "{stage}: unknown pin {name}"),
            E::DuplicatePin { stage, name } => write!(f, "{stage}: pin {name} declared twice"),
            E::PrereqVocabulary { name } => write!(f, "unknown prerequisite {name}"),
            E::DuplicatePrereq { name } => write!(f, "prerequisite {name} declared twice"),
            E::ComponentVocabulary { stage, component } => {
                write!(f, "{stage}: unknown component {component}")
            }
            E::DuplicateComponent { stage, component } => {
                write!(f, "{stage}: component {component} declared twice")
            }
            E::EnvelopeVocabulary { stage, component } => {
                write!(f, "{stage}: unknown/duplicate envelope member {component}")
            }
            E::EnvelopeExceeded {
                stage,
                component,
                declared,
                ceiling,
            } => write!(
                f,
                "{stage}: {component} {declared} exceeds ceiling {ceiling}"
            ),
            E::SubjectFloor { stage, t, r } => {
                write!(f, "{stage}: 2·{t}·{r} < 10·{t} — the subject floor binds")
            }
            E::CountMismatch {
                stage,
                member,
                declared,
                computed,
            } => write!(
                f,
                "{stage}: {member} declared {declared}, computed {computed}"
            ),
            E::CountConflation { stage, member } => {
                write!(f, "{stage}: {member} conflates subjects and H/O pairs")
            }
            E::ActivationShape { detail } => write!(f, "activation is one task: {detail}"),
            E::RunnerSet { stage, detail } => write!(f, "{stage}: runner set {detail}"),
            E::PolicySet { stage, detail } => write!(f, "{stage}: policy set {detail}"),
            E::EndpointVocabulary { stage, detail } => {
                write!(f, "{stage}: endpoint entries {detail}")
            }
            E::AutomaticTopUp { stage, component } => {
                write!(f, "{stage}: {component} declares an automatic top-up")
            }
            E::PooledCapacity {
                stage,
                component,
                source,
            } => write!(f, "{stage}: {component} pools capacity from {source}"),
            E::CloudFallback { stage } => write!(f, "{stage}: cloud fallback declared"),
            E::GuessedMargin { stage, dimension } => {
                write!(f, "{stage}: {dimension} margin assumed — never guessed")
            }
            E::FavorableShopping { stage } => {
                write!(f, "{stage}: outcome-conditioned selection declared")
            }
            E::ExposureMalformed {
                stage,
                component,
                detail,
            } => write!(f, "{stage}: {component} malformed: {detail}"),
        }
    }
}

impl std::error::Error for QuoteError {}

// ── the builder ───────────────────────────────────────────────────

/// One canonical worksheet line row — `exposure_micros` when
/// priced, `unpriced`/`unbounded` otherwise.
fn line_json(l: &ResourceLine, priced: Option<i64>, unbounded: bool) -> Json {
    Json::obj([
        ("component", Json::str(&l.component)),
        ("target", opt_str(&l.target)),
        (
            "quantity",
            l.quantity.map_or(Json::Null, |q| Json::Int(q as i64)),
        ),
        (
            "unit_price_micros",
            l.unit_price_micros.map_or(Json::Null, Json::Int),
        ),
        (
            "exposure_micros",
            if unbounded {
                Json::str("unbounded")
            } else {
                priced.map_or(Json::str("unpriced"), Json::Int)
            },
        ),
    ])
}

/// The pin → AC-2 blocker vocabulary mapping.
fn blocker_of(pin: &str) -> &'static str {
    match pin {
        "endpoint_map" => "map",
        "backend" => "backend",
        "rights" => "rights",
        _ => "pins",
    }
}

/// `build_quote(decl)` — validate the supplied declarations and emit
/// the deterministic quote packet. Pure and total: same inputs ⇒
/// byte-identical record and `quote_id`.
pub fn build_quote(decl: &QuoteDecl) -> Result<ExposureQuote, QuoteError> {
    use QuoteError as E;

    // ── packet prerequisites: the seven names, each declared once ──
    let mut prereq_seen: BTreeSet<&str> = BTreeSet::new();
    let mut prereq_rows: Vec<(String, String)> = Vec::new();
    let mut unmet: Vec<String> = Vec::new();
    for p in &decl.prerequisites {
        if !PREREQUISITES.contains(&p.name.as_str()) {
            return Err(E::PrereqVocabulary {
                name: p.name.clone(),
            });
        }
        if !prereq_seen.insert(p.name.as_str()) {
            return Err(E::DuplicatePrereq {
                name: p.name.clone(),
            });
        }
        let (label, met) = match &p.status {
            PrereqStatus::Satisfied(r) if r.is_empty() => {
                return Err(E::MissingKey {
                    member: "prerequisites[].ref",
                })
            }
            PrereqStatus::Satisfied(r) => (format!("satisfied:{r}"), true),
            PrereqStatus::Unmet => ("unmet".to_string(), false),
            PrereqStatus::Unknown => ("unknown".to_string(), false),
        };
        if !met {
            unmet.push(p.name.clone());
        }
        prereq_rows.push((p.name.clone(), label));
    }
    for name in PREREQUISITES {
        if !prereq_seen.contains(name) {
            return Err(E::MissingKey {
                member: "prerequisites[]",
            });
        }
    }

    // ── stage vocabulary/uniqueness ───────────────────────────────
    let mut stage_seen: BTreeSet<&str> = BTreeSet::new();
    for s in &decl.stages {
        if !STAGES.contains(&s.stage.as_str()) {
            return Err(E::StageVocabulary {
                stage: s.stage.clone(),
            });
        }
        if !stage_seen.insert(s.stage.as_str()) {
            return Err(E::DuplicateStage {
                stage: s.stage.clone(),
            });
        }
    }

    let mut packets: Vec<StagePacket> = Vec::with_capacity(decl.stages.len());
    let mut total_micros: i128 = 0;

    for s in &decl.stages {
        let stage = s.stage.as_str();
        let mut blockers: BTreeSet<String> = BTreeSet::new();
        for u in &unmet {
            blockers.insert(format!("prerequisite:{u}"));
        }
        if decl.currency.is_none() {
            blockers.insert("funding:currency_unset".to_string());
        }

        // ── forbidden constructs (construction-time refusals) ────
        if s.cloud_fallback {
            return Err(E::CloudFallback {
                stage: s.stage.clone(),
            });
        }
        if s.outcome_conditioned {
            return Err(E::FavorableShopping {
                stage: s.stage.clone(),
            });
        }
        if stage == "activation" && s.tasks != 1 {
            return Err(E::ActivationShape {
                detail: format!("tasks {} declared, exactly 1 required", s.tasks),
            });
        }

        // ── pins: the full set, each exactly once ─────────────────
        let mut pin_seen: BTreeSet<&str> = BTreeSet::new();
        let mut input_rows: Vec<(String, String)> = Vec::new();
        for i in &s.inputs {
            if !PINS.contains(&i.name.as_str()) {
                return Err(E::PinVocabulary {
                    stage: s.stage.clone(),
                    name: i.name.clone(),
                });
            }
            if !pin_seen.insert(i.name.as_str()) {
                return Err(E::DuplicatePin {
                    stage: s.stage.clone(),
                    name: i.name.clone(),
                });
            }
            match &i.state {
                InputState::Bound(r) if r.is_empty() => {
                    return Err(E::MissingKey {
                        member: "inputs[].ref",
                    })
                }
                InputState::Bound(r) => input_rows.push((i.name.clone(), format!("bound:{r}"))),
                InputState::Unresolved | InputState::Unknown => {
                    blockers.insert(format!("{}:{}", blocker_of(&i.name), i.name));
                    input_rows.push((i.name.clone(), i.state.name().to_string()));
                }
            }
        }
        for name in PINS {
            if !pin_seen.contains(name) {
                return Err(E::MissingKey { member: "inputs[]" });
            }
        }

        // ── quoted axes: strata/policies/runners ──────────────────
        let study_stage = matches!(stage, "pilot" | "parity" | "confirmation");
        if study_stage && s.strata.is_empty() {
            return Err(E::MissingKey { member: "strata" });
        }
        if s.strata.iter().any(|x| x.is_empty())
            || s.strata.iter().collect::<BTreeSet<_>>().len() != s.strata.len()
        {
            return Err(E::MissingKey { member: "strata" });
        }
        // The study stages quote exactly both policies; activation/
        // replication quote at most both.
        let policies_ok = if study_stage {
            s.policies.len() == 2
        } else {
            s.policies.len() <= 2
        };
        if !policies_ok
            || s.policies.iter().any(|p| p.is_empty())
            || s.policies.iter().collect::<BTreeSet<_>>().len() != s.policies.len()
        {
            return Err(E::PolicySet {
                stage: s.stage.clone(),
                detail: format!(
                    "{} declared; {} required (distinct, non-empty)",
                    s.policies.len(),
                    if study_stage { 2 } else { 1 }
                ),
            });
        }
        // Parity quotes exactly both runners; the other stages
        // declare 1–2 (native only, or a both-runner reproduction).
        let runners_ok = if stage == "parity" {
            s.runners.len() == 2
        } else {
            !s.runners.is_empty() && s.runners.len() <= 2
        };
        if !runners_ok
            || s.runners.iter().any(|r| r.is_empty())
            || s.runners.iter().collect::<BTreeSet<_>>().len() != s.runners.len()
        {
            return Err(E::RunnerSet {
                stage: s.stage.clone(),
                detail: format!(
                    "{} declared; {} required (distinct, non-empty)",
                    s.runners.len(),
                    if stage == "parity" {
                        "exactly 2"
                    } else {
                        "1–2"
                    }
                ),
            });
        }

        // ── endpoint margins ──────────────────────────────────────
        if stage == "parity" && s.endpoints.is_empty() {
            return Err(E::EndpointVocabulary {
                stage: s.stage.clone(),
                detail: "no margin entries — parity needs ≥1".into(),
            });
        }
        let mut dims_seen: BTreeSet<&str> = BTreeSet::new();
        for e in &s.endpoints {
            if !DIMENSIONS.contains(&e.dimension.as_str())
                || !dims_seen.insert(e.dimension.as_str())
            {
                return Err(E::EndpointVocabulary {
                    stage: s.stage.clone(),
                    detail: format!("dimension {} outside/duplicated", e.dimension),
                });
            }
            if e.assumed_margin {
                return Err(E::GuessedMargin {
                    stage: s.stage.clone(),
                    dimension: e.dimension.clone(),
                });
            }
            match e.margin_ppm {
                // ε binds the parity diagnostic alone — an unset
                // margin blocks precision there; elsewhere the entry
                // is echoed without a margin obligation.
                None if stage == "parity" => {
                    blockers.insert(format!("precision:margin_unresolved:{}", e.dimension));
                }
                Some(m) if m <= 0 => {
                    return Err(E::ExposureMalformed {
                        stage: s.stage.clone(),
                        component: e.dimension.clone(),
                        detail: "non-positive margin".into(),
                    })
                }
                _ => {}
            }
        }

        // ── counts: subjects ≠ pairs ──────────────────────────────
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        if s.tasks > 0 && s.repeats > 0 {
            let n_pairs = s.tasks as u64 * s.repeats as u64;
            let n_subjects = 2 * n_pairs;
            match stage {
                "confirmation" => {
                    if n_subjects < 10 * s.tasks as u64 {
                        return Err(E::SubjectFloor {
                            stage: s.stage.clone(),
                            t: s.tasks,
                            r: s.repeats,
                        });
                    }
                    counts.insert("ab_subjects".into(), n_subjects);
                }
                "parity" => {
                    counts.insert("ho_pairs".into(), n_pairs);
                }
                _ => {}
            }
            if let Some(d) = s.declared_pairs {
                match stage {
                    "parity" => {
                        if d != n_pairs {
                            return Err(E::CountMismatch {
                                stage: s.stage.clone(),
                                member: "declared_pairs",
                                declared: d,
                                computed: n_pairs,
                            });
                        }
                    }
                    _ => {
                        return Err(E::CountConflation {
                            stage: s.stage.clone(),
                            member: "declared_pairs",
                        })
                    }
                }
            }
        } else if s.declared_pairs.is_some() {
            return Err(E::CountConflation {
                stage: s.stage.clone(),
                member: "declared_pairs",
            });
        }

        // ── envelope: declared bounds within frozen ceilings ──────
        let cap: BTreeMap<&str, u64> = ceilings(stage).into_iter().collect();
        let mut env_seen: BTreeSet<&str> = BTreeSet::new();
        for q in &s.envelope {
            if !ENVELOPE_COMPONENTS.contains(&q.component.as_str())
                || !env_seen.insert(q.component.as_str())
            {
                return Err(E::EnvelopeVocabulary {
                    stage: s.stage.clone(),
                    component: q.component.clone(),
                });
            }
            if let Some(c) = cap.get(q.component.as_str()) {
                if q.quantity > *c {
                    return Err(E::EnvelopeExceeded {
                        stage: s.stage.clone(),
                        component: q.component.clone(),
                        declared: q.quantity,
                        ceiling: *c,
                    });
                }
            }
        }
        // Unset enforceable bounds block the dependent action.
        if matches!(stage, "pilot" | "confirmation") {
            for (component, _) in &cap {
                if !env_seen.contains(component) {
                    blockers.insert(format!("exposure:bound_unset:{component}"));
                }
            }
        }

        // ── resource worksheet ────────────────────────────────────
        let mut seen_lines: BTreeSet<(&str, &str)> = BTreeSet::new();
        let mut exposure_micros: i128 = 0;
        let mut components_present: BTreeSet<&str> = BTreeSet::new();
        let mut resource_rows: Vec<Json> = Vec::new();
        for l in &s.resources {
            if !COMPONENTS.contains(&l.component.as_str()) {
                return Err(E::ComponentVocabulary {
                    stage: s.stage.clone(),
                    component: l.component.clone(),
                });
            }
            if !seen_lines.insert((l.component.as_str(), l.target.as_deref().unwrap_or(""))) {
                return Err(E::DuplicateComponent {
                    stage: s.stage.clone(),
                    component: l.component.clone(),
                });
            }
            if l.top_up {
                return Err(E::AutomaticTopUp {
                    stage: s.stage.clone(),
                    component: l.component.clone(),
                });
            }
            if let Some(src) = &l.pool_from {
                return Err(E::PooledCapacity {
                    stage: s.stage.clone(),
                    component: l.component.clone(),
                    source: src.clone(),
                });
            }
            if matches!(l.unit_price_micros, Some(p) if p < 0) {
                return Err(E::ExposureMalformed {
                    stage: s.stage.clone(),
                    component: l.component.clone(),
                    detail: "negative price".into(),
                });
            }
            if stage == "parity" && l.component == "subjects_first" {
                return Err(E::CountConflation {
                    stage: s.stage.clone(),
                    member: "subjects_first",
                });
            }
            components_present.insert(l.component.as_str());
            if l.component == "provider_unknown_exposure" {
                // The only bounded declaration is an explicit zero.
                let bounded = l.quantity == Some(0);
                if !bounded {
                    blockers.insert("exposure:unbounded:provider_unknown_exposure".into());
                }
                resource_rows.push(line_json(l, None, !bounded));
                continue;
            }
            let priced = match (l.quantity, l.unit_price_micros) {
                (Some(q), Some(p)) => {
                    let line = q as i128 * p as i128;
                    exposure_micros += line;
                    Some(line.min(i64::MAX as i128) as i64)
                }
                _ => {
                    blockers.insert(format!("exposure:unpriced:{}", l.component));
                    None
                }
            };
            resource_rows.push(line_json(l, priced, false));
            // Frozen first-subject ceilings on the count line.
            if l.component == "subjects_first" {
                let ceiling = match stage {
                    "pilot" => Some(PILOT_MAX_SUBJECTS),
                    "confirmation" => Some(CONFIRM_MAX_SUBJECTS),
                    _ => None,
                };
                if let (Some(c), Some(q)) = (ceiling, l.quantity) {
                    if q > c {
                        return Err(E::EnvelopeExceeded {
                            stage: s.stage.clone(),
                            component: l.component.clone(),
                            declared: q,
                            ceiling: c,
                        });
                    }
                }
                if let (Some(q), true) = (l.quantity, stage == "confirmation") {
                    let computed = 2 * s.tasks as u64 * s.repeats as u64;
                    if s.tasks > 0 && s.repeats > 0 && q != computed {
                        return Err(E::CountMismatch {
                            stage: s.stage.clone(),
                            member: "subjects_first",
                            declared: q,
                            computed,
                        });
                    }
                }
            }
            if let Some(t) = &l.target {
                if t.is_empty() {
                    return Err(E::ExposureMalformed {
                        stage: s.stage.clone(),
                        component: l.component.clone(),
                        detail: "empty target".into(),
                    });
                }
            }
        }
        // Worksheet coverage — every required component declared.
        for c in required_components(stage) {
            if !components_present.contains(c) {
                blockers.insert(format!("exposure:unquoted:{c}"));
            }
        }
        // Parity quotes both runners — each a priced target.
        if stage == "parity" {
            for r in &s.runners {
                let quoted = s
                    .resources
                    .iter()
                    .any(|l| l.target.as_deref() == Some(r.as_str()));
                if !quoted {
                    blockers.insert(format!("exposure:runner_unquoted:{r}"));
                }
            }
        }

        // ── precision diagnostics (parity stage, η = .025) ────────
        let mut precision: Vec<PrecisionRow> = Vec::new();
        let mut precision_state = "n/a".to_string();
        if stage == "parity" {
            precision_state = "attainable".to_string();
            let n = counts.get("ho_pairs").copied().unwrap_or(0);
            if n == 0 {
                blockers.insert("precision:design_unfixed".into());
                precision_state = "unresolved".into();
            } else {
                let mut scenarios: Vec<u64> = vec![0];
                scenarios.extend(s.discordances.iter().copied());
                scenarios.sort_unstable();
                scenarios.dedup();
                for c in scenarios {
                    if c > n {
                        return Err(E::ExposureMalformed {
                            stage: s.stage.clone(),
                            component: "discordances".into(),
                            detail: format!("c {c} exceeds N {n}"),
                        });
                    }
                    let x_ppm = (c * 1_000_000 / n) as i64;
                    let Some(iv) = kl_directional_bound(x_ppm, n as i64, ETA_TOST_PPM) else {
                        return Err(E::ExposureMalformed {
                            stage: s.stage.clone(),
                            component: "discordances".into(),
                            detail: "KL bound out of domain".into(),
                        });
                    };
                    let half = iv.hi - iv.lo;
                    precision.push(PrecisionRow {
                        c,
                        x_ppm,
                        lo_ppm: iv.lo,
                        hi_ppm: iv.hi,
                        half_width_ppm: half,
                    });
                    for e in &s.endpoints {
                        match e.margin_ppm {
                            None => precision_state = "unresolved".into(),
                            Some(m) if half > m => precision_state = "infeasible".into(),
                            _ => {}
                        }
                    }
                }
            }
            if precision_state == "infeasible" {
                blockers.insert("precision:infeasible".into());
            }
        }

        // ── readiness + dispatch (never an authorization) ─────────
        let state = if precision_state == "infeasible" {
            "infeasible"
        } else if blockers.is_empty() {
            "ready_for_gate"
        } else {
            "blocked"
        };
        let gate = STAGE_GATES
            .iter()
            .find(|(g, _)| *g == stage)
            .map(|(_, n)| *n)
            .unwrap_or("unscheduled");
        let dispatch = if !s.dispatch_requested {
            "not_requested".to_string()
        } else if state == "ready_for_gate" {
            format!("gate_pending:{gate}")
        } else {
            format!("refused:{}", blockers.iter().next().expect("nonempty"))
        };
        if decl.currency.is_none() {
            exposure_micros = 0;
        }
        total_micros += exposure_micros;
        packets.push(StagePacket {
            stage: s.stage.clone(),
            state: state.to_string(),
            blockers: blockers.into_iter().collect(),
            dispatch,
            counts,
            inputs: input_rows,
            strata: s.strata.clone(),
            policies: s.policies.clone(),
            runners: s.runners.clone(),
            endpoints: s
                .endpoints
                .iter()
                .map(|e| (e.dimension.clone(), e.margin_ppm))
                .collect(),
            envelope: s
                .envelope
                .iter()
                .map(|e| (e.component.clone(), e.quantity))
                .collect(),
            resources: resource_rows,
            exposure_micros: exposure_micros.min(i64::MAX as i128) as i64,
            precision_state,
            precision,
        });
    }

    let metadata = MethodMetadata {
        estimator: KIND.to_string(),
        version: VERSION.to_string(),
        target: "per-stage readiness + full exposure quote".into(),
        assumptions: vec![
            "records-in: every input bound by HUMAN-H8/stage gates; none chosen".into(),
            "unset currency prices $0; unbounded/unknown exposure blocks only dependents".into(),
            "attainable-precision diagnostics at eta=.025 — widths, never power".into(),
        ],
        prior: None,
        keys: BTreeMap::from([(
            "currency".to_string(),
            decl.currency.clone().unwrap_or_else(|| "unset".to_string()),
        )]),
        stratum_membership: vec![],
        seed: 0,
        draws_requested: 0,
        draws_used: 0,
        floors: vec![
            "subjects=2TR>=10T".into(),
            format!("pilot_subjects<={PILOT_MAX_SUBJECTS}"),
            format!("confirm_subjects<={CONFIRM_MAX_SUBJECTS}"),
        ],
        interval: ("descriptive".into(), None),
        refusal: None,
    };

    let mut out = ExposureQuote {
        quote_id: String::new(),
        currency: decl.currency.clone(),
        total_micros: total_micros.min(i64::MAX as i128) as i64,
        prerequisites: prereq_rows,
        stages: packets,
        metadata,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("quote_id");
    }
    out.quote_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
