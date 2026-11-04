//! `hh-backend-preflight/1` — the bounded backend feasibility preflight
//! (R5.42a; GATE-G18's admitted scope; evidence spec §9.2 backend row +
//! §2.1's P2 backend admission).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — pin
//! digests, the actual probe, mediation/observer/probe outcomes, policy
//! declarations and the landed exposure quote — and never manufactures
//! an execution, a capability or a pass. `unsupported{…}`,
//! `inconclusive{…}` and `refused{…}` are typed returns for scope
//! decision, never an S1/S4 acceptance.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

/// The record version.
pub const VERSION: &str = "hh-backend-preflight/1";
const DOMAIN: &str = "backend_preflight";

/// The six admitted legs (frozen vocabulary — GATE-G18's scope).
pub const LEGS: [&str; 6] = [
    "tb2_lifecycle",
    "backend_candidate",
    "mediation",
    "observer_probe",
    "acquisition_network",
    "worksheet",
];

/// One leg's typed verdict. Only `Verified` contributes toward
/// `feasible`; every other variant names why verbatim.
#[derive(Debug, Clone, PartialEq)]
pub enum LegVerdict {
    /// The leg's predicates all held on the supplied facts.
    Verified,
    /// The actual backend lacks the capability (`capability:<fact>`).
    Unsupported(String),
    /// The leg could not be determined on this host
    /// (`<fact>_unavailable`/`<fact>_unobserved`).
    Inconclusive(String),
    /// A rule/policy refused (`<predicate>` violated).
    Refused(String),
}

impl LegVerdict {
    /// The closed verdict vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            LegVerdict::Verified => "verified",
            LegVerdict::Unsupported(_) => "unsupported",
            LegVerdict::Inconclusive(_) => "inconclusive",
            LegVerdict::Refused(_) => "refused",
        }
    }
    /// The named reason (absent on `verified`).
    pub fn detail(&self) -> Option<&str> {
        match self {
            LegVerdict::Verified => None,
            LegVerdict::Unsupported(r) | LegVerdict::Inconclusive(r) | LegVerdict::Refused(r) => {
                Some(r)
            }
        }
    }
    /// `true` only on `verified` — a single source for the fold.
    pub fn is_verified(&self) -> bool {
        matches!(self, LegVerdict::Verified)
    }
    /// The verdict's canonical JSON (`{verdict[, detail]}`) — shared
    /// with `useful`'s `hh-s1-useful/1` record (one verdict codec,
    /// never duplicated).
    pub(crate) fn to_json(&self) -> Json {
        let mut pairs = vec![("verdict", Json::str(self.name()))];
        if let Some(d) = self.detail() {
            pairs.push(("detail", Json::str(d)));
        }
        Json::obj(pairs)
    }
}

// ── leg 1 — tb2_lifecycle ────────────────────────────────────────────

/// One pinned upstream artifact: `artifact_id` bound to committed bytes
/// whose sha256 must equal `sha256` verbatim (HUMAN-H8's hash-verified
/// set — a mismatch is the substitute the gate excludes).
#[derive(Debug, Clone, PartialEq)]
pub struct PinnedArtifact {
    /// The artifact id (`a-count`, `ex-01`, …).
    pub artifact_id: String,
    /// Its path in the checkout.
    pub path: PathBuf,
    /// The declared sha256 (hex).
    pub sha256: String,
}

/// `check_pins` — recompute sha256 over each artifact's committed bytes.
/// A byte drift ⇒ `refused{pin_mismatch:<id>}` (infeasible — never a
/// substitution); an unreadable file ⇒ `inconclusive{pin_unreadable:<id>}`.
pub fn check_pins(pins: &[PinnedArtifact]) -> LegVerdict {
    for p in pins {
        let bytes = match std::fs::read(&p.path) {
            Ok(b) => b,
            Err(_) => return LegVerdict::Inconclusive(format!("pin_unreadable:{}", p.artifact_id)),
        };
        if sha256_hex(&bytes) != p.sha256 {
            return LegVerdict::Refused(format!("pin_mismatch:{}", p.artifact_id));
        }
    }
    LegVerdict::Verified
}

// ── leg 2 — backend_candidate ────────────────────────────────────────

/// The honest declaration facts the candidate must carry (HelperExecutor
/// over podman — confinement + probe + dedup + interrupt support).
pub const REQUIRED_DECLARATION: [(&str, &str); 4] = [
    ("isolation", "namespaces"),
    ("probe", "check"),
    ("dedup", "durable"),
    ("interrupt", "supported"),
];

/// Facts probed from the actual HelperExecutor/Podman candidate —
/// supplied by the caller that ran the live probe.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendProbe {
    /// The probed backend id (`podman`).
    pub backend: String,
    /// `podman info` answered (the engine is reachable).
    pub runtime_available: bool,
    /// The `hh-helper` binary resolved.
    pub helper_binary: bool,
    /// Spawn+hello result: `Some(true)` ok, `Some(false)` failed,
    /// `None` not attempted.
    pub handshake: Option<bool>,
    /// The handshake failure detail when it failed.
    pub handshake_error: Option<String>,
    /// The observed declaration (`member → value`) once a session came
    /// up — `None` when the backend never answered.
    pub declaration: Option<BTreeMap<String, String>>,
}

/// `evaluate_backend` — the supported-candidate check. An absent engine
/// or unattempted handshake is `inconclusive` (undetermined, not a fail);
/// a failed handshake or a missing required declaration member is
/// `unsupported`.
pub fn evaluate_backend(p: &BackendProbe) -> LegVerdict {
    if !p.runtime_available {
        return LegVerdict::Inconclusive(format!("backend_unavailable:{}", p.backend));
    }
    if !p.helper_binary {
        return LegVerdict::Inconclusive("helper_binary_unresolved".to_string());
    }
    match p.handshake {
        None => return LegVerdict::Inconclusive("handshake_not_attempted".to_string()),
        Some(false) => {
            return LegVerdict::Unsupported(format!(
                "handshake_failed:{}",
                p.handshake_error.as_deref().unwrap_or("unknown")
            ))
        }
        Some(true) => {}
    }
    let decl = match &p.declaration {
        None => return LegVerdict::Inconclusive("declaration_unobserved".to_string()),
        Some(d) => d,
    };
    for (member, required) in REQUIRED_DECLARATION {
        match decl.get(member) {
            Some(v) if v == required => {}
            Some(v) => {
                return LegVerdict::Unsupported(format!("capability:{member}={v}"));
            }
            None => {
                return LegVerdict::Unsupported(format!("capability:{member}_absent"));
            }
        }
    }
    LegVerdict::Verified
}

// ── leg 3 — mediation ────────────────────────────────────────────────

/// The observed result of one real `Dispatcher`+Π+`HelperExecutor`
/// mediated marker append — `carrier_backend` names the lane that
/// carried it verbatim (the HelperExecutor machinery, not a substitute).
#[derive(Debug, Clone, PartialEq)]
pub struct MediationOutcome {
    /// The lane that carried the drive (`podman`, `seatbelt`).
    pub carrier_backend: String,
    /// The drive ran at all.
    pub attempted: bool,
    /// The terminal dispatch outcome (`observed:applied`).
    pub outcome: Option<String>,
    /// Sink marker appends counted after quiescence (one honest append).
    pub markers: Option<u64>,
    /// The verbatim failure when the drive errored.
    pub failure: Option<String>,
}

/// `evaluate_mediation` — real mediation only: an unattempted drive is
/// `inconclusive`; a refused/errored dispatch is `refused`/`unsupported`;
/// a marker count ≠ 1 violates the append invariant.
pub fn evaluate_mediation(m: &MediationOutcome) -> LegVerdict {
    if !m.attempted {
        return LegVerdict::Inconclusive("mediation_not_attempted".to_string());
    }
    if let Some(f) = &m.failure {
        return LegVerdict::Unsupported(format!("mediation_error:{f}"));
    }
    match m.outcome.as_deref() {
        Some("observed:applied") => {}
        Some("refused") => return LegVerdict::Refused("mediation_denied".to_string()),
        Some(o) => return LegVerdict::Unsupported(format!("mediation_outcome:{o}")),
        None => return LegVerdict::Inconclusive("mediation_outcome_unobserved".to_string()),
    }
    match m.markers {
        Some(1) => LegVerdict::Verified,
        Some(n) => LegVerdict::Refused(format!("marker_count:{n}")),
        None => LegVerdict::Inconclusive("marker_count_unobserved".to_string()),
    }
}

// ── leg 4 — observer_probe ───────────────────────────────────────────

/// The surviving-observer + read-only-probe outcome (`observer.jsonl`
/// rows readable after the executor exits — survival is a file fact).
#[derive(Debug, Clone, PartialEq)]
pub struct ObserverOutcome {
    /// The drive ran at all.
    pub attempted: bool,
    /// Observation intervals durably recorded (three required:
    /// before_launch / at_boundary / after_quiescence).
    pub intervals: u64,
    /// The denied/read-only sentinel re-read intact.
    pub sentinel_intact: Option<bool>,
    /// The observer rows still readable after executor exit.
    pub survived: bool,
    /// The instrument `probe()` verdict
    /// (`applied|not_applied|undeterminable`).
    pub probe_verdict: Option<String>,
    /// The probe provenance — only `instrument` is admissible.
    pub probe_source: Option<String>,
}

/// `evaluate_observer` — the surviving observer/probe check: all three
/// intervals + an intact sentinel + a probe row from the instrument
/// itself (a sink-zero read authorizes nothing).
pub fn evaluate_observer(o: &ObserverOutcome) -> LegVerdict {
    if !o.attempted {
        return LegVerdict::Inconclusive("observer_not_attempted".to_string());
    }
    if o.intervals < 3 {
        return LegVerdict::Unsupported(format!("observer_incomplete:{}", o.intervals));
    }
    if o.sentinel_intact == Some(false) {
        return LegVerdict::Refused("sentinel_violated".to_string());
    }
    if !o.survived {
        return LegVerdict::Unsupported("observer_not_surviving".to_string());
    }
    match o.probe_source.as_deref() {
        Some("instrument") => {}
        Some(s) => return LegVerdict::Refused(format!("probe_inadmissible:{s}")),
        None => return LegVerdict::Inconclusive("probe_absent".to_string()),
    }
    match o.probe_verdict.as_deref() {
        Some("applied") | Some("not_applied") => LegVerdict::Verified,
        Some("undeterminable") => LegVerdict::Inconclusive("probe_undeterminable".to_string()),
        _ => LegVerdict::Inconclusive("probe_verdict_unobserved".to_string()),
    }
}

// ── leg 5 — acquisition_network ──────────────────────────────────────

/// The declared acquisition/network policy facts.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyFacts {
    /// The policy's default network mode (`none` required).
    pub network_default: String,
    /// The image identity kind (`content_address`|`foreign_digest`|`tag`).
    pub image_ref_kind: String,
    /// The declared acquisition path (`local_only`|`pull`|`unknown`).
    pub acquisition: String,
    /// Writable roots confined to the declared workspace.
    pub writable_roots_confined: bool,
}

/// `evaluate_policy` — the acquisition/network policy check: network off
/// by default, a declared image identity, local-only acquisition (no
/// pull/download/install hides in the envelope) and confined roots.
pub fn evaluate_policy(f: &PolicyFacts) -> LegVerdict {
    if f.network_default != "none" {
        return LegVerdict::Refused(format!("network_not_off:{}", f.network_default));
    }
    match f.acquisition.as_str() {
        "local_only" => {}
        "unknown" => return LegVerdict::Inconclusive("acquisition_unknown".to_string()),
        a => return LegVerdict::Refused(format!("acquisition_remote:{a}")),
    }
    match f.image_ref_kind.as_str() {
        "content_address" | "foreign_digest" | "tag" => {}
        k => return LegVerdict::Refused(format!("image_ref_undeclared:{k}")),
    }
    if !f.writable_roots_confined {
        return LegVerdict::Refused("writable_roots_unconfined".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — worksheet ────────────────────────────────────────────────

/// The §9.2 minimum-admission worksheet facts — folded from the landed
/// quote engine's packet (HUMAN-H8's declared floor values verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct WorksheetFacts {
    /// The declared currency (`USD` required).
    pub currency: Option<String>,
    /// The packet's total exposure in currency micros (`0` — the floor
    /// is $0.00 free lines).
    pub total_micros: i64,
    /// `provider_unknown_exposure` bounded at an explicit zero.
    pub unknown_exposure_bounded: bool,
    /// The pilot stage's declared `subjects_first` (`2·T·R`).
    pub pilot_subjects: Option<u64>,
    /// The declared pilot floor holds (`2·T·R ≥ 10·T`).
    pub pilot_floor_met: bool,
    /// The pilot packet's state (`ready_for_gate` when the floor is
    /// complete; the owed gate is echoed, never authorized).
    pub pilot_state: String,
    /// Stages whose packet dispatch is `not_requested` — quote-only,
    /// no allocated tranche.
    pub quote_only: Vec<String>,
    /// Stages still `blocked`/`infeasible` (their typed blockers are
    /// retained verbatim — never waived).
    pub blocked: Vec<String>,
    /// Worksheet lines left unpriced (unknown exposure).
    pub unpriced: Vec<String>,
    /// Forbidden constructs present (top_up/pool/cloud/guessed margin/
    /// favorable shopping — the engine refuses at build).
    pub forbidden: Vec<String>,
}

/// `evaluate_worksheet` — the minimum-admission floor: USD currency at
/// $0.00 total, unknown exposure bounded at zero, the pilot floor met
/// and gate-ready, parity + confirmation quote-only, zero forbidden
/// constructs.
pub fn evaluate_worksheet(f: &WorksheetFacts) -> LegVerdict {
    if f.currency.as_deref() != Some("USD") {
        return LegVerdict::Inconclusive("worksheet_currency_unset".to_string());
    }
    if let Some(x) = f.forbidden.first() {
        return LegVerdict::Refused(format!("worksheet_forbidden_construct:{x}"));
    }
    if !f.unknown_exposure_bounded {
        return LegVerdict::Refused("worksheet_unknown_exposure_unbounded".to_string());
    }
    if f.total_micros != 0 {
        return LegVerdict::Refused(format!("worksheet_spend_declared:{}", f.total_micros));
    }
    if let Some(x) = f.unpriced.first() {
        return LegVerdict::Unsupported(format!("worksheet_unpriced:{x}"));
    }
    if !f.pilot_floor_met {
        return LegVerdict::Refused("worksheet_pilot_floor_unmet".to_string());
    }
    if f.pilot_state != "ready_for_gate" {
        return LegVerdict::Unsupported(format!("worksheet_pilot_blocked:{}", f.pilot_state));
    }
    for stage in ["parity", "confirmation"] {
        if !f.quote_only.iter().any(|s| s == stage) {
            return LegVerdict::Refused(format!("worksheet_tranche_allocated:{stage}"));
        }
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────────

/// One leg's verdict plus the fact echo the evaluator consumed.
#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    /// The leg (`∈ LEGS`).
    pub name: String,
    /// Its typed verdict.
    pub verdict: LegVerdict,
    /// `member → value` fact echo (supplied inputs, verbatim).
    pub facts: BTreeMap<String, String>,
}

/// The packet disposition — `feasible` only when every leg verified.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg `verified` — the bounded preflight earned feasibility.
    Feasible,
    /// At least one `unsupported`/`refused` leg — a definitive failure.
    Infeasible,
    /// At least one `inconclusive` leg, none failed — undetermined.
    Inconclusive,
}

impl Disposition {
    /// The closed vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            Disposition::Feasible => "feasible",
            Disposition::Infeasible => "infeasible",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-backend-preflight/1` record. `scope_return` is `true` unless
/// `feasible` — the packet returns for explicit scope decision and is
/// never an S1/S4 acceptance.
#[derive(Debug, Clone, PartialEq)]
pub struct PreflightReport {
    /// `H(canonical(record minus preflight_id))` under `backend_preflight`.
    pub preflight_id: String,
    /// The record version.
    pub version: String,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// `true` unless `feasible` — returns for scope decision.
    pub scope_return: bool,
    /// The admission field — always `not_s1_s4_acceptance`.
    pub admission: String,
}

impl PreflightReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("preflight_id", Json::str(&self.preflight_id)),
            ("disposition", Json::str(self.disposition.name())),
            ("scope_return", Json::Bool(self.scope_return)),
            ("admission", Json::str(&self.admission)),
            (
                "legs",
                Json::Arr(
                    self.legs
                        .iter()
                        .map(|l| {
                            Json::obj([
                                ("name", Json::str(&l.name)),
                                ("verdict", l.verdict.to_json()),
                                (
                                    "facts",
                                    Json::Obj(
                                        l.facts
                                            .iter()
                                            .map(|(k, v)| (k.clone(), Json::str(v)))
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `assemble(legs)` — fold the six leg verdicts into the disposition and
/// mint `preflight_id` last over the canonical record. Every leg name
/// must be a `LEGS` member, unique and complete — a malformed leg set is
/// a `refused` assembly, never a partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<PreflightReport, String> {
    let mut seen = std::collections::BTreeSet::new();
    for l in &legs {
        if !LEGS.contains(&l.name.as_str()) {
            return Err(format!("leg vocabulary: {}", l.name));
        }
        if !seen.insert(l.name.as_str()) {
            return Err(format!("leg duplicate: {}", l.name));
        }
    }
    for name in LEGS {
        if !seen.contains(name) {
            return Err(format!("leg missing: {name}"));
        }
    }
    let definitive = legs.iter().any(|l| {
        matches!(
            l.verdict,
            LegVerdict::Unsupported(_) | LegVerdict::Refused(_)
        )
    });
    let undetermined = legs
        .iter()
        .any(|l| matches!(l.verdict, LegVerdict::Inconclusive(_)));
    let disposition = if definitive {
        Disposition::Infeasible
    } else if undetermined {
        Disposition::Inconclusive
    } else {
        Disposition::Feasible
    };
    let mut out = PreflightReport {
        preflight_id: String::new(),
        version: VERSION.to_string(),
        legs,
        scope_return: disposition != Disposition::Feasible,
        admission: "not_s1_s4_acceptance".to_string(),
        disposition,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("preflight_id");
    }
    out.preflight_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
