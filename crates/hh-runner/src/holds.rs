//! `holds` — the first-pass pre-declared holds (manifest 283/R6.29,
//! Stage 2; spec §10.1.4 fallback + W4 lane probe, §10.1.2 dev-cell
//! replacement; REQ-XH-123, REQ-XH-124, REQ-XH-127; contributing
//! REQ-XH-027 the `w4-lane-probe` leg, REQ-XD-102 new-H-new-G-L1).
//!
//! Machinery only — the holds are **detected** from declared inputs
//! (a trigger record, a G-L1 partition pair, PP0 infra members and
//! AG3/AG5/AG6/AG8 evidence) and **decided** by the rules here; the
//! decisions run at FP-E (R6.46) and W4 (R6.47). No rule reads an
//! outcome field: every input decode runs [`check_cost_only`] first,
//! and PP0 evidence carrying a task verdict refuses typed — AG
//! evidence reads exit state, receipts, egress rows and protocol
//! errors, never the task verdict (REQ-XH-121's carve-out).
//!
//! REQ-XH-127's reading of §10.1.2: a development-task exclusion that
//! empties its difficulty cell draws from the adjacent cell's
//! committed order — easy → medium, hard → medium (the spec maps no
//! donor for an emptied medium cell; that input refuses typed rather
//! than invent one).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hharness_budget::pricing::PricingUnit;
use hharness_budget::ModelRef;
use hharness_wire::json::Json;

use crate::continuation::{
    check_cost_only, schema, ContinuationError, FpArm, WithholdAction, DROP_LABEL,
};
use crate::disposition::{AdmissionReason, Disposition, WithheldReason};
use crate::json_ext::JsonExt;
use crate::ps0::{
    plan_worst_case_micros, ProbeCall, ProbeOutcome, ProbeTransport, Ps0Error, SlotGrant, UsageView,
};
use crate::smoke::surface_route;

/// `hh-fp-fallback/1` — the hosted-only decision record.
pub const FALLBACK_SCHEMA: &str = "hh-fp-fallback/1";
/// `hh-lane-probe/1` — the HH-native lane-probe stage record.
pub const LANE_PROBE_SCHEMA: &str = "hh-lane-probe/1";
/// `hh-fp-scope-decision/1` — the REQ-XH-127(b) FP-E scope decision.
pub const SCOPE_DECISION_SCHEMA: &str = "hh-fp-scope-decision/1";
/// The lane probe's bound — §10.1.4's cents-scale ≤ $0.05.
pub const LANE_PROBE_CAP_MICROS: i64 = 50_000;
/// The claims the hosted-only fallback withholds (REQ-XH-123).
pub const FALLBACK_CLAIMS: [&str; 2] = ["P3-L2", "P4-FP"];
/// Every first-pass claim a held original mini-swe-agent withholds
/// (REQ-XH-127(b): the corpus is withheld, so G-L1 cannot pass).
pub const ALL_FP_CLAIMS: [&str; 4] = ["P2-FP", "P3-L1", "P3-L2", "P4-FP"];
/// The AG gates PP0 evidence carries (§10.1.4 W2(b)).
pub const PP0_GATES: [&str; 4] = ["AG3", "AG5", "AG6", "AG8"];
/// Milliseconds per day for the Q-X62 trigger.
pub const MS_PER_DAY: i64 = 86_400_000;

// ── the fallback (REQ-XH-123) ────────────────────────────────────────

/// One gate partition's outcome — machinery evidence, never a slot
/// outcome field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionResult {
    /// The partition's L1 legs passed.
    Pass,
    /// A leg failed (or the partition is absent/unverified).
    Fail,
    /// Not yet decided — the partition ran but produced no verdict.
    Pending,
}

impl PartitionResult {
    /// The strict parse; `None` outside the three spellings.
    pub fn parse(s: &str) -> Option<PartitionResult> {
        match s {
            "pass" => Some(PartitionResult::Pass),
            "fail" => Some(PartitionResult::Fail),
            "pending" => Some(PartitionResult::Pending),
            _ => None,
        }
    }
}

/// The G-L1 status at the decision point — the two gate partitions
/// plus the pass id when it exists (`l1_gate::l1_pass_id`'s output,
/// bound to the H digest — REQ-XD-102).
#[derive(Debug, Clone, PartialEq)]
pub struct Gl1Status {
    /// Gate-A partition (the dev corpus partition).
    pub gate_a: PartitionResult,
    /// Gate-B partition.
    pub gate_b: PartitionResult,
    /// `l1-pass:sha256:…` when G-L1 passed.
    pub pass_id: Option<String>,
}

impl Gl1Status {
    /// Strict decode — the firewall scan first (a G-L1 record never
    /// carries a slot outcome).
    pub fn from_json(j: &Json) -> Result<Gl1Status, ContinuationError> {
        check_cost_only(j)?;
        let m = j.as_obj().ok_or_else(|| schema("g_l1 must be an object"))?;
        let part = |key: &str| -> Result<PartitionResult, ContinuationError> {
            m.get(key)
                .and_then(Json::as_str)
                .and_then(PartitionResult::parse)
                .ok_or_else(|| schema(format!("g_l1.{key} must be pass|fail|pending")))
        };
        Ok(Gl1Status {
            gate_a: part("gate_a")?,
            gate_b: part("gate_b")?,
            pass_id: m.get("pass_id").and_then(Json::as_str).map(str::to_string),
        })
    }

    /// `true` when G-L1 has passed.
    pub fn passed(&self) -> bool {
        self.pass_id.is_some()
            && self.gate_a == PartitionResult::Pass
            && self.gate_b == PartitionResult::Pass
    }

    /// `true` when G-L1 failed on **both** gate partitions.
    pub fn failed_both(&self) -> bool {
        self.gate_a == PartitionResult::Fail && self.gate_b == PartitionResult::Fail
    }
}

/// The Q-X62 trigger record — recorded at FP-S, evaluated at FP-E.
/// `trigger_after_days` is the record's own value (the ledger's
/// recorded default is 14, decided at FP-S — a parameter, never
/// hard-coded).
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerRecord {
    /// The record's ref the FP-E input cites.
    pub record_ref: String,
    /// When FP-S became "otherwise ready" — every precondition except
    /// the E2-T8 rung record met (ms).
    pub fps_otherwise_ready_ms: i64,
    /// The record's own trigger interval (days).
    pub trigger_after_days: i64,
}

impl TriggerRecord {
    /// Strict decode — the firewall scan first; the fallback is
    /// decided from this record, never an outcome field.
    pub fn from_json(j: &Json) -> Result<TriggerRecord, ContinuationError> {
        check_cost_only(j)?;
        let m = j
            .as_obj()
            .ok_or_else(|| schema("trigger record must be an object"))?;
        for k in m.keys() {
            if !["record_ref", "fps_otherwise_ready_ms", "trigger_after_days"].contains(&k.as_str())
            {
                return Err(schema(format!("trigger.{k} is not a declared member")));
            }
        }
        Ok(TriggerRecord {
            record_ref: m
                .get("record_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| schema("trigger.record_ref required"))?
                .to_string(),
            fps_otherwise_ready_ms: m
                .get("fps_otherwise_ready_ms")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("trigger.fps_otherwise_ready_ms required"))?,
            trigger_after_days: m
                .get("trigger_after_days")
                .and_then(Json::as_int)
                .ok_or_else(|| schema("trigger.trigger_after_days required"))?,
        })
    }

    /// `fired(now)` — G-L1 has not passed `trigger_after_days` after
    /// FP-S was otherwise ready (the caller passes `now` at FP-E).
    pub fn fired(&self, now_ms: i64) -> bool {
        now_ms >= self.fps_otherwise_ready_ms + self.trigger_after_days * MS_PER_DAY
    }
}

/// Why the fallback fired — the decision cites exactly one.
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackCause {
    /// G-L1 failed on both gate partitions.
    Gl1BothPartitions,
    /// The Q-X62 trigger record fired.
    TriggerRecord {
        /// The cited record.
        record_ref: String,
    },
}

impl FallbackCause {
    /// The recorded spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            FallbackCause::Gl1BothPartitions => "g_l1_both_partitions",
            FallbackCause::TriggerRecord { .. } => "qx62_trigger",
        }
    }
}

/// The fallback decision — `hosted_only` arms, the cause, and the
/// claims withheld `l1_not_passed`.
#[derive(Debug, Clone, PartialEq)]
pub struct FallbackDecision {
    /// `true` → W4 runs the three hosted arms alone (phase 2 only).
    pub hosted_only: bool,
    /// The cause, when hosted-only.
    pub cause: Option<FallbackCause>,
    /// The cited trigger record ref (for the FP-E input).
    pub trigger_ref: Option<String>,
}

/// `fallback_decision(gl1, trigger, now_ms)` — REQ-XH-123: hosted-
/// only when G-L1 failed on both partitions or the recorded trigger
/// fired; never decided from an outcome field (the inputs' strict
/// decodes scan first).
pub fn fallback_decision(
    gl1: &Gl1Status,
    trigger: Option<&TriggerRecord>,
    now_ms: i64,
) -> FallbackDecision {
    if gl1.passed() {
        return FallbackDecision {
            hosted_only: false,
            cause: None,
            trigger_ref: trigger.map(|t| t.record_ref.clone()),
        };
    }
    if gl1.failed_both() {
        return FallbackDecision {
            hosted_only: true,
            cause: Some(FallbackCause::Gl1BothPartitions),
            trigger_ref: trigger.map(|t| t.record_ref.clone()),
        };
    }
    if let Some(t) = trigger {
        if t.fired(now_ms) {
            return FallbackDecision {
                hosted_only: true,
                cause: Some(FallbackCause::TriggerRecord {
                    record_ref: t.record_ref.clone(),
                }),
                trigger_ref: Some(t.record_ref.clone()),
            };
        }
    }
    FallbackDecision {
        hosted_only: false,
        cause: None,
        trigger_ref: trigger.map(|t| t.record_ref.clone()),
    }
}

/// `hosted_only_arms()` — the fallback's W4 arm set: original
/// mini-swe-agent, Pi, Claude Code (phase 2 only).
pub fn hosted_only_arms() -> Vec<FpArm> {
    vec![FpArm::OrigMini, FpArm::Pi, FpArm::ClaudeCode]
}

/// `fallback_record(decision)` — the `hh-fp-fallback/1` decision
/// record the FP-E input cites: the hosted-only schedule and both
/// typed withholdings (`P3-L2`, `P4-FP` `withheld{l1_not_passed}`).
pub fn fallback_record(decision: &FallbackDecision) -> Json {
    let claims: Vec<Json> = FALLBACK_CLAIMS
        .iter()
        .map(|c| {
            Json::obj([
                ("claim", Json::str(*c)),
                (
                    "disposition",
                    Json::str(
                        Disposition::Withheld {
                            reason: WithheldReason::L1NotPassed,
                        }
                        .as_str(),
                    ),
                ),
                ("moved_to", Json::str("later_pass")),
            ])
        })
        .collect();
    Json::obj([
        ("schema", Json::str(FALLBACK_SCHEMA)),
        ("hosted_only", Json::Bool(decision.hosted_only)),
        (
            "cause",
            decision
                .cause
                .as_ref()
                .map(|c| Json::str(c.as_str()))
                .unwrap_or(Json::Null),
        ),
        (
            "trigger_ref",
            decision
                .trigger_ref
                .as_ref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "arms",
            Json::Arr(if decision.hosted_only {
                hosted_only_arms()
                    .iter()
                    .map(|a| Json::str(a.as_str()))
                    .collect()
            } else {
                FpArm::ALL.iter().map(|a| Json::str(a.as_str())).collect()
            }),
        ),
        (
            "withheld_claims",
            Json::Arr(if decision.hosted_only {
                claims
            } else {
                Vec::new()
            }),
        ),
        ("label", Json::str(DROP_LABEL)),
    ])
}

// ── the lane probe (REQ-XH-124) ──────────────────────────────────────

/// `lane_probe_plan(model, surface, effort_paths)` — one receipted
/// call per pinned effort path through the proxy's loopback listener
/// on the slot token. The probe exercises the HH-native lane only;
/// an effort path or surface outside the declaration refuses typed
/// before any call.
pub fn lane_probe_plan(
    model: &str,
    surface: &str,
    effort_paths: &[&str],
) -> Result<Vec<ProbeCall>, ContinuationError> {
    let route = surface_route(surface).ok_or_else(|| ContinuationError::UnsupportedTrigger {
        detail: format!("lane surface {surface} is not a proxy-served dialect"),
    })?;
    if effort_paths.is_empty() {
        return Err(schema("lane probe requires ≥ 1 pinned effort path"));
    }
    let mut calls = Vec::new();
    for effort in effort_paths {
        let body = Json::obj([
            ("model", Json::str(model)),
            ("max_tokens", Json::Int(8)),
            ("stream", Json::Bool(false)),
            ("reasoning_effort", Json::str(*effort)),
            (
                "messages",
                Json::Arr(vec![Json::obj([
                    ("role", Json::str("user")),
                    (
                        "content",
                        Json::str("HH-native lane probe — answer with the single token ok."),
                    ),
                ])]),
            ),
        ]);
        calls.push(ProbeCall {
            probe_id: format!("lane-probe-{effort}"),
            kind: crate::ps0::ProbeKind::Dialect,
            surface: surface.to_string(),
            route: route.to_string(),
            body,
            declared_input_tokens: 128,
            declared_output_tokens: 8,
        });
    }
    Ok(calls)
}

/// A lane-probe bound breach — the plan refuses before any call
/// (X-02).
#[derive(Debug, Clone, PartialEq)]
pub struct LaneProbeRefusal {
    /// The priced worst case.
    pub worst_case_micros: i64,
    /// The cap.
    pub cap_micros: i64,
}

impl std::fmt::Display for LaneProbeRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "refused{{plan_bound}}: worst-case {} µ$ exceeds the lane-probe cap {} µ$ ($0.05)",
            self.worst_case_micros, self.cap_micros
        )
    }
}
impl std::error::Error for LaneProbeRefusal {}

/// `lane_probe_bound(calls, model_ref, table)` — the mechanical ≤
/// $0.05 bound through the one pricing implementation (CC1).
pub fn lane_probe_bound(
    calls: &[ProbeCall],
    model_ref: &ModelRef,
    table: &impl PricingUnit,
) -> Result<i64, LaneProbeRefusal> {
    let worst = plan_worst_case_micros(calls, model_ref, table).unwrap_or(i64::MAX);
    if worst > LANE_PROBE_CAP_MICROS {
        return Err(LaneProbeRefusal {
            worst_case_micros: worst,
            cap_micros: LANE_PROBE_CAP_MICROS,
        });
    }
    Ok(worst)
}

/// The `hh-lane-probe/1` stage record — one receipted call per
/// pinned effort path.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneProbeStage {
    /// The planned calls.
    pub calls: Vec<ProbeCall>,
    /// The recorded outcome per call, in issued order.
    pub rows: Vec<(String, String, Option<UsageView>, i64)>,
    /// The priced worst case (integer µ$).
    pub worst_case_micros: i64,
    /// `pass` iff every call receipted `pass`.
    pub disposition: String,
    /// The slot grant the probe rode.
    pub slot: SlotGrant,
    /// The recorded instant.
    pub recorded_at: String,
    /// `recorded_execution` until the `w4-lane-probe` leg earns
    /// `live-executed` (REQ-XH-027).
    pub evidence_label: String,
}

impl LaneProbeStage {
    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let calls: Vec<Json> = self
            .calls
            .iter()
            .zip(self.rows.iter())
            .map(|(c, (disp, receipt, usage, cost))| {
                let mut m = vec![
                    ("probe_id", Json::str(&c.probe_id)),
                    ("surface", Json::str(&c.surface)),
                    ("route", Json::str(&c.route)),
                    ("disposition", Json::str(disp)),
                    ("receipt_ref", Json::str(receipt)),
                    ("cost_micros", Json::Int(*cost)),
                ];
                if let Some(u) = usage {
                    m.push((
                        "usage",
                        Json::obj([
                            ("input_uncached", Json::Int(u.input_uncached)),
                            ("cache_read", Json::Int(u.cache_read)),
                            ("output", Json::Int(u.output)),
                        ]),
                    ));
                }
                Json::obj(m)
            })
            .collect();
        Json::obj([
            ("schema", Json::str(LANE_PROBE_SCHEMA)),
            ("stage", Json::str("lane-probe")),
            ("window", Json::str("W4")),
            ("gate", Json::str("FP-E")),
            (
                "slot",
                Json::obj([
                    ("token_ref", Json::str(&self.slot.token_ref)),
                    ("slice_micros", Json::Int(self.slot.slice_micros)),
                ]),
            ),
            (
                "plan",
                Json::obj([
                    ("calls", Json::Int(self.calls.len() as i64)),
                    ("worst_case_micros", Json::Int(self.worst_case_micros)),
                    ("cap_micros", Json::Int(LANE_PROBE_CAP_MICROS)),
                ]),
            ),
            ("calls", Json::Arr(calls)),
            ("precedes", Json::str("first_hh_native_slot")),
            ("disposition", Json::str(&self.disposition)),
            ("evidence_label", Json::str(&self.evidence_label)),
            ("recorded_at", Json::str(&self.recorded_at)),
        ])
    }
}

/// `run_lane_probe(calls, slot, transport, model_ref, table,
/// recorded_at)` — the driver: bound the plan first, then issue
/// exactly the planned calls in order on the slot token, each
/// receipted, before W4's first HH-native slot.
pub fn run_lane_probe(
    calls: &[ProbeCall],
    slot: &SlotGrant,
    transport: &mut dyn ProbeTransport,
    model_ref: &ModelRef,
    table: &impl PricingUnit,
    recorded_at: &str,
) -> Result<LaneProbeStage, LaneProbeRefusal> {
    let worst = lane_probe_bound(calls, model_ref, table)?;
    let mut rows = Vec::new();
    for call in calls {
        let row = match transport.call(slot, call) {
            Ok(ProbeOutcome {
                disposition,
                receipt_ref,
                usage,
                cost_micros,
            }) => {
                let d = if receipt_ref.is_empty() {
                    "unreceipted".to_string()
                } else {
                    disposition
                };
                (d, receipt_ref, usage, cost_micros)
            }
            Err(Ps0Error { detail }) => (
                format!("unreceipted{{transport:{detail}}}"),
                String::new(),
                None,
                0,
            ),
        };
        rows.push(row);
    }
    let passed = rows.iter().all(|(d, _, _, _)| d == "pass");
    Ok(LaneProbeStage {
        calls: calls.to_vec(),
        rows,
        worst_case_micros: worst,
        disposition: if passed { "pass" } else { "failed" }.to_string(),
        slot: slot.clone(),
        recorded_at: recorded_at.to_string(),
        evidence_label: "recorded_execution".to_string(),
    })
}

/// `probe_precedes_native(probe_end_seq, first_native_seq)` — the
/// schedule-ordering predicate: every probe receipt precedes the
/// first HH-native slot start in the W4 record (REQ-XH-124).
pub fn probe_precedes_native(probe_end_seq: i64, first_native_seq: i64) -> bool {
    probe_end_seq < first_native_seq
}

/// `native_lane_hold(arm, probe_passed, first3_infra, remaining)` —
/// REQ-XH-124's hold: a failed probe, or ≥ 1 infrastructure-failed
/// slot among the arm's first 3, holds that arm — its remaining
/// slots are `withheld{admission:native_lane_hold}` while the hosted
/// arms continue. Only the native-lane arms can hold here; a hosted
/// arm input refuses typed rather than silently passing.
pub fn native_lane_hold(
    arm: FpArm,
    probe_passed: bool,
    first3_infra: &[bool],
    remaining: Vec<String>,
) -> Result<Option<WithholdAction>, ContinuationError> {
    if !arm.native_lane() {
        return Err(schema(format!(
            "{} is not a native-lane arm — REQ-XH-124's hold applies to hh_native and v5 only",
            arm.as_str()
        )));
    }
    let infra_failed = first3_infra.iter().take(3).filter(|f| **f).count();
    if probe_passed && infra_failed == 0 {
        return Ok(None);
    }
    Ok(Some(WithholdAction {
        arm,
        slots: remaining,
        reason: WithheldReason::Admission(AdmissionReason::NativeLaneHold),
        label: DROP_LABEL,
    }))
}

/// `native_lane_hold_record(arm, action, cause)` — the hold record
/// with the REQ-XD-102 repair semantics: a repair is a new H version
/// that needs a new G-L1.
pub fn native_lane_hold_record(arm: FpArm, action: &WithholdAction, cause: &str) -> Json {
    Json::obj([
        ("arm", Json::str(arm.as_str())),
        ("cause", Json::str(cause)),
        (
            "disposition",
            Json::str(
                Disposition::Withheld {
                    reason: WithheldReason::Admission(AdmissionReason::NativeLaneHold),
                }
                .as_str(),
            ),
        ),
        (
            "slots",
            Json::Arr(action.slots.iter().map(Json::str).collect()),
        ),
        ("repair", Json::str("new_h_version_requires_new_g_l1")),
        ("hosted_arms", Json::str("continue")),
        ("label", Json::str(DROP_LABEL)),
    ])
}

// ── the PP0 holds (REQ-XH-127) ───────────────────────────────────────

/// One PP0 trial's hold-relevant evidence — the infra member plus the
/// AG3/AG5/AG6/AG8 check results. A `verdict` member (or any outcome
/// field) refuses at decode — the evidence reads exit state,
/// receipts, egress rows and protocol errors only.
#[derive(Debug, Clone, PartialEq)]
pub struct Pp0TrialEvidence {
    /// The trial ref.
    pub trial_ref: String,
    /// `inconclusive{infra:*}` — an infra member (readable).
    pub infra_failed: bool,
    /// `AG3|AG5|AG6|AG8 → pass|fail` results.
    pub ag: BTreeMap<String, String>,
}

impl Pp0TrialEvidence {
    /// Strict decode; a task verdict (or any outcome member) refuses
    /// typed before shape checks.
    pub fn from_json(j: &Json) -> Result<Pp0TrialEvidence, ContinuationError> {
        check_cost_only(j)?;
        let m = j
            .as_obj()
            .ok_or_else(|| schema("pp0 evidence must be an object"))?;
        for k in m.keys() {
            if !["trial_ref", "infra_failed", "ag"].contains(&k.as_str()) {
                return Err(schema(format!("pp0 evidence member {k} is not declared")));
            }
        }
        let mut ag = BTreeMap::new();
        if let Some(checks) = m.get("ag").and_then(Json::as_obj) {
            for (g, r) in checks {
                if !PP0_GATES.contains(&g.as_str()) {
                    return Err(schema(format!("pp0 ag check {g} is not a PP0 gate")));
                }
                ag.insert(
                    g.clone(),
                    r.as_str()
                        .ok_or_else(|| schema("pp0 ag results must be strings"))?
                        .to_string(),
                );
            }
        }
        Ok(Pp0TrialEvidence {
            trial_ref: m
                .get("trial_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| schema("pp0.trial_ref required"))?
                .to_string(),
            infra_failed: m
                .get("infra_failed")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            ag,
        })
    }
}

/// What held the arm — the disposition's named member.
#[derive(Debug, Clone, PartialEq)]
pub enum Pp0HoldKind {
    /// A failed AG check — `withheld{admission:AG<n>}` naming the
    /// smallest failed gate.
    Gate(u8),
    /// An infrastructure hold with no failed gate —
    /// `withheld{admission:pp0_infra_hold}`.
    InfraNoGate,
}

impl Pp0HoldKind {
    /// The withheld reason.
    pub fn withheld(&self) -> WithheldReason {
        match self {
            Pp0HoldKind::Gate(n) => WithheldReason::Admission(AdmissionReason::Gate(*n)),
            Pp0HoldKind::InfraNoGate => WithheldReason::Admission(AdmissionReason::Pp0InfraHold),
        }
    }
}

/// `pp0_hold(trials)` — the hold detection (REQ-XH-127): ≥ 1 of the
/// arm's PP0 trials infrastructure-failed, or a failed AG3/AG5/AG6/AG8
/// check. A failed gate names the hold; the infra-only form takes
/// `pp0_infra_hold` (never an outcome read).
pub fn pp0_hold(trials: &[Pp0TrialEvidence]) -> Option<Pp0HoldKind> {
    let mut gate: Option<u8> = None;
    let mut infra = false;
    for t in trials {
        if t.infra_failed {
            infra = true;
        }
        for (g, r) in &t.ag {
            if r != "pass" {
                if let Some(n) = g.strip_prefix("AG").and_then(|s| s.parse::<u8>().ok()) {
                    gate = Some(gate.map_or(n, |g0: u8| g0.min(n)));
                }
            }
        }
    }
    // A failed gate names the hold; an infra failure with no failed
    // gate takes pp0_infra_hold.
    match gate {
        Some(n) => Some(Pp0HoldKind::Gate(n)),
        None if infra => Some(Pp0HoldKind::InfraNoGate),
        None => None,
    }
}

/// `fp_e_arms(held)` — the FP-E arm set absent every held arm (the
/// hosted-only fallback's analog for holds; the held arm stays in
/// its Holm family as unavailable — `first_pass_family`).
pub fn fp_e_arms(held: &[FpArm]) -> Vec<FpArm> {
    FpArm::ALL
        .iter()
        .copied()
        .filter(|a| !held.contains(a))
        .collect()
}

/// `pp0_disposition(arm, hold)` — the declared dispositions:
/// (a) a held Pi or Claude Code is `withheld{admission:<r>}` for the
/// pass and drops from FP-E, retained unavailable in its family;
/// (b) a held original mini-swe-agent is the FP-E scope decision —
/// [`scope_decision_record`]. A native-lane arm input refuses typed
/// (PP0 covers the three hosted arms only).
pub fn pp0_disposition(arm: FpArm, hold: &Pp0HoldKind) -> Result<Json, ContinuationError> {
    match arm {
        FpArm::Pi | FpArm::ClaudeCode => Ok(Json::obj([
            ("arm", Json::str(arm.as_str())),
            (
                "disposition",
                Json::str(
                    Disposition::Withheld {
                        reason: hold.withheld(),
                    }
                    .as_str(),
                ),
            ),
            ("for", Json::str("the_pass")),
            ("family", Json::str("retained_unavailable")),
            ("drops_from", Json::str("fp_e")),
            ("repair", Json::str("new_participant_version_later_pass")),
            ("label", Json::str(DROP_LABEL)),
        ])),
        FpArm::OrigMini => Err(schema(
            "the original mini-swe-agent's PP0 hold is the FP-E scope decision — use scope_decision_record",
        )),
        FpArm::HhNative | FpArm::V5 => Err(schema(format!(
            "{} is not a PP0 arm — PP0 covers the three hosted arms only",
            arm.as_str()
        ))),
    }
}

/// `scope_decision_record(hold)` — REQ-XH-127(b): a held original
/// mini-swe-agent withholds the corpus, so G-L1 cannot pass. FP-E
/// presents a recorded scope decision whose **default** ends the
/// pass after W2, with every first-pass claim withheld by the hold's
/// typed reason; a repaired arm is a later pass.
pub fn scope_decision_record(arm: FpArm, hold: &Pp0HoldKind) -> Result<Json, ContinuationError> {
    if arm != FpArm::OrigMini {
        return Err(schema(format!(
            "REQ-XH-127(b)'s scope decision is the held original mini-swe-agent's — {} takes disposition (a)",
            arm.as_str()
        )));
    }
    let w = Disposition::Withheld {
        reason: hold.withheld(),
    }
    .as_str();
    let claims: Vec<Json> = ALL_FP_CLAIMS
        .iter()
        .map(|c| Json::obj([("claim", Json::str(*c)), ("disposition", Json::str(&w))]))
        .collect();
    Ok(Json::obj([
        ("schema", Json::str(SCOPE_DECISION_SCHEMA)),
        ("held_arm", Json::str(arm.as_str())),
        ("corpus", Json::str("withheld")),
        ("g_l1", Json::str("cannot_pass")),
        ("default", Json::str("end_pass_after_w2")),
        ("withheld_claims", Json::Arr(claims)),
        ("repaired_arm", Json::str("later_pass")),
        ("decided_at", Json::str("fp_e")),
        ("label", Json::str(DROP_LABEL)),
    ]))
}

/// `replace_empty_cell(empty_cell, committed_orders, drawn)` —
/// REQ-XH-127(c): a development-task exclusion that empties its
/// difficulty cell draws from the adjacent cell's committed order —
/// easy → medium, hard → medium — and sets the disclosure flag for
/// the pre-registration (R6.33's consumer). An emptied medium cell
/// has no declared donor and refuses typed; an exhausted donor
/// refuses typed.
pub fn replace_empty_cell(
    empty_cell: &str,
    committed_orders: &BTreeMap<String, Vec<String>>,
    drawn: &BTreeSet<String>,
) -> Result<Json, ContinuationError> {
    let donor = match empty_cell {
        "easy" | "hard" => "medium",
        "medium" => {
            return Err(schema(
                "an emptied medium cell has no declared adjacent donor (REQ-XH-127(c) maps easy|hard → medium only)",
            ))
        }
        other => {
            return Err(schema(format!("development cell {other} is not a declared cell")))
        }
    };
    let order = committed_orders
        .get(donor)
        .ok_or_else(|| schema(format!("no committed replacement order for cell {donor}")))?;
    let task = order
        .iter()
        .find(|t| !drawn.contains(*t))
        .ok_or_else(|| schema(format!("the {donor} committed order is exhausted")))?;
    Ok(Json::obj([
        ("replaced_task", Json::str(task)),
        ("from_cell", Json::str(donor)),
        ("empty_cell", Json::str(empty_cell)),
        ("disclosure", Json::Bool(true)),
        ("re_proof", Json::str("req-xh-116")),
        ("for", Json::str("R6.33 pre-registration")),
    ]))
}
