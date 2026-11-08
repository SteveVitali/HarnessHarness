//! `continuation` — the first-pass cost-only continuation rules
//! (manifest 283/R6.29, Stage 1; spec §10.1.4 windows table + arm
//! priority + projection + wall time + firewall, §10.1.1 root cap,
//! §10.1.5 binding priors + D_max, §10.1.2 arms, §10.1.6 adaptations,
//! §6.6 stop rules; REQ-XH-112, REQ-XH-117, REQ-XH-126, REQ-XD-040
//! absorbing REQ-XC-211's literal predicate, REQ-XD-042; contributing
//! REQ-XC-322, REQ-XD-041/XH-111).
//!
//! Machinery only — values fill at FP-E (R6.46) and the rules run in
//! W4 (R6.47's fleshing). Every rule reads cost and infrastructure
//! fields **only** (X-07): the input manifest's strict decode refuses
//! any outcome member typed, and every drop/inclusion decision carries
//! the pinned label [`DROP_LABEL`] ("cost-triggered; cost correlates
//! with failure length") because cost is outcome-correlated on TB 2.1.
//!
//! One projection formula serves FP-E and every W4 round-boundary
//! checkpoint (CC1/X-04): p̂_h = 1.2 × max(1.5 × mean_h, P90_h) over
//! ≥ [`MIN_MEASURED`] measured trials else the §10.1.5 binding prior;
//! the step fits when Σ_h (remaining_h × p̂_h) ≤ root − consumed −
//! Σ orphaned calls at `bounded.hi` − C × s (REQ-XC-126's in-flight
//! slice headroom). t̂_h = 1.1 × measured mean wall (≥
//! [`MIN_WALL_MEASURED`]) else the 1,040 s prior; the HH-native lane
//! arms use original mini-swe-agent's t̂ (§10.1.4).
//!
//! The arm-priority order and protection set are **parameters** read
//! from the pre-registration record ([`ArmPriority`]) — Q-X61's
//! recorded default is a fixture constant, never a hard-coded
//! decision. §6.6's infra thresholds are written for PP0/PP1; §10.1.6
//! applies them "unchanged", read here as: W4's arms play the PP1
//! role (> 20 % infra after ≥ 10 trials stops that arm), PP0's ≥ 1
//! of 3 holds that harness (REQ-XH-127's disposition, Stage 2), and
//! ≥ 3 stopped arms stop the pass. Recorded reading, contract Notes.

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::admission::{CLAUDE_CODE, MINI_SWE_AGENT};
use crate::disposition::{AdmissionReason, Disposition, WithheldReason};
use crate::json_ext::JsonExt;

/// `hh-fp-continuation-input/1` — the cost-only input manifest.
pub const INPUT_SCHEMA: &str = "hh-fp-continuation-input/1";
/// `hh-fp-checkpoint/1` — the automatic round-boundary readout
/// (FP-E uses the same record with `scope: "fp_e"`).
pub const CHECKPOINT_SCHEMA: &str = "hh-fp-checkpoint/1";
/// `hh-fp-arm-caps/1` — the µ$ cap formula table (REQ-XH-117).
pub const CAP_TABLE_SCHEMA: &str = "hh-fp-arm-caps/1";
/// The pinned label every drop/inclusion decision carries verbatim —
/// cost is outcome-correlated; never paraphrased (X-07/X-08).
pub const DROP_LABEL: &str = "cost-triggered; cost correlates with failure length";
/// ≥ 10 measured trials before p̂_h leaves the prior (§10.1.4).
pub const MIN_MEASURED: usize = 10;
/// ≥ 3 measured trials before t̂_h leaves the prior (§10.1.4).
pub const MIN_WALL_MEASURED: usize = 3;
/// The wall-time prior per trial: 1,040 s (§10.1.4/§10.1.5).
pub const T_PRIOR_MS: i64 = 1_040_000;
/// The first-pass compute line: Σ ceilings ≤ $20 (REQ-XH-118).
pub const FP_COMPUTE_CAP_MICROS: i64 = 20_000_000;
/// Milliseconds per hour for D_max arithmetic.
pub const MS_PER_HOUR: i64 = 3_600_000;
/// Milliseconds per day for the Q-X62 trigger arithmetic.
pub const MS_PER_DAY: i64 = 86_400_000;

/// The member names that make an input an outcome field — the
/// REQ-XH-121 sealed set (slot verdict, reward, `ctrf` counts, the
/// `executed{…}` disposition members, verifier output and every
/// derivative). The continuation manifest refuses any of these at
/// any nesting level; a member outside the declared set fails the
/// strict decode separately.
pub const OUTCOME_FIELDS: [&str; 11] = [
    "verdict",
    "reward",
    "ctrf",
    "ctrf_counts",
    "executed",
    "resolved",
    "unresolved",
    "verifier_output",
    "verifier",
    "outcome",
    "score",
];

// ── errors ───────────────────────────────────────────────────────────

/// A typed continuation refusal (X-02: mechanical, never silent).
#[derive(Debug, Clone, PartialEq)]
pub enum ContinuationError {
    /// An outcome field reached a continuation input — refused, never
    /// filtered (the firewall is a refusal, not a projection).
    OutcomeField {
        /// The dotted path of the refused member.
        path: String,
    },
    /// A schema or member-shape failure on strict decode.
    Schema {
        /// The verbatim detail.
        detail: String,
    },
    /// A declared priority is not a partition of the five arms.
    Priority {
        /// The verbatim detail.
        detail: String,
    },
    /// A stop trigger outside §6.6's four kinds — there is no
    /// outcome-based trigger to configure (REQ-XD-042).
    UnsupportedTrigger {
        /// The verbatim detail.
        detail: String,
    },
}

impl std::fmt::Display for ContinuationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContinuationError::OutcomeField { path } => write!(
                f,
                "refused{{outcome_field}}: {path} — continuation inputs carry cost fields only (REQ-XH-112)"
            ),
            ContinuationError::Schema { detail } => {
                write!(f, "continuation_input: {detail}")
            }
            ContinuationError::Priority { detail } => {
                write!(f, "arm_priority: {detail}")
            }
            ContinuationError::UnsupportedTrigger { detail } => {
                write!(f, "unsupported{{stop_trigger}}: {detail}")
            }
        }
    }
}
impl std::error::Error for ContinuationError {}

fn schema(detail: impl Into<String>) -> ContinuationError {
    ContinuationError::Schema {
        detail: detail.into(),
    }
}

/// `check_cost_only(j)` — the firewall scan: any member named in
/// [`OUTCOME_FIELDS`] at any nesting level refuses [`OutcomeField`].
/// Also refuses a `disposition`-shaped member whose value opens an
/// `executed{…}` outcome member — `inconclusive{infra:*}` is an infra
/// field and passes (REQ-XH-121's carve-out).
pub fn check_cost_only(j: &Json) -> Result<(), ContinuationError> {
    check_cost_only_at(j, "")
}

fn check_cost_only_at(j: &Json, path: &str) -> Result<(), ContinuationError> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if OUTCOME_FIELDS.contains(&k.as_str()) {
                    return Err(ContinuationError::OutcomeField { path: p });
                }
                if k == "disposition" {
                    if let Some(s) = v.as_str() {
                        if s.starts_with("executed{") {
                            return Err(ContinuationError::OutcomeField { path: p });
                        }
                    }
                }
                check_cost_only_at(v, &p)?;
            }
            Ok(())
        }
        Json::Arr(a) => {
            for (i, v) in a.iter().enumerate() {
                check_cost_only_at(v, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ── arms and priority ────────────────────────────────────────────────

/// The five first-pass arms (§10.1.2's arms table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FpArm {
    /// `mini-swe-agent` — the original, hosted; protected.
    OrigMini,
    /// `hh_native` — mini-swe-agent compiled to HIR; protected
    /// (`carried::ArmLane`'s spelling).
    HhNative,
    /// `pi`.
    Pi,
    /// `claude-code`.
    ClaudeCode,
    /// `v5` — the HH-native + V5 `rsn-drop` arm (Q-X56 recorded level).
    V5,
}

impl FpArm {
    /// Every member, in a fixed order.
    pub const ALL: [FpArm; 5] = [
        FpArm::OrigMini,
        FpArm::HhNative,
        FpArm::Pi,
        FpArm::ClaudeCode,
        FpArm::V5,
    ];

    /// The canonical spelling (the runner's harness ids; the native
    /// lane arms use `carried::ArmLane`'s spelling).
    pub fn as_str(self) -> &'static str {
        match self {
            FpArm::OrigMini => MINI_SWE_AGENT,
            FpArm::HhNative => "hh_native",
            FpArm::Pi => "pi",
            FpArm::ClaudeCode => CLAUDE_CODE,
            FpArm::V5 => "v5",
        }
    }

    /// The strict parse; `None` outside the five arms.
    pub fn parse(s: &str) -> Option<FpArm> {
        match s {
            "mini-swe-agent" | "hh_native" | "hh-native" | "pi" | "claude-code" | "v5" => {
                Some(match s {
                    "mini-swe-agent" => FpArm::OrigMini,
                    "hh_native" | "hh-native" => FpArm::HhNative,
                    "pi" => FpArm::Pi,
                    "claude-code" => FpArm::ClaudeCode,
                    _ => FpArm::V5,
                })
            }
            _ => None,
        }
    }

    /// The native-lane arms ride the HH-native lane — their t̂ is
    /// original mini-swe-agent's (§10.1.4) and they carry the H set.
    pub fn native_lane(self) -> bool {
        matches!(self, FpArm::HhNative | FpArm::V5)
    }

    /// The hosted arms — the fallback's phase-2-only set (REQ-XH-123)
    /// and the PP0 smoke population (§10.1.4 W2(b)).
    pub fn hosted(self) -> bool {
        matches!(self, FpArm::OrigMini | FpArm::Pi | FpArm::ClaudeCode)
    }
}

/// The arm-priority declaration — the pre-registration's protected
/// set and truncation order (Q-X61 recorded at FP-S; a parameter,
/// never a hard-coded decision). `protected` ∪ `drop_order` must be
/// a partition of the five arms.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmPriority {
    /// Arms exempt from cost-only dropping while an unprotected arm
    /// remains.
    pub protected: Vec<FpArm>,
    /// The truncation order over the unprotected arms — the first
    /// member is dropped first.
    pub drop_order: Vec<FpArm>,
}

impl ArmPriority {
    /// Q-X61's *recorded default* (pending at FP-S): protected
    /// {original, HH-native mini-swe-agent}; truncation V5 → Claude
    /// Code → Pi. A fixture constant — the decided value arrives in
    /// the pre-registration record.
    pub fn qx61_recorded_default() -> ArmPriority {
        ArmPriority {
            protected: vec![FpArm::OrigMini, FpArm::HhNative],
            drop_order: vec![FpArm::V5, FpArm::ClaudeCode, FpArm::Pi],
        }
    }

    /// Validate the partition: every arm exactly once across the two
    /// sets.
    pub fn validate(&self) -> Result<(), ContinuationError> {
        let mut seen = Vec::new();
        for a in self.protected.iter().chain(self.drop_order.iter()) {
            if seen.contains(a) {
                return Err(ContinuationError::Priority {
                    detail: format!("{} appears twice", a.as_str()),
                });
            }
            seen.push(*a);
        }
        for a in FpArm::ALL {
            if !seen.contains(&a) {
                return Err(ContinuationError::Priority {
                    detail: format!("{} is neither protected nor in the drop order", a.as_str()),
                });
            }
        }
        if self.protected.is_empty() {
            return Err(ContinuationError::Priority {
                detail: "no protected arm declared".to_string(),
            });
        }
        Ok(())
    }

    /// Strict decode: `{protected: [arm…], drop_order: [arm…]}`.
    pub fn from_json(j: &Json) -> Result<ArmPriority, ContinuationError> {
        let m = j
            .as_obj()
            .ok_or_else(|| schema("arm_priority must be an object"))?;
        let arms = |key: &str| -> Result<Vec<FpArm>, ContinuationError> {
            let arr = m
                .get(key)
                .and_then(Json::as_arr)
                .ok_or_else(|| schema(format!("arm_priority.{key} must be an array")))?;
            arr.iter()
                .map(|v| {
                    v.as_str()
                        .and_then(FpArm::parse)
                        .ok_or_else(|| schema(format!("arm_priority.{key}: {v:?} is not an arm")))
                })
                .collect()
        };
        let p = ArmPriority {
            protected: arms("protected")?,
            drop_order: arms("drop_order")?,
        };
        p.validate()?;
        Ok(p)
    }

    /// `true` when the arm is in the protected set.
    pub fn is_protected(&self, arm: FpArm) -> bool {
        self.protected.contains(&arm)
    }
}

// ── the input manifest ───────────────────────────────────────────────

/// One measured trial's cost/infrastructure fields — the only fields
/// a continuation rule may read (§10.1.4's cost-field list).
#[derive(Debug, Clone, PartialEq)]
pub struct TrialMeasure {
    /// The slot record ref.
    pub slot: String,
    /// Metered cost in integer µ$.
    pub cost_micros: i64,
    /// Wall time in ms — a cost field under the same firewall.
    pub wall_ms: i64,
    /// `queue_wait_ms` — the checkpoint p99 report input (REQ-XC-322).
    pub queue_wait_ms: Option<i64>,
    /// `latency_ms` — the checkpoint p99 report input (REQ-XC-322).
    pub latency_ms: Option<i64>,
    /// `inconclusive{infra:*}` — an infra member, never an outcome
    /// (REQ-XH-121: holds and stops may read it).
    pub infra_failed: bool,
}

impl TrialMeasure {
    /// Strict decode of one measured-trial row.
    pub fn from_json(j: &Json) -> Result<TrialMeasure, ContinuationError> {
        let m = j
            .as_obj()
            .ok_or_else(|| schema("measured[] must be an object"))?;
        for k in m.keys() {
            if ![
                "slot",
                "cost_micros",
                "wall_ms",
                "queue_wait_ms",
                "latency_ms",
                "infra_failed",
            ]
            .contains(&k.as_str())
            {
                return Err(schema(format!("measured[].{k} is not a cost field")));
            }
        }
        Ok(TrialMeasure {
            slot: m
                .get("slot")
                .and_then(Json::as_str)
                .ok_or_else(|| schema("measured[].slot required"))?
                .to_string(),
            cost_micros: m
                .get("cost_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("measured[].cost_micros required"))?,
            wall_ms: m
                .get("wall_ms")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("measured[].wall_ms required"))?,
            queue_wait_ms: m.get("queue_wait_ms").and_then(Json::as_int),
            latency_ms: m.get("latency_ms").and_then(Json::as_int),
            infra_failed: m
                .get("infra_failed")
                .and_then(Json::as_bool)
                .unwrap_or(false),
        })
    }
}

/// The scope the rule runs in (§10.1.4: W2 measurements feed FP-E;
/// W4 measurements feed each round-boundary checkpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The FP-E inclusion decision (input scope W2).
    FpE,
    /// A W4 round-boundary checkpoint (after round 2, then each
    /// later boundary).
    Checkpoint {
        /// The round boundary the checkpoint follows.
        after_round: i64,
    },
}

impl Scope {
    /// The recorded spelling.
    pub fn as_str(self) -> String {
        match self {
            Scope::FpE => "fp_e".to_string(),
            Scope::Checkpoint { after_round } => format!("w4_round_{after_round}"),
        }
    }
}

/// The cost-only continuation input — strict decode of
/// `hh-fp-continuation-input/1`. The firewall scan runs first: an
/// outcome member refuses [`OutcomeField`] before any shape check.
#[derive(Debug, Clone)]
pub struct ContinuationInput {
    /// The decision scope.
    pub scope: Scope,
    /// Measured trials per arm (cost fields only).
    pub measured: BTreeMap<FpArm, Vec<TrialMeasure>>,
    /// Remaining planned slot ids per arm (the truncation unit).
    pub remaining: BTreeMap<FpArm, Vec<String>>,
    /// Each arm's finished slots' metered spend (µ$) — the expected-
    /// cost admission's `consumed_h` (REQ-XH-117).
    pub consumed_by_arm: BTreeMap<FpArm, i64>,
    /// In-flight slots per arm: `(slot, consumed-to-date µ$)`.
    pub in_flight: BTreeMap<FpArm, Vec<(String, i64)>>,
    /// Spend to date under the window's root (µ$).
    pub consumed_micros: i64,
    /// Σ orphaned calls at `bounded.hi` (µ$).
    pub orphaned_hi_micros: i64,
    /// The window's root cap (µ$) — REQ-XH-005's carried pool.
    pub root_cap_micros: i64,
    /// C — the in-flight bound (REQ-XH-118).
    pub c: i64,
    /// s — the slot slice (µ$; REQ-XC-126), reserved against the root
    /// only.
    pub slice_micros: i64,
    /// The §10.1.5 binding planning prior per arm (µ$/trial).
    pub priors_micros: BTreeMap<FpArm, i64>,
    /// Admitted client timeouts (`name → ms`) for the p99 report.
    pub client_timeouts_ms: BTreeMap<String, i64>,
    /// The declared arm priority.
    pub priority: ArmPriority,
}

impl ContinuationInput {
    /// Strict decode; outcome members refuse before shape checks.
    pub fn from_json(j: &Json) -> Result<ContinuationInput, ContinuationError> {
        check_cost_only(j)?;
        let m = j
            .as_obj()
            .ok_or_else(|| schema("input must be an object"))?;
        for k in m.keys() {
            if ![
                "schema",
                "scope",
                "after_round",
                "measured",
                "remaining",
                "consumed_by_arm",
                "in_flight",
                "consumed_micros",
                "orphaned_hi_micros",
                "root_cap_micros",
                "c",
                "slice_micros",
                "priors_micros",
                "client_timeouts_ms",
                "priority",
            ]
            .contains(&k.as_str())
            {
                return Err(schema(format!("{k} is not a declared member")));
            }
        }
        if m.get("schema").and_then(Json::as_str) != Some(INPUT_SCHEMA) {
            return Err(schema(format!("schema must be {INPUT_SCHEMA}")));
        }
        let scope = match m.get("scope").and_then(Json::as_str) {
            Some("fp_e") => Scope::FpE,
            Some("w4_checkpoint") => Scope::Checkpoint {
                after_round: m
                    .get("after_round")
                    .and_then(Json::as_int)
                    .ok_or_else(|| schema("after_round required at a W4 checkpoint"))?,
            },
            other => return Err(schema(format!("scope {other:?} unknown"))),
        };
        let arm_map = |key: &str| -> Result<BTreeMap<FpArm, Vec<TrialMeasure>>, ContinuationError> {
            let mut out = BTreeMap::new();
            let mm = m
                .get(key)
                .and_then(Json::as_obj)
                .ok_or_else(|| schema(format!("{key} must be an object")))?;
            for (a, v) in mm {
                let arm =
                    FpArm::parse(a).ok_or_else(|| schema(format!("{key}.{a} is not an arm")))?;
                let rows = v
                    .as_arr()
                    .ok_or_else(|| schema(format!("{key}.{a} must be an array")))?;
                out.insert(
                    arm,
                    rows.iter()
                        .map(TrialMeasure::from_json)
                        .collect::<Result<_, _>>()?,
                );
            }
            Ok(out)
        };
        let slot_map = |key: &str| -> Result<BTreeMap<FpArm, Vec<String>>, ContinuationError> {
            let mut out = BTreeMap::new();
            let mm = m
                .get(key)
                .and_then(Json::as_obj)
                .ok_or_else(|| schema(format!("{key} must be an object")))?;
            for (a, v) in mm {
                let arm =
                    FpArm::parse(a).ok_or_else(|| schema(format!("{key}.{a} is not an arm")))?;
                let rows = v
                    .as_arr()
                    .ok_or_else(|| schema(format!("{key}.{a} must be an array")))?;
                out.insert(
                    arm,
                    rows.iter()
                        .map(|s| {
                            s.as_str()
                                .map(str::to_string)
                                .ok_or_else(|| schema(format!("{key}.{a}[] must be strings")))
                        })
                        .collect::<Result<_, _>>()?,
                );
            }
            Ok(out)
        };
        let int_map = |key: &str| -> Result<BTreeMap<FpArm, i64>, ContinuationError> {
            let mut out = BTreeMap::new();
            let mm = m
                .get(key)
                .and_then(Json::as_obj)
                .ok_or_else(|| schema(format!("{key} must be an object")))?;
            for (a, v) in mm {
                let arm =
                    FpArm::parse(a).ok_or_else(|| schema(format!("{key}.{a} is not an arm")))?;
                out.insert(
                    arm,
                    v.as_int()
                        .ok_or_else(|| schema(format!("{key}.{a} must be an integer")))?,
                );
            }
            Ok(out)
        };
        let mut in_flight = BTreeMap::new();
        let fl = m
            .get("in_flight")
            .and_then(Json::as_obj)
            .ok_or_else(|| schema("in_flight must be an object"))?;
        for (a, v) in fl {
            let arm =
                FpArm::parse(a).ok_or_else(|| schema(format!("in_flight.{a} is not an arm")))?;
            let rows = v
                .as_arr()
                .ok_or_else(|| schema(format!("in_flight.{a} must be an array")))?;
            let mut rs = Vec::new();
            for r in rows {
                let rm = r
                    .as_obj()
                    .ok_or_else(|| schema(format!("in_flight.{a}[] must be objects")))?;
                rs.push((
                    rm.get("slot")
                        .and_then(Json::as_str)
                        .ok_or_else(|| schema(format!("in_flight.{a}[].slot required")))?
                        .to_string(),
                    rm.get("consumed_micros")
                        .and_then(Json::as_int)
                        .ok_or_else(|| {
                            schema(format!("in_flight.{a}[].consumed_micros required"))
                        })?,
                ));
            }
            in_flight.insert(arm, rs);
        }
        let mut client_timeouts_ms = BTreeMap::new();
        let ct = m
            .get("client_timeouts_ms")
            .and_then(Json::as_obj)
            .ok_or_else(|| schema("client_timeouts_ms must be an object"))?;
        for (k, v) in ct {
            client_timeouts_ms.insert(
                k.clone(),
                v.as_int()
                    .ok_or_else(|| schema(format!("client_timeouts_ms.{k} must be an integer")))?,
            );
        }
        Ok(ContinuationInput {
            scope,
            measured: arm_map("measured")?,
            remaining: slot_map("remaining")?,
            consumed_by_arm: int_map("consumed_by_arm")?,
            in_flight,
            consumed_micros: m
                .get("consumed_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("consumed_micros required"))?,
            orphaned_hi_micros: m
                .get("orphaned_hi_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("orphaned_hi_micros required"))?,
            root_cap_micros: m
                .get("root_cap_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("root_cap_micros required"))?,
            c: m.get("c")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("c required"))?,
            slice_micros: m
                .get("slice_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("slice_micros required"))?,
            priors_micros: int_map("priors_micros")?,
            client_timeouts_ms,
            priority: ArmPriority::from_json(
                m.get("priority")
                    .ok_or_else(|| schema("priority required"))?,
            )?,
        })
    }
}

// ── the one projection formula ───────────────────────────────────────

/// Ceiling division.
fn ceil_div(n: i64, d: i64) -> i64 {
    if d == 0 {
        return 0;
    }
    n.div_euclid(d) + i64::from(n.rem_euclid(d) != 0)
}

/// The nearest-rank percentile over sorted values (`pct` = 90, 99).
fn percentile(sorted: &[i64], pct: i64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len() as i64;
    let rank = ceil_div(pct * n, 100).clamp(1, n);
    sorted[(rank - 1) as usize]
}

/// `p_hat(arm, input)` — the one projection formula (§10.1.4): over
/// ≥ [`MIN_MEASURED`] measured trials, ceil(1.2 × max(ceil(1.5 ×
/// mean), P90)); otherwise the binding planning prior. Integer µ$
/// throughout (R6.7's round-up-per-line).
pub fn p_hat(arm: FpArm, input: &ContinuationInput) -> i64 {
    let measured = input
        .measured
        .get(&arm)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if measured.len() >= MIN_MEASURED {
        let sum: i64 = measured.iter().map(|t| t.cost_micros).sum();
        let n = measured.len() as i64;
        let mean_15 = ceil_div(3 * sum, 2 * n); // ceil(1.5 × mean) without the fractional mean
        let mut costs: Vec<i64> = measured.iter().map(|t| t.cost_micros).collect();
        costs.sort();
        let p90 = percentile(&costs, 90);
        ceil_div(6 * mean_15.max(p90), 5) // ceil(1.2 × max(…))
    } else {
        *input.priors_micros.get(&arm).unwrap_or(&0)
    }
}

/// `t_hat(arm, input)` — the wall-time projection (§10.1.4): over ≥
/// [`MIN_WALL_MEASURED`] measured trials, ceil(1.1 × mean wall);
/// otherwise the 1,040 s prior. The native-lane arms always take
/// original mini-swe-agent's t̂.
pub fn t_hat(arm: FpArm, input: &ContinuationInput) -> i64 {
    let arm = if arm.native_lane() {
        FpArm::OrigMini
    } else {
        arm
    };
    let measured = input
        .measured
        .get(&arm)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if measured.len() >= MIN_WALL_MEASURED {
        let sum: i64 = measured.iter().map(|t| t.wall_ms).sum();
        ceil_div(11 * sum, 10 * measured.len() as i64)
    } else {
        T_PRIOR_MS
    }
}

/// The step-fit test (§10.1.4): `Σ_h remaining_h × p̂_h` on the left;
/// `root − consumed − Σ orphaned at bounded.hi − C × s` on the right.
/// Returns `(lhs, rhs)`; the step fits when `lhs ≤ rhs`.
pub fn cost_fit(input: &ContinuationInput, remaining: &BTreeMap<FpArm, Vec<String>>) -> (i64, i64) {
    let lhs: i64 = remaining
        .iter()
        .map(|(a, slots)| slots.len() as i64 * p_hat(*a, input))
        .sum();
    let rhs = input.root_cap_micros
        - input.consumed_micros
        - input.orphaned_hi_micros
        - input.c * input.slice_micros;
    (lhs, rhs)
}

// ── withhold actions and the drop loop ───────────────────────────────

/// One withholding decision — every entry carries [`DROP_LABEL`].
#[derive(Debug, Clone, PartialEq)]
pub struct WithholdAction {
    /// The arm whose remaining slots are withheld.
    pub arm: FpArm,
    /// The withheld slot ids.
    pub slots: Vec<String>,
    /// `withheld{…}` — `budget` for cost drops, `window_deadline` for
    /// wall-time drops (REQ-XH-112), `admission:<r>` for holds.
    pub reason: WithheldReason,
    /// [`DROP_LABEL`] — verbatim.
    pub label: &'static str,
}

impl WithholdAction {
    /// The canonical JSON row.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("arm", Json::str(self.arm.as_str())),
            (
                "disposition",
                Json::str(
                    Disposition::Withheld {
                        reason: self.reason,
                    }
                    .as_str(),
                ),
            ),
            (
                "slots",
                Json::Arr(self.slots.iter().map(Json::str).collect()),
            ),
            ("label", Json::str(self.label)),
        ])
    }
}

/// A protected-pair truncation: every remaining slot withheld, and
/// P2-FP / P3-L2 / P4-FP refuse primary inference (k < 5, R < 5)
/// with typed reasons (REQ-XH-112/117).
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceRefusal {
    /// The claim.
    pub claim: &'static str,
    /// The typed reason.
    pub reason: String,
}

impl InferenceRefusal {
    /// The canonical JSON row.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("claim", Json::str(self.claim)),
            ("refusal", Json::str(&self.reason)),
        ])
    }
}

/// The drop loop shared by the cost and wall-time rules (CC1 — one
/// order, one procedure): while the fit predicate fails, withhold the
/// remaining slots of the lowest-priority unprotected arm that still
/// has any. When no unprotected arm remains and the step still does
/// not fit, the protected set itself is truncated: every remaining
/// slot is withheld and the three claims refuse primary inference.
pub fn apply_drop_order(
    input: &ContinuationInput,
    mut remaining: BTreeMap<FpArm, Vec<String>>,
    reason: WithheldReason,
    fits: impl Fn(&BTreeMap<FpArm, Vec<String>>) -> bool,
) -> (
    Vec<WithholdAction>,
    Vec<InferenceRefusal>,
    BTreeMap<FpArm, Vec<String>>,
) {
    let mut actions = Vec::new();
    let mut refusals = Vec::new();
    while !fits(&remaining) {
        let victim = input
            .priority
            .drop_order
            .iter()
            .find(|a| remaining.get(*a).map(|s| !s.is_empty()).unwrap_or(false))
            .copied();
        match victim {
            Some(arm) => {
                let slots = remaining.remove(&arm).unwrap_or_default();
                if !slots.is_empty() {
                    actions.push(WithholdAction {
                        arm,
                        slots,
                        reason,
                        label: DROP_LABEL,
                    });
                }
            }
            None => {
                // The protected set itself no longer fits — every
                // remaining slot is withheld (never a partial pair:
                // protected members truncate together).
                for (arm, slots) in std::mem::take(&mut remaining) {
                    if !slots.is_empty() {
                        actions.push(WithholdAction {
                            arm,
                            slots,
                            reason,
                            label: DROP_LABEL,
                        });
                    }
                }
                let w = Disposition::Withheld { reason }.as_str();
                for claim in ["P2-FP", "P3-L2", "P4-FP"] {
                    refusals.push(InferenceRefusal {
                        claim,
                        reason: format!("{w}:protected_pair_truncated{{k<5,R<5}}"),
                    });
                }
                break;
            }
        }
    }
    (actions, refusals, remaining)
}

// ── the checkpoint ───────────────────────────────────────────────────

/// The wall-time decision inputs at a checkpoint or FP-E (declared —
/// never derived inside the rule).
#[derive(Debug, Clone, PartialEq)]
pub struct WallInput {
    /// The window's dead-man D in ms (at FP-E: the `d_max` recompute's
    /// output; at a boundary: the running window's remaining D budget
    /// measured against elapsed).
    pub d_limit_ms: i64,
    /// The REQ-XB-128 admission cut-off in ms (≈ 0.65 h in W4).
    pub cutoff_ms: i64,
    /// The declared per-window overhead in ms.
    pub overhead_ms: i64,
    /// The carried-artifact load in ms (REQ-XC-330; measured by W2).
    pub load_ms: i64,
    /// The O/H pairs the remaining slots form — the pair-atomicity
    /// input (empty for a singleton-arm scope).
    pub pairs: Vec<OhPair>,
}

/// An O/H pair — admitted together or `withheld{window_deadline}`
/// together, never split (REQ-XD-111, REQ-XH-126(d)).
#[derive(Debug, Clone, PartialEq)]
pub struct OhPair {
    /// The O-side slot id.
    pub o_slot: String,
    /// The H-side slot id.
    pub h_slot: String,
}

/// `withhold_pair_deadline(pair)` — the pair-atomic cut-off
/// disposition: both slots `withheld{window_deadline}`, never one
/// (REQ-XH-126(d)).
pub fn withhold_pair_deadline(pair: &OhPair) -> Vec<(String, Disposition)> {
    vec![
        (
            pair.o_slot.clone(),
            Disposition::Withheld {
                reason: WithheldReason::WindowDeadline,
            },
        ),
        (
            pair.h_slot.clone(),
            Disposition::Withheld {
                reason: WithheldReason::WindowDeadline,
            },
        ),
    ]
}

/// `check_pair_closure(withheld, pairs)` — a withheld set that covers
/// exactly one side of a declared pair refuses: pairs are atomic.
pub fn check_pair_closure(
    withheld: &BTreeMap<String, ()>,
    pairs: &[OhPair],
) -> Result<(), ContinuationError> {
    for p in pairs {
        let o = withheld.contains_key(&p.o_slot);
        let h = withheld.contains_key(&p.h_slot);
        if o != h {
            return Err(schema(format!(
                "pair ({}, {}) split — a pair is admitted together or withheld together",
                p.o_slot, p.h_slot
            )));
        }
    }
    Ok(())
}

/// The retry allowance: ⌈0.1 × planned⌉ trials (REQ-XH-016).
pub fn retry_allowance(slots: i64) -> i64 {
    ceil_div(slots, 10)
}

/// `w_p_ms(remaining, t_hats, c, overhead, load)` — the wall-time
/// projection (REQ-XH-126(a)): `Σ_h (remaining_h + ⌈0.1 rem⌉) × t̂_h
/// / C + overhead + load`, ceiling at the division.
pub fn w_p_ms(
    remaining: &BTreeMap<FpArm, Vec<String>>,
    input: &ContinuationInput,
    wall: &WallInput,
) -> i64 {
    let numerator: i64 = remaining
        .iter()
        .map(|(a, slots)| {
            let n = slots.len() as i64;
            (n + retry_allowance(n)) * t_hat(*a, input)
        })
        .sum();
    ceil_div(numerator, input.c) + wall.overhead_ms + wall.load_ms
}

/// One p99 report row — `queue_wait + latency` p99 against an
/// admitted client timeout (REQ-XC-322 leg; cost fields only).
#[derive(Debug, Clone, PartialEq)]
pub struct P99Row {
    /// The timeout's declared name.
    pub timeout: String,
    /// The admitted bound (ms).
    pub bound_ms: i64,
    /// The measured p99 (ms); `None` when nothing measured.
    pub p99_ms: Option<i64>,
    /// `p99 ≤ bound`.
    pub within: Option<bool>,
}

/// `p99_report(measured, timeouts)` — across every measured trial in
/// scope, p99 of `queue_wait_ms + latency_ms` against each admitted
/// client timeout (the W4 checkpoint readout's REQ-XC-322 member).
pub fn p99_report(
    measured: &BTreeMap<FpArm, Vec<TrialMeasure>>,
    timeouts: &BTreeMap<String, i64>,
) -> Vec<P99Row> {
    let mut samples: Vec<i64> = Vec::new();
    for rows in measured.values() {
        for t in rows {
            match (t.queue_wait_ms, t.latency_ms) {
                (Some(q), Some(l)) => samples.push(q + l),
                _ => {}
            }
        }
    }
    samples.sort();
    timeouts
        .iter()
        .map(|(name, bound)| {
            let p99 = if samples.is_empty() {
                None
            } else {
                Some(percentile(&samples, 99))
            };
            P99Row {
                timeout: name.clone(),
                bound_ms: *bound,
                p99_ms: p99,
                within: p99.map(|p| p <= *bound),
            }
        })
        .collect()
}

/// The checkpoint outcome — projections, the two fit reports, the
/// withhold actions and any protected-pair inference refusals.
#[derive(Debug, Clone)]
pub struct CheckpointOutcome {
    /// The decision scope.
    pub scope: Scope,
    /// `arm → p̂_h` (µ$/trial, integer).
    pub p_hats: BTreeMap<FpArm, i64>,
    /// `arm → t̂_h` (ms/trial).
    pub t_hats: BTreeMap<FpArm, i64>,
    /// `(lhs, rhs)` of the cost step-fit after drops.
    pub cost_fit: (i64, i64),
    /// `(W_p, D_limit − cut-off)` after drops, when a wall input ran.
    pub wall_fit: Option<(i64, i64)>,
    /// Every withholding (budget first, then window_deadline).
    pub withheld: Vec<WithholdAction>,
    /// Protected-pair inference refusals.
    pub inference_refusals: Vec<InferenceRefusal>,
    /// The slots still admitted per arm.
    pub continuing: BTreeMap<FpArm, Vec<String>>,
    /// The REQ-XC-322 p99 rows.
    pub p99: Vec<P99Row>,
}

impl CheckpointOutcome {
    /// The `hh-fp-checkpoint/1` readout — cost fields only by
    /// construction; [`check_cost_only`] over the emitted record is
    /// the battery's second pair of eyes.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(CHECKPOINT_SCHEMA)),
            ("scope", Json::str(self.scope.as_str())),
            (
                "projection",
                Json::Obj(
                    self.p_hats
                        .iter()
                        .map(|(a, p)| {
                            (
                                a.as_str().to_string(),
                                Json::obj([
                                    ("p_hat_micros", Json::Int(*p)),
                                    ("t_hat_ms", Json::Int(*self.t_hats.get(a).unwrap_or(&0))),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ),
            (
                "cost_fit",
                Json::obj([
                    ("lhs_micros", Json::Int(self.cost_fit.0)),
                    ("rhs_micros", Json::Int(self.cost_fit.1)),
                    ("fits", Json::Bool(self.cost_fit.0 <= self.cost_fit.1)),
                ]),
            ),
            (
                "wall_fit",
                match self.wall_fit {
                    Some((wp, room)) => Json::obj([
                        ("w_p_ms", Json::Int(wp)),
                        ("room_ms", Json::Int(room)),
                        ("fits", Json::Bool(wp <= room)),
                    ]),
                    None => Json::Null,
                },
            ),
            (
                "withheld",
                Json::Arr(self.withheld.iter().map(WithholdAction::to_json).collect()),
            ),
            (
                "inference_refusals",
                Json::Arr(
                    self.inference_refusals
                        .iter()
                        .map(InferenceRefusal::to_json)
                        .collect(),
                ),
            ),
            (
                "continuing",
                Json::Obj(
                    self.continuing
                        .iter()
                        .map(|(a, s)| {
                            (
                                a.as_str().to_string(),
                                Json::Arr(s.iter().map(Json::str).collect()),
                            )
                        })
                        .collect(),
                ),
            ),
            (
                "p99",
                Json::Arr(
                    self.p99
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("timeout", Json::str(&r.timeout)),
                                ("bound_ms", Json::Int(r.bound_ms)),
                                ("p99_ms", r.p99_ms.map(Json::Int).unwrap_or(Json::Null)),
                                ("within", r.within.map(Json::Bool).unwrap_or(Json::Null)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("label", Json::str(DROP_LABEL)),
        ])
    }
}

/// `run_checkpoint(input, wall)` — the automatic round-boundary
/// checkpoint (REQ-XH-112): re-project from scope measurements, drop
/// the lowest-priority arms until the cost step fits (`withheld{
/// budget}`), then re-project wall time and drop by the same order
/// (`withheld{window_deadline}`). Rounds already run stay — the input
/// carries the remaining slots only. The same function serves FP-E
/// (`scope: fp_e`) and every later boundary — one formula (CC1).
pub fn run_checkpoint(
    input: &ContinuationInput,
    wall: Option<&WallInput>,
) -> Result<CheckpointOutcome, ContinuationError> {
    input.priority.validate()?;
    let p_hats: BTreeMap<FpArm, i64> = FpArm::ALL.iter().map(|a| (*a, p_hat(*a, input))).collect();
    let t_hats: BTreeMap<FpArm, i64> = FpArm::ALL.iter().map(|a| (*a, t_hat(*a, input))).collect();

    // Cost rule — withheld{budget} in the declared drop order.
    let (mut actions, mut refusals, mut remaining) = apply_drop_order(
        input,
        input.remaining.clone(),
        WithheldReason::Budget,
        |rem| {
            let lhs: i64 = rem
                .iter()
                .map(|(a, s)| s.len() as i64 * p_hats.get(a).copied().unwrap_or(0))
                .sum();
            lhs <= input.root_cap_micros
                - input.consumed_micros
                - input.orphaned_hi_micros
                - input.c * input.slice_micros
        },
    );
    // Wall rule — withheld{window_deadline} in the same order over
    // what the cost rule left.
    let mut wall_fit = None;
    if let Some(w) = wall {
        let room = w.d_limit_ms - w.cutoff_ms;
        let (w_actions, w_refusals, rem) =
            apply_drop_order(input, remaining, WithheldReason::WindowDeadline, |rem| {
                w_p_ms(rem, input, w) <= room
            });
        remaining = rem;
        actions.extend(w_actions);
        refusals.extend(w_refusals);
        wall_fit = Some((w_p_ms(&remaining, input, w), room));
    }
    // Pair atomicity on the surviving withheld set — a declared pair
    // never splits (the drop loop withholds whole arms or everything,
    // so a split can only mean a malformed `pairs` declaration).
    let withheld_ids: BTreeMap<String, ()> = actions
        .iter()
        .flat_map(|a| a.slots.iter().map(|s| (s.clone(), ())))
        .collect();
    if let Some(w) = wall {
        check_pair_closure(&withheld_ids, &w.pairs)?;
    }
    let cost_fit = cost_fit(input, &remaining);
    Ok(CheckpointOutcome {
        scope: input.scope,
        p_hats,
        t_hats,
        cost_fit,
        wall_fit,
        withheld: actions,
        inference_refusals: refusals,
        continuing: remaining,
        p99: p99_report(&input.measured, &input.client_timeouts_ms),
    })
}

// ── per-arm caps and expected-cost admission (REQ-XH-117) ────────────

/// `cap_table(planned, input)` — the µ$ cap formula table: `cap_h =
/// planned_slots_h × p̂_h` for each unprotected arm (REQ-XH-117). The
/// protected arms are absent — bounded by the root and the checkpoint
/// only.
pub fn cap_table(
    planned: &BTreeMap<FpArm, i64>,
    input: &ContinuationInput,
) -> BTreeMap<FpArm, i64> {
    FpArm::ALL
        .iter()
        .filter(|a| !input.priority.is_protected(**a))
        .map(|a| (*a, planned.get(a).copied().unwrap_or(0) * p_hat(*a, input)))
        .collect()
}

/// `cap_record(planned, caps, p_hats)` — the `hh-fp-arm-caps/1`
/// formula table for the pre-registration (values fill at FP-E).
pub fn cap_record(
    planned: &BTreeMap<FpArm, i64>,
    caps: &BTreeMap<FpArm, i64>,
    p_hats: &BTreeMap<FpArm, i64>,
) -> Json {
    Json::obj([
        ("schema", Json::str(CAP_TABLE_SCHEMA)),
        (
            "formula",
            Json::str("cap_h = planned_slots_h × p_hat_h (integer micros)"),
        ),
        (
            "arms",
            Json::Obj(
                caps.iter()
                    .map(|(a, c)| {
                        (
                            a.as_str().to_string(),
                            Json::obj([
                                (
                                    "planned_slots",
                                    Json::Int(planned.get(a).copied().unwrap_or(0)),
                                ),
                                (
                                    "p_hat_micros",
                                    Json::Int(p_hats.get(a).copied().unwrap_or(0)),
                                ),
                                ("cap_micros", Json::Int(*c)),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

/// The slot-admission verdict under an arm's cap.
#[derive(Debug, Clone, PartialEq)]
pub enum AdmitVerdict {
    /// Expected cost fits the cap — admit.
    Admit,
    /// Expected cost exceeds the cap — the arm's remaining slots are
    /// `withheld{budget}`, with no cell and never a scored refusal.
    Capped {
        /// `consumed_h + Σ in-flight max(p̂, consumed-to-date) + p̂`.
        expected_micros: i64,
        /// The arm's cap.
        cap_micros: i64,
    },
    /// A protected arm — bounded by the root cap and the checkpoint
    /// only; the per-arm cap never applies (Q-X61).
    ProtectedByRoot,
}

/// `admit_slot(arm, input, caps)` — the expected-cost admission
/// (REQ-XH-117): admit iff `consumed_h + Σ_{in-flight} max(p̂_h,
/// consumed-to-date) + p̂_h ≤ cap_h`.
pub fn admit_slot(
    arm: FpArm,
    input: &ContinuationInput,
    caps: &BTreeMap<FpArm, i64>,
) -> AdmitVerdict {
    if input.priority.is_protected(arm) {
        return AdmitVerdict::ProtectedByRoot;
    }
    let p = p_hat(arm, input);
    let consumed = input.consumed_by_arm.get(&arm).copied().unwrap_or(0);
    let flight: i64 = input
        .in_flight
        .get(&arm)
        .map(|rows| rows.iter().map(|(_, c)| p.max(*c)).sum())
        .unwrap_or(0);
    let expected = consumed + flight + p;
    let cap = caps.get(&arm).copied().unwrap_or(0);
    if expected <= cap {
        AdmitVerdict::Admit
    } else {
        AdmitVerdict::Capped {
            expected_micros: expected,
            cap_micros: cap,
        }
    }
}

/// `overshoot_bound_micros(in_flight, s, p_hat)` — the in-flight
/// overshoot bound: in-flight slots × (s − p̂_h), covered by the root
/// (the C × s feasibility term already reserves the slices).
pub fn overshoot_bound_micros(in_flight: i64, slice_micros: i64, p_hat_micros: i64) -> i64 {
    in_flight * (slice_micros - p_hat_micros).max(0)
}

/// One confirmatory-family member (§10.1.3): P2-FP's two contrasts
/// against mini-swe-agent and P4-FP's single contrast. A dropped arm
/// stays in its family as an unavailable member — the family is
/// retained, never shrunk (REQ-XH-117/127).
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyMember {
    /// The claim (`P2-FP`, `P4-FP`).
    pub claim: &'static str,
    /// The contrast label.
    pub contrast: &'static str,
    /// The arm the contrast depends on.
    pub arm: FpArm,
    /// `false` when the arm was dropped or held — the member is
    /// retained as unavailable, never removed.
    pub available: bool,
    /// The `withheld{…}` spelling when unavailable.
    pub withheld: Option<String>,
}

/// `first_pass_family(dropped)` — the first-pass confirmatory
/// families with each dropped arm's member retained as unavailable
/// (REQ-XH-117: "a dropped arm stays in its Holm family as an
/// unavailable member").
pub fn first_pass_family(dropped: &BTreeMap<FpArm, WithheldReason>) -> Vec<FamilyMember> {
    [
        ("P2-FP", "Δ(pi − mini-swe-agent)", FpArm::Pi),
        (
            "P2-FP",
            "Δ(claude-code − mini-swe-agent)",
            FpArm::ClaudeCode,
        ),
        ("P4-FP", "Δ(v5 − hh_native)", FpArm::V5),
    ]
    .iter()
    .map(|(claim, contrast, arm)| {
        let w = dropped.get(arm);
        FamilyMember {
            claim,
            contrast,
            arm: *arm,
            available: w.is_none(),
            withheld: w.map(|r| Disposition::Withheld { reason: *r }.as_str()),
        }
    })
    .collect()
}

/// `arm_cost_report(metered, capped)` — REQ-XD-040 + REQ-XC-211's
/// literal predicate: a capped arm's cost is reported as a censored
/// lower bound, never the full figure.
pub fn arm_cost_report(metered_micros: i64, capped: bool) -> Json {
    Json::obj([
        ("metered_micros", Json::Int(metered_micros)),
        (
            "basis",
            Json::str(if capped {
                "censored_lower_bound"
            } else {
                "complete"
            }),
        ),
    ])
}

// ── D_max over R6.14's ceilings (REQ-XH-126(b)) ─────────────────────

/// One completed window's contribution to the Σ ≤ $20 sum — its
/// billed figure where a reconcile read exists, else its ceiling.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSum {
    /// The window id.
    pub window: String,
    /// The declared ceiling (µ$).
    pub ceiling_micros: i64,
    /// The reconciled billed figure, when one exists.
    pub billed_micros: Option<i64>,
}

impl WindowSum {
    /// The effective figure in the sum.
    pub fn effective_micros(&self) -> i64 {
        self.billed_micros.unwrap_or(self.ceiling_micros)
    }
}

/// The W4 ceiling decomposition: `ceiling = r·D + fixed`, where
/// `fixed` = restart + residual disk + egress terms (R6.14's
/// formula).
#[derive(Debug, Clone, PartialEq)]
pub struct W4Ceiling {
    /// The instance hourly rate r (µ$/h).
    pub hourly_micros: i64,
    /// The non-D terms: `r·restart_h + S·r_disk·h_r + egress`.
    pub fixed_micros: i64,
}

/// The contingency window's contribution: its full ceiling when
/// authorized, else only the restart-overflow reservation (`r × 1 h +
/// S·r_disk·h_r` ≈ $0.56 — the release adds ≈ 4.4 h to D_max).
#[derive(Debug, Clone, PartialEq)]
pub struct WcTerm {
    /// `true` when Wc was authorized at a gate.
    pub authorized: bool,
    /// The declared Wc ceiling (µ$).
    pub ceiling_micros: i64,
    /// The restart-overflow reservation kept on release (µ$).
    pub reservation_micros: i64,
}

impl WcTerm {
    /// The effective figure in the sum.
    pub fn effective_micros(&self) -> i64 {
        if self.authorized {
            self.ceiling_micros
        } else {
            self.reservation_micros
        }
    }
}

/// `overflow_reservation_micros(r, disk_rate, s_gib, h_r)` — the kept
/// restart-overflow reservation: `r × 1 h + S·r_disk·h_r` (§10.1.5).
pub fn overflow_reservation_micros(
    hourly_micros: i64,
    disk_gib_hour_micros: i64,
    s_gib: i64,
    h_r_hours: i64,
) -> i64 {
    hourly_micros + s_gib * disk_gib_hour_micros * h_r_hours
}

/// `d_max_ms(completed, w4, wc)` — the largest W4 dead-man D (ms)
/// that keeps Σ ceilings ≤ $20: completed windows at billed where
/// reconciled else ceiling; an unauthorized Wc contributes only its
/// restart-overflow reservation. Floor division — the largest D that
/// still fits.
pub fn d_max_ms(completed: &[WindowSum], w4: &W4Ceiling, wc: &WcTerm) -> i64 {
    let others: i64 = completed
        .iter()
        .map(WindowSum::effective_micros)
        .sum::<i64>()
        + wc.effective_micros();
    let headroom = FP_COMPUTE_CAP_MICROS - others - w4.fixed_micros;
    if headroom <= 0 || w4.hourly_micros <= 0 {
        return 0;
    }
    headroom * MS_PER_HOUR / w4.hourly_micros
}

// ── §6.6 stop rules (REQ-XD-042) ─────────────────────────────────────

/// The stop triggers — §6.6's closed set: budget, infrastructure,
/// integrity, calendar. There is no outcome-based trigger; a fifth
/// kind refuses typed (REQ-XD-042).
#[derive(Debug, Clone, PartialEq)]
pub enum StopTrigger {
    /// The proxy root cap — `consumed + reserved` reaches the root.
    Budget {
        /// Metered consumed (µ$).
        consumed_micros: i64,
        /// Outstanding reservations (µ$).
        reserved_micros: i64,
        /// The root cap (µ$).
        root_cap_micros: i64,
    },
    /// An infrastructure-rate stop: `stage` selects the §6.6
    /// threshold — PP0 holds on ≥ 1 of 3; the window stage (the
    /// first-pass reading of the PP1 rule) stops an arm when the
    /// infra rate exceeds 20 % after ≥ 10 trials. `stopped_arms`
    /// feeds the ≥ 3 pilot stop.
    Infra {
        /// `pp0` or `window`.
        stage: String,
        /// Trials measured in the stage.
        trials: i64,
        /// `inconclusive{infra:*}` trials.
        infra_failed: i64,
        /// Arms already stopped in the pass.
        stopped_arms: i64,
        /// The arm under evaluation.
        arm: FpArm,
    },
    /// An integrity event — stops everything at once, keeping
    /// partial rows.
    Integrity {
        /// The §6.6 event kind.
        event: IntegrityEvent,
    },
    /// The provider credit period end — unless the operator ruled
    /// otherwise (Q-X21, recorded on the input).
    Calendar {
        /// The credit period end (ms).
        credit_period_end_ms: i64,
        /// Now (ms).
        now_ms: i64,
        /// A recorded operator override.
        operator_overrode: bool,
    },
}

/// The §6.6 integrity events — each stops everything at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityEvent {
    /// A model call without a receipt.
    UnreceiptedCall,
    /// Egress outside the allowlist.
    EgressOutsideAllowlist,
    /// Credential exposure.
    CredentialExposure,
    /// An O/H image-digest mismatch.
    ImageDigestMismatch,
    /// A catalog-identity change.
    CatalogIdentityChange,
    /// A confirmed answer-key receipt in more than one trial.
    AnswerKeyMultiTrial,
}

impl IntegrityEvent {
    /// The recorded spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IntegrityEvent::UnreceiptedCall => "unreceipted_call",
            IntegrityEvent::EgressOutsideAllowlist => "egress_outside_allowlist",
            IntegrityEvent::CredentialExposure => "credential_exposure",
            IntegrityEvent::ImageDigestMismatch => "image_digest_mismatch",
            IntegrityEvent::CatalogIdentityChange => "catalog_identity_change",
            IntegrityEvent::AnswerKeyMultiTrial => "answer_key_multi_trial",
        }
    }
}

/// A stop decision — every variant retains partial rows and costs
/// (REQ-XD-042).
#[derive(Debug, Clone, PartialEq)]
pub struct StopDecision {
    /// `arm` for an arm stop/hold; `pass` for a whole-pass stop.
    pub scope: String,
    /// The trigger kind (`budget`/`infra`/`integrity`/`calendar`).
    pub trigger: &'static str,
    /// The verbatim detail.
    pub detail: String,
    /// The `withheld{…}` the arm's remaining slots take, where the
    /// decision is arm-scoped.
    pub withheld: Option<WithheldReason>,
    /// Partial rows and costs are retained — always `true`.
    pub partial_rows_retained: bool,
}

impl StopDecision {
    /// The canonical JSON row.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("scope", Json::str(&self.scope)),
            ("trigger", Json::str(self.trigger)),
            ("detail", Json::str(&self.detail)),
            (
                "withheld",
                self.withheld
                    .map(|r| Json::str(Disposition::Withheld { reason: r }.as_str()))
                    .unwrap_or(Json::Null),
            ),
            (
                "partial_rows_retained",
                Json::Bool(self.partial_rows_retained),
            ),
        ])
    }
}

/// `stop_trigger_from_json(j)` — the strict decode: `kind` ∈
/// `budget|infra|integrity|calendar` only; an outcome-shaped trigger
/// (`outcome`, `verdict`, `reward`, …) or anything else refuses
/// typed — no outcome-based stopping exists to configure.
pub fn stop_trigger_from_json(j: &Json) -> Result<StopTrigger, ContinuationError> {
    check_cost_only(j)?;
    let m = j
        .as_obj()
        .ok_or_else(|| schema("stop trigger must be an object"))?;
    match m.get("kind").and_then(Json::as_str) {
        Some("budget") => Ok(StopTrigger::Budget {
            consumed_micros: m
                .get("consumed_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("budget.consumed_micros required"))?,
            reserved_micros: m
                .get("reserved_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("budget.reserved_micros required"))?,
            root_cap_micros: m
                .get("root_cap_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("budget.root_cap_micros required"))?,
        }),
        Some("infra") => Ok(StopTrigger::Infra {
            stage: m
                .get("stage")
                .and_then(Json::as_str)
                .ok_or_else(|| schema("infra.stage required"))?
                .to_string(),
            trials: m
                .get("trials")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("infra.trials required"))?,
            infra_failed: m
                .get("infra_failed")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("infra.infra_failed required"))?,
            stopped_arms: m
                .get("stopped_arms")
                .and_then(Json::as_int)
                .unwrap_or(0),
            arm: m
                .get("arm")
                .and_then(Json::as_str)
                .and_then(FpArm::parse)
                .ok_or_else(|| schema("infra.arm required"))?,
        }),
        Some("integrity") => {
            let event = match m.get("event").and_then(Json::as_str) {
                Some("unreceipted_call") => IntegrityEvent::UnreceiptedCall,
                Some("egress_outside_allowlist") => IntegrityEvent::EgressOutsideAllowlist,
                Some("credential_exposure") => IntegrityEvent::CredentialExposure,
                Some("image_digest_mismatch") => IntegrityEvent::ImageDigestMismatch,
                Some("catalog_identity_change") => IntegrityEvent::CatalogIdentityChange,
                Some("answer_key_multi_trial") => IntegrityEvent::AnswerKeyMultiTrial,
                other => {
                    return Err(ContinuationError::UnsupportedTrigger {
                        detail: format!("integrity.event {other:?} is not a §6.6 event"),
                    })
                }
            };
            Ok(StopTrigger::Integrity { event })
        }
        Some("calendar") => Ok(StopTrigger::Calendar {
            credit_period_end_ms: m
                .get("credit_period_end_ms")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("calendar.credit_period_end_ms required"))?,
            now_ms: m
                .get("now_ms")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("calendar.now_ms required"))?,
            operator_overrode: m
                .get("operator_overrode")
                .and_then(Json::as_bool)
                .unwrap_or(false),
        }),
        Some(other) => Err(ContinuationError::UnsupportedTrigger {
            detail: format!(
                "{other} — §6.6's stop rules are budget, infrastructure, integrity and calendar only; no outcome-based trigger exists (REQ-XD-042)"
            ),
        }),
        None => Err(schema("stop trigger .kind required")),
    }
}

/// `evaluate_stop(trigger)` — §6.6's thresholds, unchanged in the
/// first pass (§10.1.6): the root cap, the infra holds/stops, the
/// integrity events (everything at once, partial rows kept), the
/// credit period end. Returns `None` when no rule fires.
pub fn evaluate_stop(trigger: &StopTrigger) -> Option<StopDecision> {
    match trigger {
        StopTrigger::Budget {
            consumed_micros,
            reserved_micros,
            root_cap_micros,
        } if consumed_micros + reserved_micros >= *root_cap_micros => Some(StopDecision {
            scope: "pass".to_string(),
            trigger: "budget",
            detail: format!(
                "consumed {consumed_micros} + reserved {reserved_micros} ≥ root {root_cap_micros} µ$"
            ),
            withheld: Some(WithheldReason::Budget),
            partial_rows_retained: true,
        }),
        StopTrigger::Infra {
            stage,
            trials,
            infra_failed,
            stopped_arms,
            arm,
        } => {
            if *stopped_arms >= 3 {
                return Some(StopDecision {
                    scope: "pass".to_string(),
                    trigger: "infra",
                    detail: format!("{stopped_arms} arms already stopped — ≥ 3 stops the pass"),
                    withheld: Some(WithheldReason::Infra),
                    partial_rows_retained: true,
                });
            }
            if stage == "pp0" && *infra_failed >= 1 && *trials > 0 {
                return Some(StopDecision {
                    scope: arm.as_str().to_string(),
                    trigger: "infra",
                    detail: format!(
                        "{infra_failed} of {trials} PP0 trials infra-failed — the PP0 hold (REQ-XH-127)"
                    ),
                    withheld: Some(WithheldReason::Admission(AdmissionReason::Pp0InfraHold)),
                    partial_rows_retained: true,
                });
            }
            if stage == "window" && *trials >= 10 && infra_failed * 5 > *trials {
                return Some(StopDecision {
                    scope: arm.as_str().to_string(),
                    trigger: "infra",
                    detail: format!(
                        "infra rate {infra_failed}/{trials} > 20% after ≥ 10 trials — the arm stops"
                    ),
                    withheld: Some(WithheldReason::Infra),
                    partial_rows_retained: true,
                });
            }
            None
        }
        StopTrigger::Integrity { event } => Some(StopDecision {
            scope: "pass".to_string(),
            trigger: "integrity",
            detail: format!("{} — everything stops at once", event.as_str()),
            withheld: Some(WithheldReason::Infra),
            partial_rows_retained: true,
        }),
        StopTrigger::Calendar {
            credit_period_end_ms,
            now_ms,
            operator_overrode,
        } if now_ms >= credit_period_end_ms && !operator_overrode => Some(StopDecision {
            scope: "pass".to_string(),
            trigger: "calendar",
            detail: format!("now {now_ms} ≥ credit period end {credit_period_end_ms}"),
            withheld: None,
            partial_rows_retained: true,
        }),
        _ => None,
    }
}
