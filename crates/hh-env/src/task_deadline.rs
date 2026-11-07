//! The task-declared deadline admission (R6.9; AR-07; REQ-XB-118;
//! design/31 F13) — the *separate admitted path* for agent and verifier
//! deadlines a task declares above the legacy 900 s cap.
//!
//! The legacy derivation [`crate::verifier_bound::verifier_deadline_ms`]
//! (`min(declared, 900 s)`) is untouched and stays byte-identical — this
//! module adds a second function beside it, never widens it:
//!
//! - `task_declared_deadline_ms(declared_sec, multiplier_ppm,
//!   ceiling_sec)` enforces `declared × multiplier` in ms against the
//!   protocol-declared ceiling. A declaration above the ceiling and a
//!   malformed declaration (zero/negative declared, zero multiplier,
//!   overflow) refuse typed — never clamped, never defaulted.
//! - `admit_task_deadlines(record, admission)` is transactional: every
//!   leg is checked before the `ext["deadline_admission"]` record
//!   stamps, so a refused admission leaves the spec byte-identical.
//! - `task_deadline_echo(record)` yields the slot-row echo members.

use hharness_wire::json::Json;

use crate::errors::EnvError;
use crate::record::EnvironmentRecord;

/// The `ext` member the stamped admission record rides under.
pub const ADMISSION_FIELD: &str = "deadline_admission";
/// The canonical multiplier — `1.0` in ppm. A setup multiplier other
/// than `1_000_000` is declared per run (B1 F16); this machinery
/// records whatever the run declared, it never substitutes.
pub const CANONICAL_MULTIPLIER_PPM: u64 = 1_000_000;
/// The `source` the admission record names (F13 §4: values are read
/// verbatim from the pinned `task.toml` bytes).
pub const SOURCE_TASK_TOML: &str = "task.toml";

fn unsupported(detail: impl Into<String>) -> EnvError {
    EnvError::Unsupported {
        capability: "deadline.task_declared",
        detail: detail.into(),
    }
}

/// `task_declared_deadline_ms(declared_sec, multiplier_ppm,
/// ceiling_sec) → enforced ms` — the admitted derivation for
/// task-declared deadlines (AR-07): `declared × multiplier / 1e6` in
/// milliseconds, bounded by the *protocol-declared* ceiling — a value
/// above the ceiling refuses `exceeds_protocol_ceiling`, never clamps.
///
/// Refusals: `declared_sec == 0`, `multiplier_ppm == 0`, an enforced
/// value over the ceiling, or a product that overflows — a malformed
/// declaration never silently becomes a default.
pub fn task_declared_deadline_ms(
    declared_sec: u64,
    multiplier_ppm: u64,
    ceiling_sec: u64,
) -> Result<u64, EnvError> {
    if declared_sec == 0 {
        return Err(unsupported(
            "declared deadline 0 s is malformed — a deadline is positive",
        ));
    }
    if multiplier_ppm == 0 {
        return Err(unsupported(
            "multiplier_ppm 0 is malformed — a multiplier is positive",
        ));
    }
    if declared_sec > ceiling_sec {
        return Err(unsupported(format!(
            "declared {declared_sec} s exceeds the protocol ceiling \
             {ceiling_sec} s — a larger need is excluded or \
             pre-registered, never clamped"
        )));
    }
    let enforced_ms = (declared_sec as u128)
        .checked_mul(multiplier_ppm as u128)
        .and_then(|v| v.checked_div(1_000_000))
        .and_then(|v| v.checked_mul(1000))
        .and_then(|v| u64::try_from(v).ok())
        .ok_or_else(|| {
            unsupported(format!(
                "declared {declared_sec} s × multiplier {multiplier_ppm} \
                 ppm overflows — a deadline the arithmetic cannot hold \
                 is malformed"
            ))
        })?;
    if enforced_ms > ceiling_sec * 1000 {
        return Err(unsupported(format!(
            "enforced {enforced_ms} ms exceeds the protocol ceiling \
             {ceiling_sec} s — a multiplier that pushes the declared \
             bound over the ceiling is refused, never clamped"
        )));
    }
    Ok(enforced_ms)
}

/// `PhaseDeadline` — one phase's declared deadline: `declared_sec`
/// verbatim from the task declaration and the run's declared
/// `multiplier_ppm` (canonical 1 000 000).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseDeadline {
    /// The declared seconds (task.toml verbatim).
    pub declared_sec: u64,
    /// The declared multiplier in ppm of 1.0.
    pub multiplier_ppm: u64,
}

/// `TaskDeadlineAdmission` — the declared scope the stamped record
/// carries verbatim (F13 §4): the named task, the dataset pin, the
/// task digest, the protocol, the per-phase declarations and the
/// protocol ceiling.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskDeadlineAdmission {
    /// The named task — the admission's scope (`record.ext["task_id"]`
    /// must equal it verbatim, fail-closed as the verifier-bound
    /// admission does).
    pub task_id: String,
    /// The dataset pin the deadline bytes were read under.
    pub dataset_pin: String,
    /// The pinned task digest the declaration was read from.
    pub task_digest: String,
    /// The protocol whose ceiling bounds the admission.
    pub protocol_id: String,
    /// The agent-phase declaration.
    pub agent: PhaseDeadline,
    /// The verifier-phase declaration.
    pub verifier: PhaseDeadline,
    /// The protocol ceiling (seconds) every enforced value stays under.
    pub ceiling_sec: u64,
}

/// `admit_task_deadlines(record, admission)` — the transactional
/// stamp: every leg is checked before anything is written, so a
/// refused admission leaves the spec byte-identical (never a
/// half-admitted bound). On admission `ext["deadline_admission"]`
/// carries `{source, dataset_pin, task_digest, protocol_id, agent{
/// declared_sec, multiplier_ppm, enforced_ms}, verifier{…},
/// ceiling_sec}` verbatim.
///
/// Refusals (`Unsupported{deadline.task_declared}`): empty or
/// mismatched `task_id`, a spec carrying no `task_id` member, empty
/// `dataset_pin`/`task_digest`/`protocol_id`, and any declaration
/// [`task_declared_deadline_ms`] refuses.
pub fn admit_task_deadlines(
    record: &mut EnvironmentRecord,
    admission: &TaskDeadlineAdmission,
) -> Result<(), EnvError> {
    if admission.task_id.is_empty() {
        return Err(unsupported(
            "the admission names no task — a deadline admission binds a \
             named-task scope",
        ));
    }
    match record.ext.get("task_id") {
        Some(Json::Str(t)) if t == &admission.task_id => {}
        Some(Json::Str(t)) => {
            return Err(unsupported(format!(
                "the admission names task `{}` but this spec is `{t}` — \
                 `{t}` keeps its own deadline path",
                admission.task_id
            )));
        }
        _ => {
            return Err(unsupported(
                "the spec carries no `task_id` member — the named-task \
                 scope cannot be verified (fail-closed)",
            ));
        }
    }
    for (member, value) in [
        ("dataset_pin", &admission.dataset_pin),
        ("task_digest", &admission.task_digest),
        ("protocol_id", &admission.protocol_id),
    ] {
        if value.is_empty() {
            return Err(unsupported(format!(
                "the admission's `{member}` member is empty — the record \
                 carries the verbatim pins or nothing is stamped"
            )));
        }
    }
    // Derive both enforced values *before* stamping — a refusal leaves
    // the record untouched.
    let agent_ms = task_declared_deadline_ms(
        admission.agent.declared_sec,
        admission.agent.multiplier_ppm,
        admission.ceiling_sec,
    )?;
    let verifier_ms = task_declared_deadline_ms(
        admission.verifier.declared_sec,
        admission.verifier.multiplier_ppm,
        admission.ceiling_sec,
    )?;
    let phase = |name: &'static str, d: &PhaseDeadline, enforced_ms: u64| {
        (
            name,
            Json::obj([
                ("declared_sec", Json::Int(d.declared_sec as i64)),
                ("multiplier_ppm", Json::Int(d.multiplier_ppm as i64)),
                ("enforced_ms", Json::Int(enforced_ms as i64)),
            ]),
        )
    };
    record.ext.insert(
        ADMISSION_FIELD.to_string(),
        Json::obj([
            ("source", Json::str(SOURCE_TASK_TOML)),
            ("task_id", Json::str(admission.task_id.clone())),
            ("dataset_pin", Json::str(admission.dataset_pin.clone())),
            ("task_digest", Json::str(admission.task_digest.clone())),
            ("protocol_id", Json::str(admission.protocol_id.clone())),
            phase("agent", &admission.agent, agent_ms),
            phase("verifier", &admission.verifier, verifier_ms),
            ("ceiling_sec", Json::Int(admission.ceiling_sec as i64)),
        ]),
    );
    Ok(())
}

/// `task_deadline_echo(record)` — the per-slot echo members: declared
/// and enforced deadlines verbatim under the stamped admission. A spec
/// without the admission echoes nothing — the echo exists only because
/// the admission does.
pub fn task_deadline_echo(record: &EnvironmentRecord) -> Vec<(&'static str, Json)> {
    let Some(adm) = record.ext.get(ADMISSION_FIELD) else {
        return Vec::new();
    };
    let mut echo = Vec::new();
    for (member, key) in [
        ("agent", "deadline_agent"),
        ("verifier", "deadline_verifier"),
    ] {
        if let Some(Json::Obj(p)) = adm.get(member) {
            echo.push((key, Json::Obj(p.clone())));
        }
    }
    if let Some(v) = adm.get("ceiling_sec") {
        echo.push(("deadline_ceiling_sec", v.clone()));
    }
    echo
}
