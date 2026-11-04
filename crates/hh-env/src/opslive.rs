//! `hh-ops-live/1` — the bounded S4 operational-proof execution packet
//! (R5.44; GATE-G11's admitted scope; evidence spec §5.2/§5.3).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the 24
//! frozen O1–O8×3 trials' actual drives on the admitted podman backend
//! (exec-bearing cells) and the native Account/prepare path (O6/O7),
//! the `OpsReport` stage counts, the two repaired machinery
//! preconditions and the declared §9.2 worksheet — and never
//! manufactures a dispatch, a verdict or a pass. Disposition is
//! `executed` only when every leg verifies; `o8` verifies on
//! `defect_detected` (detector sensitivity — never an invariant pass).

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::ops::{CellVerdict, Kp4Outcome, OpsCell, TrialOutcome};
use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-ops-live/1";
const DOMAIN: &str = "ops_live";

/// The admitted leg vocabulary (frozen — GATE-G11's S4 scope).
pub const LEGS: [&str; 11] = [
    "o1",
    "o2",
    "o3",
    "o4",
    "o5",
    "o6",
    "o7",
    "o8",
    "stage_report",
    "repairs",
    "worksheet",
];

/// The two machinery predicates R5.43's packet carried — repaired here
/// and recorded as discharged (never silently closed): the spawned
/// `--backend podman` is no longer clobbered by a policy hello, and the
/// `probe()` verb reaches the journal (live channel + resume reconnect).
pub const DISCHARGED: [&str; 2] = ["probe_verb_enoent", "exec_backend_hello_clobber"];

/// One trial's retained record — every field an observed fact, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct TrialOutcomeRecord {
    /// The frozen trial id (`o{cell}r{rep}` / a rerun's new identity).
    pub trial_id: String,
    /// The trial actually drove (a backend/setup failure still lands a
    /// retained record — `invoked: false`, named outcome).
    pub invoked: bool,
    /// The declared boundary was reached/assessable.
    pub boundary_observed: bool,
    /// The trial was fully observable.
    pub fully_observable: bool,
    /// The retained outcome (`TrialOutcome::as_str` — includes all the
    /// retained negative kinds; O8's expected is `defect_detected`).
    pub outcome: String,
    /// A counted violation.
    pub violation: bool,
    /// The verbatim probe result, when exercised (`applied`/
    /// `not_applied`/`undeterminable`/`error:…` — a fact, never hidden).
    pub probe: Option<String>,
    /// The verbatim backend the executor reported (`container` expected).
    pub executor_backend: Option<String>,
    /// Setup retries consumed (≤1 bound).
    pub setup_retries: u32,
    /// The verbatim setup/drive failure, when one occurred.
    pub failure: Option<String>,
}

/// `TrialOutcome` for the packet's outcome field — the closed
/// vocabulary the report's tally counts.
pub fn outcome_of(v: &CellVerdict) -> TrialOutcome {
    match v {
        CellVerdict::Pass => TrialOutcome::Pass,
        CellVerdict::DefectDetected { .. } => TrialOutcome::DefectDetected,
        CellVerdict::Failed(_) => TrialOutcome::Failed,
    }
}

/// The per-cell leg verdict over its three retained trials. `expected`
/// is the outcome the cell's spec row demands (`pass` for O1/O3–O7,
/// `defect_detected` for O8; O2's retained classification is
/// `unknown`/`resolved_*` — verified on any non-failed Kp4 outcome).
pub fn evaluate_cell(cell: OpsCell, trials: &[TrialOutcomeRecord]) -> LegVerdict {
    if trials.is_empty() {
        return LegVerdict::Inconclusive(format!("cell_not_attempted:{}", cell.as_str()));
    }
    if trials.iter().any(|t| t.setup_retries > 1) {
        return LegVerdict::Refused(format!("retry_bound:{}", cell.as_str()));
    }
    for t in trials {
        if t.outcome == "unattempted" {
            return LegVerdict::Inconclusive(format!("{}:unattempted", t.trial_id));
        }
        let ok = match cell {
            OpsCell::O8 => t.outcome.starts_with("defect_detected"),
            OpsCell::O2 => {
                !matches!(
                    t.outcome.as_str(),
                    "failed" | "refusal" | "missing_observer" | "unreached_fault"
                ) && t.boundary_observed
            }
            _ => t.outcome == TrialOutcome::Pass.as_str(),
        };
        if !ok {
            if t.invoked {
                return LegVerdict::Refused(format!("{}:{}", t.trial_id, t.outcome));
            }
            return LegVerdict::Inconclusive(format!("{}:{}", t.trial_id, t.outcome));
        }
    }
    LegVerdict::Verified
}

/// The O2 Kp4 outcome → the retained packet spelling (classification,
/// never a pass).
pub fn kp4_outcome_str(o: &Kp4Outcome) -> String {
    match o {
        Kp4Outcome::UnknownUnprobed => "unknown_unprobed".to_string(),
        Kp4Outcome::UnknownUndeterminable => "unknown_undeterminable".to_string(),
        Kp4Outcome::ResolvedApplied => "resolved_applied".to_string(),
        Kp4Outcome::ResolvedNotApplied => "resolved_not_applied".to_string(),
        Kp4Outcome::Failed(f) => format!("failed:{}", f.as_str()),
    }
}

/// The repairs leg's supplied facts — the two discharged predicates'
/// verbatim evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairsFacts {
    /// A podman-spawned helper + policy hello reported `container`.
    pub backend_honest: bool,
    /// An exec ran inside the podman cell post-hello (`/work` visible).
    pub exec_inside_container: bool,
    /// `probe()` reached the journal and answered a typed verdict on the
    /// live session channel.
    pub probe_live_channel: bool,
    /// `probe()` reached the journal on the severed-channel reconnect.
    pub probe_reconnect: bool,
}

/// `evaluate_repairs` — both carried predicates discharged for real.
pub fn evaluate_repairs(r: &RepairsFacts) -> LegVerdict {
    if !r.backend_honest || !r.exec_inside_container {
        return LegVerdict::Refused(EXEC_BACKEND_CLOBBER.to_string());
    }
    if !r.probe_live_channel || !r.probe_reconnect {
        return LegVerdict::Refused(PROBE_CARRIED.to_string());
    }
    LegVerdict::Verified
}

/// The `probe_verb_enoent` predicate (verbatim — the connect placeholder
/// spawn's ENOENT on this host).
pub const PROBE_CARRIED: &str = "probe_verb_enoent";
/// The `exec_backend_hello_clobber` predicate (verbatim — the policy
/// hello's `seatbelt` re-derivation over the spawned podman backend).
pub const EXEC_BACKEND_CLOBBER: &str = "exec_backend_hello_clobber";

/// The packet disposition — `executed` only when every leg verified.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg verified — the bounded S4 matrix ran and retained.
    Executed,
    /// At least one `failed`/`refused`/`unsupported` leg.
    Failed,
    /// At least one `inconclusive` leg, none failed — undetermined.
    Inconclusive,
}

impl Disposition {
    /// The closed vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            Disposition::Executed => "executed",
            Disposition::Failed => "failed",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-ops-live/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct OpsLiveReport {
    /// `H(canonical(record minus packet_id))` under `ops_live`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The eleven legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The discharged carried predicates, verbatim.
    pub discharged_predicates: Vec<String>,
    /// Every retained trial/setup attempt identity, in order.
    pub attempts: Vec<String>,
    /// The frozen permutation seed.
    pub seed: u64,
    /// The admission field — always `bounded_s4_execution`.
    pub admission: String,
    /// The `OpsReport::render()` payload (stage counts, ratios, the
    /// missingness bound, `o8_detector`, `acceptance_blocked`, `stopped`).
    pub stage_report: Json,
}

impl OpsLiveReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("disposition", Json::str(self.disposition.name())),
            ("seed", Json::Int(self.seed as i64)),
            (
                "discharged_predicates",
                Json::Arr(
                    self.discharged_predicates
                        .iter()
                        .map(|s| Json::str(s))
                        .collect(),
                ),
            ),
            ("admission", Json::str(&self.admission)),
            (
                "attempts",
                Json::Arr(self.attempts.iter().map(|s| Json::str(s)).collect()),
            ),
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
            ("stage_report", self.stage_report.clone()),
        ])
    }
}

/// `assemble(legs, attempts, seed, stage_report)` — fold the leg
/// verdicts into the disposition and mint `packet_id` last over the
/// canonical record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a refused assembly.
pub fn assemble(
    legs: Vec<Leg>,
    attempts: Vec<String>,
    seed: u64,
    stage_report: Json,
) -> Result<OpsLiveReport, String> {
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
        Disposition::Failed
    } else if undetermined {
        Disposition::Inconclusive
    } else {
        Disposition::Executed
    };
    let mut out = OpsLiveReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        legs,
        discharged_predicates: DISCHARGED.iter().map(|s| s.to_string()).collect(),
        attempts,
        seed,
        admission: "bounded_s4_execution".to_string(),
        stage_report,
        disposition,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
