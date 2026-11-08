//! §5.7 admission smoke battery + §5.8 hosting-failure classifier
//! (R6.25 stage 2; REQ-XC-324/329, REQ-XC-311 absorbing REQ-XC-205,
//! REQ-XC-312, REQ-XC-321, REQ-XC-203, REQ-XC-323; design/34 §1.9–§1.10
//! + F15/F16; hints 6–7).
//!
//! The battery is committed machinery: the HH-authored task
//! `hh-smoke/write-file` (never a TB task), one scripted
//! `hh.model_proxy.recording/1` per first-pass harness in its own tool
//! schema and streaming mode, the declared replay settings
//! (`keying: sequence`, `on_divergence: serve_recorded`) and the
//! offline records-in checker computing every §5.7 assertion over slot
//! records. It executes at $0 on the phase backend only in W1 (R6.32);
//! until then every record here is `fixture-verified` (X-08).
//!
//! The §5.8 classifier is pure over recorded facts: every hosting
//! failure maps to its disposition through [`classify_hosting`] — a
//! non-zero exit after a model-route failure is
//! `inconclusive{infra:model_route:<disposition>}` (engine replan),
//! never a task loss.

use hharness_wire::json::Json;

use crate::admission_build::InstallStep;
use crate::capture::NativeCall;
use crate::disposition::{
    AdmissionReason, Disposition, EndCause, InconclusiveCause, InfraCause, SlotFlag, WithheldReason,
};
use crate::json_ext::JsonExt;
use crate::slot_record::ExperimentSlot;

/// The HH-authored smoke task id (never a TB task).
pub const SMOKE_TASK_ID: &str = "hh-smoke/write-file";
/// The smoke-battery plan schema spelling.
pub const BATTERY_SCHEMA: &str = "hh-smoke-battery/1";
/// The recording schema the committed scripts spell (R6.12's format —
/// the strict codec is `hh.model_proxy.recording/1` in hh-hosting; the
/// runner cannot name it (K-2), so the check below is a records-level
/// member assertion, never a second codec).
pub const RECORDING_SCHEMA: &str = "hh.model_proxy.recording/1";
/// The declared replay keying (§5.7).
pub const REPLAY_KEYING: &str = "sequence";
/// The declared divergence policy (§5.7).
pub const REPLAY_ON_DIVERGENCE: &str = "serve_recorded";
/// The R6.11 release modes the model socket serves (SM-slow leg).
pub const RELEASE_MODES: [&str; 2] = ["buffered", "buffered_heartbeat"];
/// The §4.3 dispositions a *model-route failure* ends in — a non-zero
/// exit after one is `inconclusive{infra:model_route:<d>}` (§5.8 row 4).
pub const MODEL_ROUTE_FAILURES: [&str; 5] = [
    "upstream_error",
    "upstream_timeout",
    "stream_broken",
    "proxy_aborted",
    "refused_auth",
];

/// A typed smoke-check failure — the checker returns one per failed
/// assertion (never a silent pass, never a relabel).
#[derive(Debug, Clone, PartialEq)]
pub struct SmokeFailure {
    /// The assertion leg.
    pub check: &'static str,
    /// The verbatim detail.
    pub detail: String,
}

fn fail(check: &'static str, detail: impl Into<String>) -> SmokeFailure {
    SmokeFailure {
        check,
        detail: detail.into(),
    }
}

// ── the battery (task, recordings, replay settings, variant table) ───────────

/// The §5.7 variants — the closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SmokeVariant {
    /// `SM-pos` — the positive run.
    Pos,
    /// `SM-429` — first call answered 429 + `Retry-After`.
    R429,
    /// `SM-402` — call 2 answered as a slot budget refusal.
    R402,
    /// `SM-slow` — long recorded latency under `buffered*` release.
    Slow,
    /// `SM-hang` — `sleep 100000` under a 60 s deadline.
    Hang,
}

impl SmokeVariant {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SmokeVariant::Pos => "SM-pos",
            SmokeVariant::R429 => "SM-429",
            SmokeVariant::R402 => "SM-402",
            SmokeVariant::Slow => "SM-slow",
            SmokeVariant::Hang => "SM-hang",
        }
    }

    /// Every member — the closed set.
    pub const ALL: [SmokeVariant; 5] = [
        SmokeVariant::Pos,
        SmokeVariant::R429,
        SmokeVariant::R402,
        SmokeVariant::Slow,
        SmokeVariant::Hang,
    ];

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<SmokeVariant> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

/// `replay_decl(variant)` — the declared replay settings (§5.7):
/// `keying: sequence`, `on_divergence: serve_recorded`, `latency:
/// immediate` except SM-slow's `recorded` (P9's timing leg).
pub fn replay_decl(variant: SmokeVariant) -> Json {
    Json::obj([
        ("keying", Json::str(REPLAY_KEYING)),
        ("on_divergence", Json::str(REPLAY_ON_DIVERGENCE)),
        (
            "latency",
            Json::str(if variant == SmokeVariant::Slow {
                "recorded"
            } else {
                "immediate"
            }),
        ),
    ])
}

/// `check_battery(doc)` — the committed `hh-smoke-battery/1` plan:
/// the smoke task id, the replay settings, all five variants and one
/// recording per first-pass harness.
pub fn check_battery(doc: &Json, harnesses: &[&str]) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    let m = match doc.as_obj() {
        Some(m) => m,
        None => return vec![fail("battery", "plan must be an object")],
    };
    if m.get("schema").and_then(Json::as_str) != Some(BATTERY_SCHEMA) {
        out.push(fail("battery", "schema is not hh-smoke-battery/1"));
    }
    if m.get("task").and_then(Json::as_str) != Some(SMOKE_TASK_ID) {
        out.push(fail("task", "task is not hh-smoke/write-file"));
    }
    for (k, want) in [
        ("keying", REPLAY_KEYING),
        ("on_divergence", REPLAY_ON_DIVERGENCE),
    ] {
        let got = m
            .get("replay")
            .and_then(Json::as_obj)
            .and_then(|r| r.get(k))
            .and_then(Json::as_str);
        if got != Some(want) {
            out.push(fail("replay", format!("replay.{k} must be {want}")));
        }
    }
    let variants: Vec<&str> = m
        .get("variants")
        .and_then(Json::as_arr)
        .map(|a| a.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    for v in SmokeVariant::ALL {
        if !variants.contains(&v.as_str()) {
            out.push(fail("variants", format!("variant {} absent", v.as_str())));
        }
    }
    let recs = m.get("recordings").and_then(Json::as_obj);
    for h in harnesses {
        match recs.and_then(|r| r.get(*h)).and_then(Json::as_str) {
            Some(s) if !s.is_empty() => {}
            _ => out.push(fail("recordings", format!("recording for {h} absent"))),
        }
    }
    out
}

/// `recording_shape(doc)` — the records-level member assertion over a
/// committed scripted recording: the R6.12 document shape
/// (`schema/slot_id/provenance/calls[]`; per-call `k`, `downstream`,
/// `upstream_request`, `upstream_response`, `failure`, `released`,
/// `disposition`, `latency_ms`; `provenance` ∈ `recorded|synthetic`).
/// Not a codec — the strict parse stays `hh.model_proxy.recording/1`
/// in hh-hosting (CC1); W1's replay runs it.
pub fn recording_shape(doc: &Json) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    let m = match doc.as_obj() {
        Some(m) => m,
        None => return vec![fail("recording", "recording must be an object")],
    };
    if m.get("schema").and_then(Json::as_str) != Some(RECORDING_SCHEMA) {
        out.push(fail(
            "recording",
            "schema is not hh.model_proxy.recording/1",
        ));
    }
    match m.get("provenance").and_then(Json::as_str) {
        Some("recorded") | Some("synthetic") => {}
        _ => out.push(fail("recording", "provenance outside recorded|synthetic")),
    }
    let calls = match m.get("calls").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => a,
        _ => {
            out.push(fail("recording", "calls must be a non-empty array"));
            return out;
        }
    };
    const CALL_MEMBERS: [&str; 8] = [
        "k",
        "downstream",
        "upstream_request",
        "upstream_response",
        "failure",
        "released",
        "disposition",
        "latency_ms",
    ];
    for (i, c) in calls.iter().enumerate() {
        let cm = match c.as_obj() {
            Some(cm) => cm,
            None => {
                out.push(fail("recording", format!("calls[{i}] must be an object")));
                continue;
            }
        };
        for k in cm.keys() {
            if !CALL_MEMBERS.contains(&k.as_str()) {
                out.push(fail("recording", format!("calls[{i}] unknown member {k}")));
            }
        }
        if cm.get("k").and_then(Json::as_int) != Some(i as i64) {
            out.push(fail("recording", format!("calls[{i}].k must be {i}")));
        }
        let dn = cm.get("downstream").and_then(Json::as_obj);
        match dn {
            Some(d)
                if d.get("method").and_then(Json::as_str).is_some()
                    && d.get("path").and_then(Json::as_str).is_some()
                    && d.get("body").and_then(Json::as_obj).is_some() => {}
            _ => out.push(fail(
                "recording",
                format!("calls[{i}].downstream must carry method/path/body"),
            )),
        }
    }
    out
}

/// `recording_surface(doc, want_path, want_stream)` — the scripted
/// recording is in the harness's own tool schema and streaming mode:
/// every call's downstream path is the declared surface route and the
/// request body's `stream` member equals the admission's
/// `client.streaming`.
pub fn recording_surface(doc: &Json, want_path: &str, want_stream: bool) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    let calls = doc
        .get("calls")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default();
    for (i, c) in calls.iter().enumerate() {
        let d = c.get("downstream").and_then(Json::as_obj);
        let path = d.and_then(|x| x.get("path")).and_then(Json::as_str);
        if path != Some(want_path) {
            out.push(fail(
                "surface",
                format!("calls[{i}] path {path:?} — expected {want_path}"),
            ));
        }
        let body_stream = d
            .and_then(|x| x.get("body"))
            .and_then(|b| b.get("utf8"))
            .and_then(Json::as_str)
            .and_then(|s| hharness_wire::json::parse(s).ok())
            .and_then(|j| j.get("stream").and_then(Json::as_bool));
        if let Some(s) = body_stream {
            if s != want_stream {
                out.push(fail(
                    "surface",
                    format!("calls[{i}] stream {s} — admission declares {want_stream}"),
                ));
            }
        }
    }
    out
}

// ── endpoint admission (REQ-XC-203) ──────────────────────────────────────────

/// The surfaces the proxy serves (§5.7's proxied formats).
pub const PROXIED_SURFACES: [&str; 2] = ["openai.chat", "anthropic.messages"];

/// `surface_route(surface)` — the downstream path the surface's model
/// calls arrive on (the model socket's route).
pub fn surface_route(surface: &str) -> Option<&'static str> {
    match surface {
        "openai.chat" => Some("\u{2f}v1\u{2f}chat\u{2f}completions"),
        "anthropic.messages" => Some("\u{2f}v1\u{2f}messages"),
        _ => None,
    }
}

/// `surface_model_hosts(surface)` — the vendor model hosts a surface's
/// calls would otherwise escape to; the mediator must never see a
/// request for them (REQ-XC-312).
pub fn surface_model_hosts(surface: &str) -> Vec<&'static str> {
    match surface {
        "anthropic.messages" => vec!["anthropic.com"],
        "openai.chat" => vec!["openai.com"],
        _ => vec![],
    }
}

/// The `n/a{endpoint}` typed exclusion (REQ-XC-203).
pub const NA_ENDPOINT: &str = "n/a{endpoint}";

/// `endpoint_admission(rec)` — REQ-XC-203: a working custom base URL on
/// a proxied format is required. A harness limited to a Google format
/// (`config.surface` outside the proxied set, e.g. `google.*`) or
/// vendor-hosted inference (`harness.vendor_hosted: true`) is excluded
/// with `n/a{endpoint}` — a typed exclusion, never a slot.
pub fn endpoint_admission(rec: &Json) -> Result<(), String> {
    let hosted = rec
        .get("harness")
        .and_then(|h| h.get("vendor_hosted"))
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let surface = rec
        .get("config")
        .and_then(|c| c.get("surface"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if hosted || !PROXIED_SURFACES.contains(&surface) {
        return Err(NA_ENDPOINT.to_string());
    }
    Ok(())
}

/// `receipt_on_surface(rec, receipts)` — AC-5.1-3's evidence leg: the
/// SM-pos receipts arrive on the proxied surface route at the loopback
/// relay — the working custom base URL is evidenced by a receipt, not
/// asserted.
pub fn receipt_on_surface(rec: &Json, receipts: &[Json]) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    let surface = rec
        .get("config")
        .and_then(|c| c.get("surface"))
        .and_then(Json::as_str)
        .unwrap_or("");
    let Some(route) = surface_route(surface) else {
        out.push(fail(
            "endpoint",
            format!("surface {surface} is not proxied"),
        ));
        return out;
    };
    // The declared refused-route set (Claude Code's `count_tokens`
    // estimate fallback — §5.4): the call still arrived at the relay.
    let allowed: Vec<&str> = match surface {
        "anthropic.messages" => vec![route, "\u{2f}v1\u{2f}messages\u{2f}count_tokens"],
        _ => vec![route],
    };
    for (i, r) in receipts.iter().enumerate() {
        let d = r.get("downstream").and_then(Json::as_obj);
        let host = d.and_then(|x| x.get("host")).and_then(Json::as_str);
        let path = d.and_then(|x| x.get("path")).and_then(Json::as_str);
        if host != Some("127.0.0.1") && host != Some("localhost") {
            out.push(fail(
                "endpoint",
                format!("receipt[{i}] downstream host {host:?} — the custom base URL must reach the loopback relay"),
            ));
        }
        if !allowed.contains(&path.unwrap_or("")) {
            out.push(fail(
                "endpoint",
                format!("receipt[{i}] path {path:?} — expected {route} on {surface}"),
            ));
        }
    }
    out
}

// ── mediator and surface assertions (REQ-XC-311/312) ─────────────────────────

/// A mediator row is a granted (non-denied) egress attempt.
pub fn row_granted(row: &Json) -> bool {
    !matches!(
        row.get("decision").and_then(Json::as_str),
        Some("deny") | Some("denied")
    )
}

fn host_covered(patterns: &[String], host: &str) -> bool {
    patterns
        .iter()
        .any(|p| crate::render::pattern_covers(p, host))
}

/// `expected_denied(rec)` — the admission's declared host patterns.
pub fn expected_denied(rec: &Json) -> Vec<String> {
    rec.get("egress")
        .and_then(|e| e.get("expected_denied"))
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_obj()
                        .and_then(|m| m.get("host_or_pattern"))
                        .and_then(Json::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `mediator_subset(rec, rows)` — REQ-XC-311's SM-pos leg: every
/// agent-phase non-model attempt is a logged residual — every denied
/// host is covered by `expected_denied` (the undeclared-denial row is
/// §5.8's, handled by the classifier's flag leg) and no request was
/// granted a non-model host (AG5: a harness that completes only with
/// one granted fails the gate and is granted no host).
pub fn mediator_subset(rec: &Json, rows: &[Json]) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    let expected = expected_denied(rec);
    for (i, row) in rows.iter().enumerate() {
        let m = match row.as_obj() {
            Some(m) => m,
            None => {
                out.push(fail("mediator_row", format!("row[{i}] must be an object")));
                continue;
            }
        };
        let host = m.get("host").and_then(Json::as_str).unwrap_or("");
        if row_granted(row) {
            out.push(fail(
                "ag5_granted",
                format!("row[{i}] granted non-model host {host} — AG5 fails, no host is granted"),
            ));
        } else if !host_covered(&expected, host) {
            out.push(fail(
                "expected_denied",
                format!("row[{i}] denied host {host} not covered by expected_denied"),
            ));
        }
    }
    out
}

/// `model_socket_assert(rec, rows, receipts)` — REQ-XC-312: every
/// model call arrived on the model socket (loopback relay + the
/// surface route) and the mediator saw no request for the relay
/// address or the model host.
pub fn model_socket_assert(rec: &Json, rows: &[Json], receipts: &[Json]) -> Vec<SmokeFailure> {
    let mut out = receipt_on_surface(rec, receipts);
    let surface = rec
        .get("config")
        .and_then(|c| c.get("surface"))
        .and_then(Json::as_str)
        .unwrap_or("");
    // The relay address never appears at all (the model socket is
    // direct); a CONNECT tunnel or a granted request to a model host is
    // the REQ-XC-312 violation — denied rows to the vendor host are the
    // declared expected_denied residuals (REQ-XC-311/327), not breaches.
    let banned: Vec<String> = ["127.0.0.1", "localhost"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let models: Vec<String> = surface_model_hosts(surface)
        .into_iter()
        .flat_map(|h| [h.to_string(), format!("api.{h}")])
        .collect();
    let is_model = |host: &str| {
        models
            .iter()
            .any(|b| host == b || host.ends_with(&format!(".{b}")))
    };
    for (i, row) in rows.iter().enumerate() {
        let host = row.get("host").and_then(Json::as_str).unwrap_or("");
        let bare = host.split(':').next().unwrap_or(host);
        let method = row.get("method").and_then(Json::as_str).unwrap_or("");
        let relay_hit = banned
            .iter()
            .any(|b| bare == b || host.starts_with(&format!("{b}:")));
        let model_breach = is_model(bare) && (method == "CONNECT" || row_granted(row));
        if relay_hit || model_breach {
            out.push(fail(
                "model_host",
                format!("row[{i}] {method} {host} — the mediator must never see the relay address or a model-host tunnel/grant"),
            ));
        }
    }
    out
}

// ── the per-variant records-in checker (REQ-XC-324) ──────────────────────────

/// The records a variant check consumes — all committed records, never
/// live state (X-08: `fixture-verified` until W1 executes).
#[derive(Debug, Clone, Default)]
pub struct SmokeRecords {
    /// The `hh-experiment-slot/1` record the variant produced.
    pub slot: Option<ExperimentSlot>,
    /// The receipt `proxy` views in `slot_call_seq` order.
    pub receipts: Vec<Json>,
    /// The mediator rows (`{host, method?, decision}`).
    pub mediator_rows: Vec<Json>,
    /// The native-log call list the copy-out parsed (REQ-XC-207 input).
    pub native_calls: Vec<NativeCall>,
    /// The scripted recording document.
    pub recording: Option<Json>,
    /// The ST2 baseline.
    pub st2: Option<crate::admission_build::St2Observation>,
    /// The post-agent ST4 probe.
    pub st4: Option<crate::capture::St4Observation>,
    /// The process group was killed on deadline lapse (SM-hang leg).
    pub agent_terminated: bool,
    /// `leak_scan` hits over the receipts' request bodies.
    pub leak_hits: usize,
}

fn agent_phase(slot: &ExperimentSlot) -> Option<&crate::slot_record::SlotPhase> {
    slot.phases.iter().find(|p| p.phase == "agent")
}

fn end_cause_of(slot: &ExperimentSlot) -> Option<EndCause> {
    agent_phase(slot).and_then(|p| EndCause::parse(&p.end_cause))
}

fn check_receipted(records: &SmokeRecords, out: &mut Vec<SmokeFailure>) {
    let unreceipted = crate::capture::reconcile_receipts(&records.native_calls, &records.receipts);
    if !unreceipted.is_empty() {
        out.push(fail(
            "unreceipted_call",
            format!(
                "{} native calls have no receipt — inconclusive{{infra:unreceipted_call}}",
                unreceipted.len()
            ),
        ));
    }
}

fn common_checks(
    rec: &Json,
    variant: SmokeVariant,
    records: &SmokeRecords,
    out: &mut Vec<SmokeFailure>,
) {
    check_receipted(records, out);
    out.extend(mediator_subset(rec, &records.mediator_rows));
    out.extend(model_socket_assert(
        rec,
        &records.mediator_rows,
        &records.receipts,
    ));
    let Some(slot) = &records.slot else {
        out.push(fail("slot", "no slot record"));
        return;
    };
    // REQ-XC-321 — completion is process exit (exit code recorded),
    // the native status stays descriptive; only the deadline and a
    // budget refusal stop a harness (the end cause is inside the
    // closed ST3 set).
    let phase = agent_phase(slot);
    match end_cause_of(slot) {
        Some(EndCause::Exited(rc)) => {
            if phase.and_then(|p| p.exit_code) != Some(rc) {
                out.push(fail(
                    "exit_code",
                    "exited{rc} without the exit code recorded",
                ));
            }
        }
        Some(EndCause::AgentTimeout) => {
            if phase.and_then(|p| p.deadline_enforced_ms).is_none() {
                out.push(fail(
                    "deadline",
                    "agent_timeout without deadline_enforced_ms — only the task deadline stops a harness",
                ));
            }
        }
        Some(EndCause::BudgetRefused) | Some(EndCause::Crashed) => {}
        None => out.push(fail(
            "end_cause",
            "agent phase end_cause outside the closed set",
        )),
    }
    // The script length equals the receipt count for every variant the
    // run completes normally; injection variants record their own
    // expected count leg below.
    let _ = variant;
}

/// `check_smoke(variant, rec, records)` — the offline records-in
/// checker: every §5.7 assertion for `variant` over the committed
/// fixture records. Returns every failed leg (empty = pass).
pub fn check_smoke(variant: SmokeVariant, rec: &Json, records: &SmokeRecords) -> Vec<SmokeFailure> {
    let mut out = Vec::new();
    common_checks(rec, variant, records, &mut out);
    let slot = records.slot.clone();
    match variant {
        SmokeVariant::Pos => {
            // ST4 re-probe unchanged (version + install tree).
            match (&records.st2, &records.st4) {
                (Some(st2), Some(st4)) => {
                    if !crate::capture::st4_verify(st2, st4).is_empty() {
                        out.push(fail(
                            "self_modified",
                            "post-agent version or install tree changed",
                        ));
                    }
                }
                _ => out.push(fail("st4", "SM-pos lacks the post-agent probe pair")),
            }
            if let Some(slot) = &slot {
                if end_cause_of(slot) != Some(EndCause::Exited(0)) {
                    out.push(fail("end_cause", "SM-pos must end exited{0}"));
                }
                if slot.verdict.as_ref().and_then(|v| v.reward_ppm) != Some(1_000_000) {
                    out.push(fail("reward", "SM-pos reward is not 1 (ppm 1_000_000)"));
                }
            }
            if let Some(recording) = &records.recording {
                let script_len = recording
                    .get("calls")
                    .and_then(Json::as_arr)
                    .map(|a| a.len())
                    .unwrap_or(0);
                if records.receipts.len() != script_len {
                    out.push(fail(
                        "receipts",
                        format!(
                            "receipts {} != script length {script_len}",
                            records.receipts.len()
                        ),
                    ));
                }
            }
            if records.leak_hits > 0 {
                out.push(fail("leak_scan", "leak_scan hits over request bodies"));
            }
            // R6.24's records-level assertion (CC1 reuse; REQ-XC-327).
            if rec
                .get("harness")
                .and_then(|h| h.get("name"))
                .and_then(Json::as_str)
                == Some(crate::admission::CLAUDE_CODE)
            {
                if let Err(e) =
                    crate::render::sm_pos_assert(rec, &records.mediator_rows, &records.receipts)
                {
                    out.push(fail("anthropic", e.detail));
                }
            }
        }
        SmokeVariant::R429 => {
            let first = records.receipts.first();
            let status = first
                .and_then(|r| {
                    r.get("upstream")
                        .or_else(|| r.get("released"))
                        .and_then(|u| u.get("status"))
                })
                .and_then(Json::as_int);
            if status != Some(429) {
                out.push(fail("inject_429", "first call was not answered 429"));
            }
            let retried = records.receipts.len() > 1;
            let exited = slot
                .as_ref()
                .and_then(end_cause_of)
                .map(|e| matches!(e, EndCause::Exited(_)))
                .unwrap_or(false);
            if !retried && !exited {
                out.push(fail(
                    "retry",
                    "the harness neither retried (each a receipt) nor exited",
                ));
            }
        }
        SmokeVariant::R402 => {
            let second = records.receipts.get(1);
            let disp = second
                .and_then(|r| r.get("disposition"))
                .and_then(Json::as_str);
            if disp != Some("refused_budget") {
                out.push(fail("inject_402", "call 2 was not a slot budget refusal"));
            }
            if slot.as_ref().and_then(end_cause_of) != Some(EndCause::BudgetRefused) {
                out.push(fail("end_cause", "SM-402 must end budget_refused"));
            }
            if slot.as_ref().and_then(|s| s.verdict.as_ref()).is_none() {
                out.push(fail("verify", "SM-402 did not verify"));
            }
        }
        SmokeVariant::Slow => {
            for (i, r) in records.receipts.iter().enumerate() {
                let mode = r
                    .get("released")
                    .and_then(|x| x.get("release_mode"))
                    .or_else(|| r.get("release_mode"))
                    .and_then(Json::as_str);
                if let Some(m) = mode {
                    if !RELEASE_MODES.contains(&m) {
                        out.push(fail(
                            "release_mode",
                            format!(
                                "receipt[{i}] release_mode {m} outside buffered|buffered_heartbeat"
                            ),
                        ));
                    }
                }
                let gone = r
                    .get("released")
                    .and_then(|x| x.get("status"))
                    .and_then(Json::as_str)
                    == Some("client_gone");
                if gone {
                    out.push(fail(
                        "client_timeout",
                        format!("receipt[{i}] released client_gone — a timeout below the declared bound"),
                    ));
                }
            }
        }
        SmokeVariant::Hang => {
            if slot.as_ref().and_then(end_cause_of) != Some(EndCause::AgentTimeout) {
                out.push(fail("end_cause", "SM-hang must end agent_timeout"));
            }
            if !records.agent_terminated {
                out.push(fail("terminated", "the agent process group was not killed"));
            }
            if slot.as_ref().and_then(|s| s.verdict.as_ref()).is_none() {
                out.push(fail("verify", "SM-hang did not verify"));
            }
        }
    }
    out
}

// ── native terminal status (REQ-XC-321's descriptive parse) ──────────────────

/// `native_status(harness, tail)` — parse the harness's native terminal
/// status *descriptively* (never gates): the recorded marker names the
/// harness's own completion form.
pub fn native_status(harness: &str, tail: &str) -> Json {
    let (kind, detail) = match harness {
        crate::admission::MINI_SWE_AGENT => {
            if tail.contains("COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT") {
                (
                    "submitted",
                    "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT marker observed",
                )
            } else {
                ("other", "no submit marker in the native tail")
            }
        }
        crate::admission::CLAUDE_CODE => {
            if tail.contains("\"type\":\"result\"") {
                ("result", "stream-json result event observed")
            } else {
                ("other", "no result event in the native tail")
            }
        }
        crate::render::PI => {
            if tail.contains("\"type\":\"agent_end\"") || tail.contains("\"type\":\"end\"") {
                ("agent_end", "pi terminal event observed")
            } else {
                ("other", "no terminal event in the native tail")
            }
        }
        _ => ("unknown", "no native-status parser for this harness"),
    };
    Json::obj([
        ("kind", Json::str(kind)),
        ("detail", Json::str(detail)),
        ("parser", Json::str("descriptive")),
    ])
}

// ── the §5.8 hosting-failure classifier (REQ-XC-323) ─────────────────────────

/// The recorded facts the classifier consumes — the §5.8 events, all
/// records (slot record, receipts, mediator rows, probes), never a
/// live observation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostingFacts {
    /// A bundle/layer build step failed at admission (§5.8 row 1).
    pub build_failure: Option<InstallStep>,
    /// ST2 verify mismatched (row 2).
    pub st2_mismatch: bool,
    /// The ST2 probe crashed with a healthy cell — `infra:harness_setup:<c>` (row 3).
    pub st2_probe_crash: Option<String>,
    /// Relay or cell death (row 13).
    pub environment_died: bool,
    /// The harness log showed a call with no receipt (REQ-XC-207;
    /// precedence over a valid reward like RD-17's premature refusal).
    pub unreceipted_call: bool,
    /// A premature slot budget refusal (§3.3's precedence row — the
    /// classifier inherits it: budget machinery, not the harness).
    pub premature_refusal: bool,
    /// The agent phase hit the deadline (row 6).
    pub agent_timed_out: bool,
    /// A non-premature slot budget refusal (row 7).
    pub budget_refused: bool,
    /// The harness's exit code (rows 4–5).
    pub agent_exit: Option<i64>,
    /// The last model call's §4.3 disposition (row 4's input).
    pub last_call_disposition: Option<String>,
    /// A valid reward exists (the verify leg's output).
    pub reward: Option<bool>,
    /// The harness survived an upstream 429/5xx (row 8 — covariate).
    pub upstream_failure_survived: bool,
    /// Malformed model output observed (row 9 — task-level).
    pub malformed_output: bool,
    /// Replay divergence 409 (row 10 — smoke/conformance finding only).
    pub replay_diverged: bool,
    /// Denied-egress hosts not covered by `expected_denied` (row 12).
    pub undeclared_denied: Vec<String>,
    /// Version or install tree changed post-agent (row 14).
    pub self_modified: bool,
    /// A client abandoned a queued/buffered call (row 15).
    pub client_gone: bool,
}

/// What the table's "Retried?" column means for the mapped row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// An engine replan within the declared bound.
    EngineReplan,
    /// Never retried — the slot outcome stands.
    No,
    /// A fixed recipe is a new admission, never a retry.
    NewAdmission,
    /// Never appears in a paid run (smoke/conformance finding only).
    NeverPaid,
}

/// The classifier output — the mapped §5.8 disposition plus the flags
/// and covariates the accompanying rows contribute.
#[derive(Debug, Clone, PartialEq)]
pub struct HostingOutcome {
    /// The primary disposition spelling — `Disposition` for slot rows;
    /// `n/a{install:<step>}` and finding spellings for the
    /// admission/replay rows ride `label`.
    pub disposition: Option<Disposition>,
    /// The verbatim outcome spelling for non-slot rows
    /// (`n/a{install:<step>}` | `smoke_finding{replay_diverged}`).
    pub label: Option<String>,
    /// The "Retried?" column.
    pub retry: Retry,
    /// Slot flags the accompanying rows raise.
    pub flags: Vec<SlotFlag>,
    /// Recorded covariates (`infra events`, `client_gone` charge).
    pub covariates: Vec<String>,
}

fn out(
    disposition: Option<Disposition>,
    retry: Retry,
    flags: Vec<SlotFlag>,
    covariates: Vec<String>,
) -> HostingOutcome {
    HostingOutcome {
        disposition,
        label: None,
        retry,
        flags,
        covariates,
    }
}

fn inconclusive(cause: InfraCause) -> HostingOutcome {
    out(
        Some(Disposition::Inconclusive {
            cause: InconclusiveCause::Infra(cause),
        }),
        Retry::EngineReplan,
        vec![],
        vec![],
    )
}

/// `classify_hosting(facts)` — the §5.8 table, one pure function over
/// recorded facts. Admission-side events (build, ST2) precede cell
/// events; the never-scored legs (premature refusal, unreceipted call)
/// take precedence over a valid reward (RD-17's rule generalized by
/// REQ-XC-207); the agent-end rows follow; flag/covariate rows ride
/// the scored outcome.
pub fn classify_hosting(facts: &HostingFacts) -> HostingOutcome {
    // Row 1 — a build failure is admission-side; a fixed recipe is a
    // new admission, never a slot.
    if let Some(step) = facts.build_failure {
        let mut o = out(None, Retry::NewAdmission, vec![], vec![]);
        o.label = Some(step.na());
        return o;
    }
    // Row 2 — ST2 verify mismatch halts the arm (systematic).
    if facts.st2_mismatch {
        return out(
            Some(Disposition::Withheld {
                reason: WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch),
            }),
            Retry::No,
            vec![],
            vec![],
        );
    }
    // Row 3 — ST2 probe crash with a healthy cell replans.
    if let Some(c) = &facts.st2_probe_crash {
        return inconclusive(InfraCause::Named(format!("harness_setup:{c}")));
    }
    // Row 13 — relay or cell death is infrastructure.
    if facts.environment_died {
        return inconclusive(InfraCause::EnvironmentDied);
    }
    // Never-scored precedence legs — the budget machinery or the
    // receipt record, not the harness, ended the trial.
    if facts.premature_refusal {
        return inconclusive(InfraCause::BudgetReservation);
    }
    if facts.unreceipted_call {
        return inconclusive(InfraCause::UnreceiptedCall);
    }
    // Row 10 — a replay divergence is a smoke/conformance finding only.
    if facts.replay_diverged {
        let mut o = out(None, Retry::NeverPaid, vec![], vec![]);
        o.label = Some("smoke_finding{replay_diverged}".to_string());
        return o;
    }
    // The flag/covariate legs ride whichever scored outcome follows.
    let mut flags = Vec::new();
    let mut covariates = Vec::new();
    if !facts.undeclared_denied.is_empty() {
        flags.push(SlotFlag::UndeclaredEgressDenied);
        covariates.push("admission re-review".to_string());
    }
    if facts.self_modified {
        flags.push(SlotFlag::HarnessSelfModified);
        covariates.push("arm review".to_string());
    }
    if facts.upstream_failure_survived {
        covariates.push("infra_event:upstream".to_string());
    }
    if facts.malformed_output {
        covariates.push("malformed_model_output".to_string());
    }
    if facts.client_gone {
        covariates.push("released:client_gone charged".to_string());
    }
    let resolved = facts.reward.unwrap_or(false);
    // Row 6 — hang past deadline verifies and scores.
    if facts.agent_timed_out {
        return out(
            Some(Disposition::Executed {
                resolved,
                end_cause: EndCause::AgentTimeout,
            }),
            Retry::No,
            flags,
            covariates,
        );
    }
    // Row 7 — a non-premature slot budget refusal verifies and scores.
    if facts.budget_refused {
        return out(
            Some(Disposition::Executed {
                resolved,
                end_cause: EndCause::BudgetRefused,
            }),
            Retry::No,
            flags,
            covariates,
        );
    }
    // Rows 4–5 — a non-zero exit after a model-route failure is
    // infrastructure; any other non-zero exit verifies and scores.
    if let Some(rc) = facts.agent_exit {
        if rc != 0 {
            if let Some(d) = &facts.last_call_disposition {
                if MODEL_ROUTE_FAILURES.contains(&d.as_str()) {
                    return inconclusive(InfraCause::ModelRoute(d.clone()));
                }
            }
        }
        return out(
            Some(Disposition::Executed {
                resolved,
                end_cause: EndCause::Exited(rc),
            }),
            Retry::No,
            flags,
            covariates,
        );
    }
    // Rows 8–9/11–12/14–15 ride a scored slot with a valid reward.
    out(
        Some(Disposition::Executed {
            resolved,
            end_cause: EndCause::Exited(0),
        }),
        Retry::No,
        flags,
        covariates,
    )
}

/// `facts_from_records(slot, native, receipts, mediator, st2, st4,
/// expected)` — derive [`HostingFacts`] records-in from a fixture slot
/// bundle (AC-10's recompute leg): the stored disposition must equal
/// what [`classify_hosting`] produces from the same records.
pub fn facts_from_records(
    slot: &ExperimentSlot,
    native_calls: &[NativeCall],
    receipts: &[Json],
    mediator_rows: &[Json],
    expected_denied: &[String],
) -> HostingFacts {
    let mut f = HostingFacts {
        unreceipted_call: !crate::capture::reconcile_receipts(native_calls, receipts).is_empty(),
        last_call_disposition: receipts
            .last()
            .and_then(|r| r.get("disposition"))
            .and_then(Json::as_str)
            .map(str::to_string),
        ..Default::default()
    };
    if let Some(v) = &slot.verdict {
        f.reward = Some(v.reward_ppm.map(|p| p > 0).unwrap_or(false));
        let fs: Vec<String> = v.flags.iter().map(|x| x.as_str().to_string()).collect();
        f.self_modified = fs.contains(&"harness_self_modified".to_string());
    }
    f.undeclared_denied = mediator_rows
        .iter()
        .filter(|r| !row_granted(r))
        .filter_map(|r| r.get("host").and_then(Json::as_str))
        .filter(|h| !host_covered(expected_denied, h))
        .map(str::to_string)
        .collect();
    if let Some(p) = slot.phases.iter().find(|p| p.phase == "agent") {
        match EndCause::parse(&p.end_cause) {
            Some(EndCause::Exited(rc)) => f.agent_exit = Some(rc),
            Some(EndCause::AgentTimeout) => f.agent_timed_out = true,
            Some(EndCause::BudgetRefused) => f.budget_refused = true,
            Some(EndCause::Crashed) => f.agent_exit = Some(-1),
            None => {}
        }
    }
    if let Disposition::Inconclusive { cause } = &slot.disposition {
        if let InconclusiveCause::Infra(InfraCause::BudgetReservation) = cause {
            f.premature_refusal = true;
        }
        if let InconclusiveCause::Infra(InfraCause::EnvironmentDied) = cause {
            f.environment_died = true;
        }
    }
    if let Disposition::Withheld {
        reason: WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch),
    } = &slot.disposition
    {
        f.st2_mismatch = true;
    }
    f
}
