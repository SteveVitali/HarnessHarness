//! The P-01…P-16 probe catalogue + driver (§6.6 §6; R-2.10.6; S4.5a).
//!
//! A probe **exercises the ABI** and reports what it observed — the same
//! drive for every participant; the participant's behavior determines the
//! verdict, never its declaration (the reconcile does the compare). Verdicts:
//!
//! * `supported` — the probe observed the capability working;
//! * `unsupported` — the probe ran and observed non-support;
//! * `partial` — the probe observed the surface but not the full contract
//!   (probe-only verdict — never a declaration value);
//! * `not_applicable` — the dimension cannot apply to this mechanism;
//! * `unknown` — the probe could not reach a determination;
//! * `skipped` — the environment **cannot exercise** the probe (never
//!   coerced to `unsupported` — AC-R-2.10.6-3).
//!
//! `p0` marks the conformance floor — `{P-01, P-03, P-04, P-06, P-09, P-16}`:
//! a P0 DRIFT quarantines the participant version (the registry act; the
//! probe only produces the entry).
//!
//! `applies_to` lists the mechanisms the probe can exercise — a probe
//! outside it returns `skipped` with the mechanism named (never `unknown`).

use hh_ontology::participant::HostingMechanism;
use hh_wire::Json;

use crate::abi::HostingError;
use crate::service::HostingService;

/// One catalogue row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeSpec {
    /// The probe id (`P-01`…`P-16`).
    pub id: &'static str,
    /// The capability dimension it measures.
    pub dimension: &'static str,
    /// `true` = conformance floor (a DRIFT quarantines).
    pub p0: bool,
    /// The mechanisms the probe can exercise (empty = all).
    pub applies_to: &'static [HostingMechanism],
}

use HostingMechanism as M;

/// The session-ABI surface — probes that need a live session.
const SESSION: &[M] = &[M::SessionAbi];
/// Session or container (a driven process either way).
const PROCESS: &[M] = &[M::SessionAbi, M::ContainerInstalled];
/// Interception-capable mechanisms.
const INTERCEPT: &[M] = &[M::SessionAbi, M::ModelBoundaryIntercept];

/// The closed catalogue — sixteen probes, ids `P-01`…`P-16`.
pub const PROBE_CATALOGUE: &[ProbeSpec] = &[
    ProbeSpec {
        id: "P-01",
        dimension: "basic_turn",
        p0: true,
        applies_to: PROCESS,
    },
    ProbeSpec {
        id: "P-02",
        dimension: "streaming",
        p0: false,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-03",
        dimension: "interrupt",
        p0: true,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-04",
        dimension: "tool_calling_supplied",
        p0: true,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-05",
        dimension: "tool_calling_own",
        p0: false,
        applies_to: PROCESS,
    },
    ProbeSpec {
        id: "P-06",
        dimension: "policy_deny",
        p0: true,
        applies_to: INTERCEPT,
    },
    ProbeSpec {
        id: "P-07",
        dimension: "policy_ask",
        p0: false,
        applies_to: INTERCEPT,
    },
    ProbeSpec {
        id: "P-08",
        dimension: "context_delivery",
        p0: false,
        applies_to: PROCESS,
    },
    ProbeSpec {
        id: "P-09",
        dimension: "coordinate_model",
        p0: true,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-10",
        dimension: "coordinate_other",
        p0: false,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-11",
        dimension: "usage_reporting",
        p0: false,
        applies_to: PROCESS,
    },
    ProbeSpec {
        id: "P-12",
        dimension: "resume_cold",
        p0: false,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-13",
        dimension: "resume_warm",
        p0: false,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-14",
        dimension: "steer",
        p0: false,
        applies_to: SESSION,
    },
    ProbeSpec {
        id: "P-15",
        dimension: "credential_channel",
        p0: false,
        applies_to: PROCESS,
    },
    ProbeSpec {
        id: "P-16",
        dimension: "end_state",
        p0: true,
        applies_to: PROCESS,
    },
];

/// Look up a probe by id (`P-01`) **or** dimension (`basic_turn`).
pub fn probe_spec(id_or_dimension: &str) -> Option<&'static ProbeSpec> {
    PROBE_CATALOGUE
        .iter()
        .find(|p| p.id == id_or_dimension || p.dimension == id_or_dimension)
}

/// The P0 dimensions (the quarantine set — the boundary filters on this).
/// The canonical set lives in `hh_ledger::hosted` (CC7 — one schema source:
/// the catalogue marks `p0` flags, the ledger module is the set the
/// registry/embed boundary consumes); re-exported here so probe callers
/// spell it `probes::P0_DIMENSIONS`.
pub use hh_ledger::hosted::{is_p0, P0_DIMENSIONS};

/// What a probe produced — the observed record, records-out.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeOutcome {
    /// The verdict spelling.
    pub verdict: String,
    /// The observed capability state (the value the conformance entry's
    /// `observed` member carries — the verdict's lowercase twin, or a
    /// structured value).
    pub observed: Json,
    /// A short honest note (data, never a claim).
    pub note: String,
    /// The detail ref (session/event pointer) when there is one.
    pub detail_ref: Option<String>,
}

impl ProbeOutcome {
    fn v(verdict: &str, note: impl Into<String>) -> ProbeOutcome {
        ProbeOutcome {
            verdict: verdict.to_string(),
            observed: Json::str(verdict),
            note: note.into(),
            detail_ref: None,
        }
    }

    /// The probe's Json form (the plane seam result).
    pub fn to_json(&self, spec: &ProbeSpec) -> Json {
        Json::obj([
            ("probe_id", Json::str(spec.id)),
            ("dimension", Json::str(spec.dimension)),
            ("verdict", Json::str(&self.verdict)),
            ("observed", self.observed.clone()),
            ("note", Json::str(&self.note)),
            (
                "detail_ref",
                self.detail_ref
                    .as_ref()
                    .map_or(Json::Null, |r| Json::str(r.clone())),
            ),
            ("p0_dimension", Json::Bool(spec.p0)),
        ])
    }
}

/// `run_probe(service, spec)` — exercise `spec.dimension` against a fresh
/// session and read the event log. Every probe opens its own session on the
/// service (sessions are cheap and hermetic); the outcome carries the
/// session ref as `detail_ref`.
pub fn run_probe(svc: &mut HostingService, spec: &ProbeSpec) -> Result<ProbeOutcome, HostingError> {
    let mechanism = svc.participant().descriptor.hosting_mechanism;
    if !spec.applies_to.is_empty() && !spec.applies_to.contains(&mechanism) {
        return Ok(ProbeOutcome::v(
            "skipped",
            format!(
                "probe {} cannot exercise mechanism {}",
                spec.id,
                mechanism.as_str()
            ),
        ));
    }
    let mut outcome = match spec.dimension {
        "basic_turn" => probe_basic_turn(svc),
        "streaming" => probe_streaming(svc),
        "interrupt" => probe_interrupt(svc),
        "tool_calling_supplied" => probe_tool_supplied(svc),
        "tool_calling_own" => probe_tool_own(svc),
        "policy_deny" => probe_policy(svc, "deny"),
        "policy_ask" => probe_policy(svc, "ask"),
        "context_delivery" => probe_context(svc),
        "coordinate_model" => probe_coordinate(svc, "model"),
        "coordinate_other" => probe_coordinate(svc, "mode"),
        "usage_reporting" => probe_usage(svc),
        "resume_cold" => probe_resume(svc, "cold"),
        "resume_warm" => probe_resume(svc, "warm"),
        "steer" => probe_steer(svc),
        "credential_channel" => probe_credential(svc),
        "end_state" => probe_end_state(svc),
        _ => Ok(ProbeOutcome::v("unknown", "uncatalogued dimension")),
    }?;
    // A probe that could not open a session reports `unknown`, never a
    // guess — the open refusal is data.
    if outcome.detail_ref.is_none() && outcome.verdict != "skipped" {
        outcome.note = format!("{} [{}]", outcome.note, spec.id);
    }
    Ok(outcome)
}

/// The probe spec used to open probe sessions (the minimal run spec — a
/// definition ref the Lab owns and an empty connection).
fn probe_run_spec() -> crate::abi::HostedRunSpec {
    crate::abi::HostedRunSpec {
        definition_ref: "hh.probe/1".into(),
        params: Json::obj([]),
        placement: crate::records::ProcessPlacement::InEnvironment,
        connection_info: Json::obj([]),
        context_items: Vec::new(),
        budget_view: None,
        credential_channels: Vec::new(),
        resume_cursor: None,
    }
}

fn kinds(svc: &HostingService, session: &str) -> Vec<String> {
    svc.stream_events(session)
        .map(|ev| ev.iter().map(|e| e.kind.clone()).collect())
        .unwrap_or_default()
}

fn probe_basic_turn(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    let tid = svc.submit(&opened.session_ref, &Json::str("probe: turn"))?;
    let finished = kinds(svc, &opened.session_ref)
        .iter()
        .zip(svc.stream_events(&opened.session_ref).unwrap().iter())
        .any(|(_, e)| {
            e.kind == "turn.finished"
                && e.payload.get("turn_id").and_then(Json::as_str) == Some(tid.as_str())
        });
    svc.close(&opened.session_ref).ok();
    let mut o = if finished {
        ProbeOutcome::v("supported", "a prompt turn finished")
    } else {
        ProbeOutcome::v("unsupported", "the turn never finished")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_streaming(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: stream"))?;
    let deltas = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .filter(|e| e.kind == "message.delta")
        .count();
    svc.close(&opened.session_ref).ok();
    let mut o = if deltas > 0 {
        ProbeOutcome::v("supported", format!("{deltas} message.delta rows"))
    } else {
        ProbeOutcome::v("unsupported", "no message.delta rows observed")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_interrupt(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: interrupt"))?;
    let cancelled = svc.cancel(&opened.session_ref)?;
    let saw_cancel = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| {
            e.kind == "turn.finished"
                && e.payload.get("stop_reason_raw").and_then(Json::as_str) == Some("cancelled")
        });
    svc.close(&opened.session_ref).ok();
    let mut o = if cancelled && saw_cancel {
        ProbeOutcome::v("supported", "cancel observed the turn's cancellation")
    } else if cancelled {
        ProbeOutcome::v(
            "partial",
            "cancel returned but no cancelled finish observed",
        )
    } else {
        ProbeOutcome::v("unsupported", "the participant ignored the cancel")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_tool_supplied(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    // A *supplied* tool: the Lab's sealed-MCP surface — the service observes
    // the tool call on the protocol channel (the fixture's lab_supplied
    // scripted tool) with a permission round-trip.
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: supplied tool"))?;
    let evs = svc.stream_events(&opened.session_ref).unwrap();
    let saw_request = evs.iter().any(|e| e.kind == "permission.requested");
    let saw_tool = evs.iter().any(|e| e.kind == "tool.completed");
    svc.close(&opened.session_ref).ok();
    let mut o = if saw_request && saw_tool {
        ProbeOutcome::v(
            "supported",
            "a supplied-tool call went through permission then completed",
        )
    } else if saw_tool && !saw_request {
        // A completed tool call with NO permission surface — the hazard
        // case the probe exists to catch (auto-approval performed, never
        // `supported` for a *mediated* supplied tool).
        ProbeOutcome::v(
            "unsupported",
            "tool call completed with no permission surface (unmediated)",
        )
    } else {
        ProbeOutcome::v("unsupported", "no supplied tool call observed")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_tool_own(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: own tool"))?;
    let saw = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| e.kind == "tool.proposed" || e.kind == "tool.completed");
    svc.close(&opened.session_ref).ok();
    let mut o = if saw {
        ProbeOutcome::v("supported", "participant tool calls observed")
    } else {
        ProbeOutcome::v(
            "not_applicable",
            "the participant made no tool calls this turn",
        )
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_policy(svc: &mut HostingService, mode: &str) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(
        &opened.session_ref,
        &Json::str(format!("probe: policy {mode}")),
    )?;
    let (has_requested, decisions): (bool, Vec<String>) = {
        let evs = svc.stream_events(&opened.session_ref).unwrap();
        (
            evs.iter().any(|e| e.kind == "permission.requested"),
            evs.iter()
                .filter(|e| e.kind == "permission.decided")
                .filter_map(|e| {
                    e.payload
                        .get("decision")
                        .and_then(Json::as_str)
                        .map(String::from)
                })
                .collect(),
        )
    };
    svc.close(&opened.session_ref).ok();
    let mut o = match mode {
        "deny" => {
            if decisions.iter().any(|d| d == "deny") {
                ProbeOutcome::v("supported", "a denied effect was refused at the boundary")
            } else if decisions.is_empty() {
                ProbeOutcome::v(
                    "unsupported",
                    "no permission decision reached the participant (unmediated)",
                )
            } else {
                ProbeOutcome::v("partial", "decisions observed but none denied")
            }
        }
        _ => {
            // `ask`: pending → a decision is still required; support is a
            // decided row produced after the request.
            if has_requested && !decisions.is_empty() {
                ProbeOutcome::v("supported", "the ask path produced a decision")
            } else {
                ProbeOutcome::v("unsupported", "the ask path produced no decision")
            }
        }
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_context(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let mut spec = probe_run_spec();
    spec.context_items = vec![Json::obj([
        ("kind", Json::str("instruction")),
        ("value", Json::str("probe-context")),
    ])];
    let opened = match svc.open(spec) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    let delivered = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| e.kind == "context_delivered" || e.kind == "artefact.delivered");
    svc.close(&opened.session_ref).ok();
    let mut o = if delivered {
        ProbeOutcome::v("supported", "context items acknowledged delivered")
    } else {
        ProbeOutcome::v("unsupported", "no delivery acknowledgement observed")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_coordinate(svc: &mut HostingService, which: &str) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    let applied = svc
        .set_coordinate(&opened.session_ref, which, &Json::str("probe-value"))
        .is_ok();
    let confirmed = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| e.kind == "coordinate.changed");
    svc.close(&opened.session_ref).ok();
    let mut o = if applied && confirmed {
        ProbeOutcome::v("supported", format!("set_coordinate({which}) took effect"))
    } else if applied {
        ProbeOutcome::v("partial", "accepted without an observable effect")
    } else {
        ProbeOutcome::v("unsupported", "set_coordinate refused or had no effect")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_usage(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: usage"))?;
    let reported = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| e.kind == "usage.reported");
    svc.close(&opened.session_ref).ok();
    let mut o = if reported {
        ProbeOutcome::v("supported", "usage.reported observed")
    } else {
        ProbeOutcome::v("unsupported", "no usage report observed")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_resume(svc: &mut HostingService, mode: &str) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: resume"))?;
    svc.close(&opened.session_ref).ok();
    let cursor = crate::abi::ResumeCursor {
        session_ref: opened.session_ref.clone(),
        seq: 0,
        mode: mode.to_string(),
        ext: std::collections::BTreeMap::new(),
    };
    match svc.resume(&cursor) {
        Ok(new_session) => {
            svc.close(&new_session.session_ref).ok();
            Ok(ProbeOutcome {
                verdict: "supported".into(),
                observed: Json::str("supported"),
                note: format!("{mode} resume continued the session"),
                detail_ref: Some(new_session.session_ref),
            })
        }
        Err(HostingError::CapabilityNotSupported { .. }) => Ok(ProbeOutcome {
            verdict: "unsupported".into(),
            observed: Json::str("unsupported"),
            note: format!("{mode} resume is undeclared"),
            detail_ref: Some(opened.session_ref),
        }),
        Err(HostingError::Transport { detail }) => Ok(ProbeOutcome {
            verdict: "unsupported".into(),
            observed: Json::str("unsupported"),
            note: format!("{mode} resume refused: {detail}"),
            detail_ref: Some(opened.session_ref),
        }),
        Err(e) => Err(e),
    }
}

fn probe_steer(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    let tid = svc.submit(&opened.session_ref, &Json::str("probe: steer"))?;
    let running = svc
        .stream_events(&opened.session_ref)
        .map(|ev| {
            !ev.iter().any(|e| {
                e.kind == "turn.finished"
                    && e.payload.get("turn_id").and_then(Json::as_str) == Some(tid.as_str())
            })
        })
        .unwrap_or(false);
    let mut o = if !running {
        ProbeOutcome::v(
            "skipped",
            "the turn finished before the steer window — the environment \
             cannot exercise mid-turn steering on this participant",
        )
    } else {
        match svc.steer(&opened.session_ref, &tid, "probe-steer") {
            Ok(()) => {
                let steered = svc
                    .stream_events(&opened.session_ref)
                    .unwrap()
                    .iter()
                    .any(|e| {
                        e.payload
                            .get("text")
                            .and_then(Json::as_str)
                            .map(|t| t.contains("steered:"))
                            .unwrap_or(false)
                    });
                if steered {
                    ProbeOutcome::v("supported", "the steer changed the turn's output")
                } else {
                    ProbeOutcome::v("unsupported", "steer accepted but produced no effect")
                }
            }
            Err(HostingError::CapabilityNotSupported { .. }) => {
                ProbeOutcome::v("unsupported", "steer is undeclared")
            }
            Err(_) => ProbeOutcome::v("unsupported", "steer refused"),
        }
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_credential(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let mut spec = probe_run_spec();
    spec.credential_channels = svc
        .participant()
        .hosting_ext
        .credential_supply
        .clone()
        .unwrap_or_default();
    if spec.credential_channels.is_empty() {
        return Ok(ProbeOutcome::v(
            "skipped",
            "the participant declares no credential_supply channels",
        ));
    }
    let channel = spec.credential_channels[0].clone();
    let opened = match svc.open(spec) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    let delivered = svc
        .deliver_credential(&opened.session_ref, &channel, "cred-ref:1")
        .is_ok();
    let received = svc
        .stream_events(&opened.session_ref)
        .unwrap()
        .iter()
        .any(|e| e.kind == "credential_received");
    svc.close(&opened.session_ref).ok();
    let mut o = if delivered && received {
        ProbeOutcome::v("supported", "a credential ref flowed on a declared channel")
    } else if delivered {
        ProbeOutcome::v("partial", "delivered without an observable acknowledgement")
    } else {
        ProbeOutcome::v("unsupported", "the credential channel refused")
    };
    o.detail_ref = Some(opened.session_ref);
    Ok(o)
}

fn probe_end_state(svc: &mut HostingService) -> Result<ProbeOutcome, HostingError> {
    let opened = match svc.open(probe_run_spec()) {
        Ok(o) => o,
        Err(e) => return Ok(ProbeOutcome::v("unknown", format!("open refused: {e}"))),
    };
    svc.submit(&opened.session_ref, &Json::str("probe: end state"))?;
    match svc.close(&opened.session_ref) {
        Ok(end) => {
            let coherent = end.turns >= 1 && !end.reason.is_empty();
            let mut o = if coherent {
                ProbeOutcome::v(
                    "supported",
                    format!("end state: reason={} turns={}", end.reason, end.turns),
                )
            } else {
                ProbeOutcome::v("unsupported", "the end-state snapshot is incoherent")
            };
            o.detail_ref = Some(opened.session_ref);
            Ok(o)
        }
        Err(e) => Ok(ProbeOutcome::v(
            "unsupported",
            format!("close produced no end state: {e}"),
        )),
    }
}

/// The plane-seam entry — `handle("probe"|"drive_probe", {dimension})` → the
/// outcome Json (the boundary appends the conformance entry).
pub fn drive_probe_json(svc: &mut HostingService, params: &Json) -> Result<Json, HostingError> {
    let dim = params
        .get("dimension")
        .and_then(Json::as_str)
        .ok_or_else(|| HostingError::SchemaViolation {
            path: "dimension".into(),
            detail: "expected string".into(),
        })?;
    let spec = probe_spec(dim).ok_or_else(|| HostingError::SchemaViolation {
        path: "dimension".into(),
        detail: format!("no probe catalogued for {dim}"),
    })?;
    let outcome = run_probe(svc, spec)?;
    Ok(outcome.to_json(spec))
}
