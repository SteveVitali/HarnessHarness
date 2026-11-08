//! `l1_obs` — the L1 observation plane (manifest 282/R6.28, K2;
//! spec §7.4 REQ-XD-103; design/36 §1.2, Part 7 K2).
//!
//! L1.1 feeds H the original's **raw** observations — the
//! `(output, returncode, exception_info)` triples — never O's rendered
//! tool messages, because H must render observations with its own
//! template and the rendering is part of what L1 tests (E1 G8).
//!
//! - [`RawObservation`] — the raw triple.
//! - [`render_mini_observation`] — the verbatim `mini.yaml`
//!   observation template (`mini.yaml:112–128`, research/28 A.5):
//!   a Jinja JSON object; under 10 000 **characters** the full
//!   `output`; at/above it `output_head`/`output_tail` (5 000 chars
//!   each) + `elided_chars` + `"Output too long."`. Strings render
//!   with `tojson` (Python `json.dumps` ensure-ascii + Jinja's
//!   HTML-safe `<>&'` escapes). REQ-XE-007's shape — the forward
//!   renderer exists so the inverse is golden-tested against it.
//! - [`invert_mini_observation`] — the **declared fallback inverse**
//!   (design/36 §1.2): below 10 000 the `output`/`returncode`/
//!   `exception_info` members recover exactly; at/above, `output_head`,
//!   `output_tail` and `elided_chars` determine everything the
//!   template reads, so the inverse synthesizes head +
//!   [`INVERT_FILLER`] + tail at the recorded length — the interior is
//!   unobservable by the template and re-renders byte-identically.
//! - [`ObsReplay`] — the recorded-observation sequence server behind
//!   the [`L1Exec`] seam: one raw per action, a typed
//!   `observation_overrun` refusal on a sequence overrun.
//! - [`ObsReplayExecutor`] — the same double bound onto the HH exec
//!   seam (`hh_env::executor::ToolExecutor`, l.565 — the contract's
//!   named seam): `execute` serves the next recorded triple, overrun
//!   refuses typed.
//! - [`raws_from_messages`] / [`raws_from_recording`] — the corpus
//!   half: recover the raw sequence from O's recorded requests —
//!   `extra.raw` members when present, else the inverse (the
//!   UNVERIFIED `extra` leg of design/36 §1.2 resolves to the tested
//!   fallback, as the contract directs).

use std::collections::{BTreeSet, VecDeque};

use hharness_wire::json::{parse, Json};

use crate::json_ext::JsonExt;

/// The L1 layer's typed error — `rule` names the refusing leg.
#[derive(Debug, Clone, PartialEq)]
pub struct L1Error {
    /// The refusing rule (`observation_overrun` |
    /// `observation_inverse` | `sequence_exhausted` |
    /// `replay_diverged` | `coverage` | `transport` | `drive` |
    /// `record_shape`).
    pub rule: &'static str,
    /// The verbatim detail.
    pub detail: String,
}

pub(crate) fn err(rule: &'static str, detail: impl Into<String>) -> L1Error {
    L1Error {
        rule,
        detail: detail.into(),
    }
}

impl std::fmt::Display for L1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "l1:{}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for L1Error {}

/// The template's character threshold (Python `str` length — never
/// bytes; E1 A.5).
pub const OBS_THRESHOLD_CHARS: usize = 10_000;
/// The head/tail slice length at/above the threshold.
pub const OBS_SLICE_CHARS: usize = 5_000;
/// The declared interior filler the inverse synthesizes at/above the
/// threshold (design/36 §1.2 — the interior is unobservable by the
/// template; one declared char, never the recorded interior).
pub const INVERT_FILLER: char = 'x';
/// The submit marker (E1 A.4 `environments/local.py` `_check_finished`).
pub const SUBMIT_MARKER: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

/// The raw observation — O's `(output, returncode, exception_info)`
/// per executed action (design/36 §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct RawObservation {
    /// The command's merged stdout+stderr (UTF-8 after
    /// `errors="replace"` — the recorded doc carries the replaced
    /// text, never raw bytes).
    pub output: String,
    /// The exit code (`-1` on timeout — E1 A.4).
    pub returncode: i64,
    /// The exception detail when the environment reported one.
    pub exception_info: Option<Json>,
}

/// `jinja_truthy(j)` — Python truthiness for the
/// `{%- if output.exception_info %}` guard.
fn jinja_truthy(j: &Json) -> bool {
    match j {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Int(i) => *i != 0,
        Json::Str(s) => !s.is_empty(),
        Json::Arr(a) => !a.is_empty(),
        Json::Obj(m) => !m.is_empty(),
    }
}

/// `dumps(j)` — Python `json.dumps` (default separators `, `/`: `,
/// sort-keys as Jinja's `tojson` policy fixes) then the HTML-safe
/// `<`/`>`/`&`/`'` escapes — one `tojson` spelling (CC1: the string
/// escaper is [`crate::render::py_json_str`]'s, never a second).
pub fn py_tojson(j: &Json) -> String {
    fn dumps(j: &Json) -> String {
        match j {
            Json::Null => "null".to_string(),
            Json::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Json::Int(i) => i.to_string(),
            Json::Str(s) => crate::render::py_json_str(s),
            Json::Arr(a) => {
                let inner: Vec<String> = a.iter().map(dumps).collect();
                format!("[{}]", inner.join(", "))
            }
            Json::Obj(m) => {
                let inner: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", crate::render::py_json_str(k), dumps(v)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
    }
    dumps(j)
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\'', "\\u0027")
}

/// `render_mini_observation(raw)` — the verbatim `mini.yaml`
/// observation template (research/28 A.5, `mini.yaml:112–128`):
/// character-counted threshold and slices, `exception_info` only when
/// truthy.
pub fn render_mini_observation(raw: &RawObservation) -> String {
    let exc = match &raw.exception_info {
        Some(j) if jinja_truthy(j) => format!(", \"exception_info\": {}", py_tojson(j)),
        _ => String::new(),
    };
    let chars = raw.output.chars().count();
    if chars < OBS_THRESHOLD_CHARS {
        format!(
            "{{\n  \"returncode\": {},\n  \"output\": {}{exc}\n}}",
            raw.returncode,
            py_tojson(&Json::str(&raw.output))
        )
    } else {
        let head: String = raw.output.chars().take(OBS_SLICE_CHARS).collect();
        let tail: String = raw.output.chars().skip(chars - OBS_SLICE_CHARS).collect();
        format!(
            "{{\n  \"returncode\": {},\n  \"output_head\": {},\n  \"output_tail\": {},\n  \"elided_chars\": {},\n  \"warning\": \"Output too long.\"{exc}\n}}",
            raw.returncode,
            py_tojson(&Json::str(head)),
            py_tojson(&Json::str(tail)),
            chars - OBS_THRESHOLD_CHARS,
        )
    }
}

/// `invert_mini_observation(rendered)` — the declared, golden-tested
/// inverse of the mini.yaml observation template (design/36 §1.2's
/// fallback; the `extra`-member leg of `raws_from_messages` is tried
/// first at the corpus layer). Below the threshold the JSON members
/// recover exactly; at/above it the synthesized raw carries the
/// recorded head, [`INVERT_FILLER`]-padded interior and tail at the
/// recorded character length — re-rendering byte-identically.
pub fn invert_mini_observation(rendered: &str) -> Result<RawObservation, L1Error> {
    let doc = parse(rendered).map_err(|e| {
        err(
            "observation_inverse",
            format!("rendered observation is not JSON: {e}"),
        )
    })?;
    let m = doc.as_obj().ok_or_else(|| {
        err(
            "observation_inverse",
            "rendered observation must be an object",
        )
    })?;
    let returncode = m
        .get("returncode")
        .and_then(Json::as_int)
        .ok_or_else(|| err("observation_inverse", "returncode must be an int"))?;
    let exception_info = m.get("exception_info").cloned();
    if let Some(output) = m.get("output").and_then(Json::as_str) {
        return Ok(RawObservation {
            output: output.to_string(),
            returncode,
            exception_info,
        });
    }
    let head = m
        .get("output_head")
        .and_then(Json::as_str)
        .ok_or_else(|| err("observation_inverse", "output_head must be a string"))?;
    let tail = m
        .get("output_tail")
        .and_then(Json::as_str)
        .ok_or_else(|| err("observation_inverse", "output_tail must be a string"))?;
    let elided = m
        .get("elided_chars")
        .and_then(Json::as_int)
        .filter(|n| *n >= 0)
        .ok_or_else(|| err("observation_inverse", "elided_chars must be an int >= 0"))?;
    let mut output = String::with_capacity(head.len() + tail.len() + elided as usize);
    output.push_str(head);
    output.extend(std::iter::repeat_n(INVERT_FILLER, elided as usize));
    output.push_str(tail);
    Ok(RawObservation {
        output,
        returncode,
        exception_info,
    })
}

/// `L1Exec` — the exec seam the L1 legs bind (the cell's command exec
/// at fixture level): one command in, one raw observation out.
pub trait L1Exec {
    /// Execute `command`; return the raw observation or a typed
    /// refusal (an observation-sequence overrun for the replay leg).
    fn run(&mut self, command: &str) -> Result<RawObservation, L1Error>;
}

/// `ObsReplay` — the observation-replay executor's core: serves O's
/// recorded raw observations one per action (K2). A sequence overrun
/// is a typed `observation_overrun` refusal — never a fabricated
/// observation.
#[derive(Debug, Clone)]
pub struct ObsReplay {
    raws: VecDeque<RawObservation>,
    served: Vec<RawObservation>,
}

impl ObsReplay {
    /// `ObsReplay::new(raws)` — the recorded sequence to serve.
    pub fn new(raws: Vec<RawObservation>) -> ObsReplay {
        ObsReplay {
            raws: raws.into(),
            served: Vec::new(),
        }
    }

    /// The next recorded raw — `observation_overrun` past the end.
    pub fn next(&mut self) -> Result<RawObservation, L1Error> {
        let raw = self.raws.pop_front().ok_or_else(|| {
            err(
                "observation_overrun",
                "observation replay exhausted — a request beyond the recorded sequence is a fail, never a fabricated observation",
            )
        })?;
        self.served.push(raw.clone());
        Ok(raw)
    }

    /// How many recorded raws remain.
    pub fn remaining(&self) -> usize {
        self.raws.len()
    }

    /// The raws served so far, in order.
    pub fn served(&self) -> &[RawObservation] {
        &self.served
    }
}

impl L1Exec for ObsReplay {
    fn run(&mut self, _command: &str) -> Result<RawObservation, L1Error> {
        self.next()
    }
}

/// `ObsReplayExecutor` — the observation-replay double bound onto the
/// HH exec seam (`hh_env::executor::ToolExecutor`, `executor.rs:565`):
/// `execute` serves the next recorded `(output, returncode,
/// exception_info)`, emitting the output as one `stdout` chunk and the
/// exit status on the terminal report; `exception_info` rides
/// [`ObsReplayExecutor::last_observation`] (the report vocabulary has
/// no exception member — reported, never coerced). Overrun refuses
/// `EnvError::Unsupported{l1.observation_replay}` — typed, never a
/// fabricated result.
pub struct ObsReplayExecutor {
    replay: ObsReplay,
    decl: hharness_env::executor::ExecutorDeclaration,
    last: Option<RawObservation>,
}

impl ObsReplayExecutor {
    /// `ObsReplayExecutor::new(raws)` — the recorded sequence.
    pub fn new(raws: Vec<RawObservation>) -> ObsReplayExecutor {
        ObsReplayExecutor {
            replay: ObsReplay::new(raws),
            decl: hharness_env::executor::ExecutorDeclaration {
                executor_id: "l1.observation_replay/1".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: hharness_env::executor::DedupSupport::None,
                probe_support: hharness_env::executor::ProbeSupport::None,
                interrupt: hharness_env::executor::InterruptSupport::Unsupported,
                error_classes: ["executor_error".to_string()]
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
                streams: true,
                domains: [hharness_hir::kinds::EffectDomain::Exec]
                    .into_iter()
                    .collect(),
            },
            last: None,
        }
    }

    /// The raw triple the last `execute` served.
    pub fn last_observation(&self) -> Option<&RawObservation> {
        self.last.as_ref()
    }

    /// The underlying replay (remaining/served counts).
    pub fn replay(&self) -> &ObsReplay {
        &self.replay
    }
}

impl hharness_env::executor::ToolExecutor for ObsReplayExecutor {
    fn declaration(&self) -> &hharness_env::executor::ExecutorDeclaration {
        &self.decl
    }

    fn execute(
        &mut self,
        request: &hharness_env::executor::ExecutionRequest,
        sink: &mut dyn FnMut(hharness_env::executor::ExecutorSignal),
    ) -> Result<hharness_env::executor::TerminalReport, hharness_env::errors::EnvError> {
        let raw = self
            .replay
            .next()
            .map_err(|e| hharness_env::errors::EnvError::Unsupported {
                capability: "l1.observation_replay",
                detail: e.to_string(),
            })?;
        sink(hharness_env::executor::ExecutorSignal {
            token: request.attribution_token.clone(),
            kind: hharness_env::capture::CaptureKind::OutputChunk {
                stream: "stdout".to_string(),
                data: raw.output.clone(),
            },
        });
        let original_size = raw.output.len() as u64;
        self.last = Some(raw.clone());
        Ok(hharness_env::executor::TerminalReport {
            status: hharness_env::executor::TerminalStatus::Ok,
            exit_status: Some(raw.returncode),
            outcome_hint: "applied".to_string(),
            retryable_hint: None,
            detail_ref: None,
            truncated: false,
            omitted_bytes: 0,
            original_size,
        })
    }

    fn probe(
        &self,
        _effect_id: &str,
        _attempt_no: u64,
    ) -> Result<hharness_env::executor::ProbeVerdict, hharness_env::errors::EnvError> {
        Ok(hharness_env::executor::ProbeVerdict::Undeterminable)
    }
}

/// `raws_from_messages(messages)` — recover the raw observation
/// sequence from a request's message list. Each `role:"tool"` message
/// contributes one raw: its `extra.raw_observation
/// {output,returncode,exception_info}` members when present (the
/// native-trajectory leg, design/36 §1.2 — UNVERIFIED upstream, so the
/// member shape is declared here), else the tested template inverse
/// over the rendered `content`.
pub fn raws_from_messages(messages: &[Json]) -> Result<Vec<RawObservation>, L1Error> {
    let mut out = Vec::new();
    for msg in messages {
        let m = match msg.as_obj() {
            Some(m) => m,
            None => continue,
        };
        if m.get("role").and_then(Json::as_str) != Some("tool") {
            continue;
        }
        if let Some(raw) = m
            .get("extra")
            .and_then(JsonExt::as_obj)
            .and_then(|e| e.get("raw_observation"))
            .and_then(JsonExt::as_obj)
        {
            let output = raw
                .get("output")
                .and_then(Json::as_str)
                .ok_or_else(|| err("observation_inverse", "extra.raw_observation.output"))?;
            let returncode = raw
                .get("returncode")
                .and_then(Json::as_int)
                .ok_or_else(|| err("observation_inverse", "extra.raw_observation.returncode"))?;
            out.push(RawObservation {
                output: output.to_string(),
                returncode,
                exception_info: raw.get("exception_info").cloned(),
            });
            continue;
        }
        let content = m
            .get("content")
            .and_then(Json::as_str)
            .ok_or_else(|| err("observation_inverse", "tool message carries no content"))?;
        out.push(invert_mini_observation(content)?);
    }
    Ok(out)
}

/// `raws_from_recording(rec)` — the recorded sequence's raw
/// observations: the longest `role:"tool"` list across the recorded
/// downstream requests (each later request carries every earlier
/// observation), inverted per step. An empty tool list yields an
/// empty sequence (a submit-at-once trajectory records no
/// observation).
pub fn raws_from_recording(
    rec: &hharness_gateway::recording::Recording,
) -> Result<Vec<RawObservation>, L1Error> {
    let mut best: Vec<RawObservation> = Vec::new();
    for call in &rec.calls {
        let body = String::from_utf8_lossy(&call.downstream_body);
        let doc = match parse(&body) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let msgs = doc
            .get("messages")
            .and_then(JsonExt::as_arr)
            .cloned()
            .unwrap_or_default();
        let raws = raws_from_messages(&msgs)?;
        if raws.len() > best.len() {
            best = raws;
        }
    }
    Ok(best)
}
