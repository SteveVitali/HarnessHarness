//! `l1_corpus` — the L1 synthetic corpus and the turn-1 census
//! generator (manifest 282/R6.28, K3; spec §7.4 REQ-XD-107, REQ-XD-109;
//! design/36 §1.4, Part 7 K3).
//!
//! - [`BM_BRANCHES`] — the 19-branch coverage list of spec §7.4
//!   (BM1–BM19; the spec governs over design's M-numbering — contract
//!   Notes). A missing branch makes the gate **refuse**
//!   (REQ-XD-109/107) — [`check_corpus`].
//! - [`bm_fixtures`] — the synthetic branch fixtures: authored
//!   `hh.model_proxy.recording/1` documents with `provenance:
//!   synthetic` (P6), one per branch, deterministic — the committed
//!   corpus pins them byte-for-byte.
//! - [`census_entries`] — the turn-1 census **generator** over a task
//!   manifest (generator only — the census *executes* in W1 (a), spec
//!   §10.1.4): one-response synthetic sequences that submit at once
//!   (mini: one `bash` call `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`),
//!   carrying the task's instance bytes so the corpus covers every
//!   instruction-dependent byte L2 will see.
//! - [`check_corpus`] — the `hh-l1-corpus/1` coverage map: every BM
//!   branch covered by a fixture entry and every manifest task covered
//!   by a census entry, else a typed `coverage` refusal.

use hharness_gateway::recording::{RecordedCall, RecordedResponse, Recording};
use hharness_wire::json::Json;

use crate::json_ext::JsonExt;
use crate::l1_obs::{err, render_mini_observation, L1Error, RawObservation, SUBMIT_MARKER};

/// The corpus manifest schema spelling.
pub const CORPUS_SCHEMA: &str = "hh-l1-corpus/1";

/// The 19-branch coverage list of spec §7.4 (`BM1`–`BM19`; the spec
/// governs the naming — contract Notes).
pub const BM_BRANCHES: [(&str, &str); 19] = [
    ("BM1", "single tool call, observation < 10 000 characters"),
    (
        "BM2",
        "observation of 9 999/10 000/10 001 characters incl. multi-byte at 5 000",
    ),
    ("BM3", "several tool calls in one response"),
    ("BM4", "no tool call"),
    ("BM5", "malformed arguments JSON"),
    ("BM6", "unknown tool"),
    ("BM7", "missing `command`"),
    (
        "BM8",
        "finish_reason: length with no tool call (truncation branch)",
    ),
    (
        "BM9",
        "three consecutive format errors → RepeatedFormatError",
    ),
    ("BM10", "format error, then a clean step (counter reset)"),
    (
        "BM11",
        "command exceeding timeout (partial output, rc -1, exception_info)",
    ),
    ("BM12", "marker on first line with rc 0 → Submitted"),
    (
        "BM13",
        "marker not on first line / marker with rc != 0 → no submit",
    ),
    ("BM14", "submit mid-batch → padded action not executed"),
    ("BM15", "non-UTF-8 output (errors=\"replace\")"),
    ("BM16", "provider context-window error → abort"),
    (
        "BM17",
        "429/502 then success → transport retry, identical bytes, equal retry count",
    ),
    (
        "BM18",
        "assistant prose plus tool call and reasoning_content present",
    ),
    (
        "BM19",
        "env-exposing commands (env|sort, pwd, hostname) — L1.2 divergence surface",
    ),
];

/// One authored branch fixture: its id, the branches it covers, and
/// the `hh.model_proxy.recording/1` sequence (`provenance: synthetic`).
pub struct BmFixture {
    /// The fixture id (`bm01`…`bm19`).
    pub id: &'static str,
    /// The BM branches this fixture covers.
    pub branches: &'static [&'static str],
    /// The authored sequence.
    pub recording: Recording,
}

fn resp_body(message: Json, finish: &str) -> Vec<u8> {
    Json::obj([
        (
            "choices",
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("message", message),
                ("finish_reason", Json::str(finish)),
            ])]),
        ),
        ("id", Json::str("chatcmpl-syn")),
        ("created", Json::Int(1)),
        ("model", Json::str("glm-5.3-flash")),
        ("object", Json::str("chat.completion")),
    ])
    .to_canonical_string()
    .into_bytes()
}

fn assistant(tool_calls: Json, content: &str) -> Json {
    let mut m = vec![
        ("role", Json::str("assistant")),
        ("content", Json::str(content)),
    ];
    if !matches!(tool_calls, Json::Null) {
        m.push(("tool_calls", tool_calls));
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn tc(id: &str, command: &str) -> Json {
    Json::obj([
        ("id", Json::str(id)),
        ("type", Json::str("function")),
        (
            "function",
            Json::obj([
                ("name", Json::str("bash")),
                (
                    "arguments",
                    Json::str(format!("{{\"command\":{}}}", {
                        let j = Json::str(command);
                        j.to_canonical_string()
                    })),
                ),
            ]),
        ),
    ])
}

fn tc_raw(id: &str, name: &str, arguments: &str) -> Json {
    Json::obj([
        ("id", Json::str(id)),
        ("type", Json::str("function")),
        (
            "function",
            Json::obj([
                ("name", Json::str(name)),
                ("arguments", Json::str(arguments)),
            ]),
        ),
    ])
}

fn request_body(obs_msgs: Vec<Json>) -> Vec<u8> {
    let mut messages = vec![
        Json::obj([
            ("role", Json::str("system")),
            ("content", Json::str("You are mini-swe-agent.")),
        ]),
        Json::obj([
            ("role", Json::str("user")),
            ("content", Json::str("Solve the task instance.")),
        ]),
    ];
    messages.extend(obs_msgs);
    Json::obj([
        ("model", Json::str("glm-5.3-flash")),
        ("messages", Json::Arr(messages)),
        ("tools", Json::Arr(vec![crate::l1_drive::bash_tool_pub()])),
    ])
    .to_canonical_string()
    .into_bytes()
}

fn obs_msg(id: &str, raw: &RawObservation) -> Json {
    Json::obj([
        ("role", Json::str("tool")),
        ("tool_call_id", Json::str(id)),
        ("content", Json::str(render_mini_observation(raw))),
    ])
}

fn call(k: u64, downstream: Vec<u8>, status: u16, resp: &[u8]) -> RecordedCall {
    RecordedCall {
        k,
        downstream_method: "POST".to_string(),
        downstream_path: crate::l1_drive::CHAT_ROUTE_PUB.to_string(),
        downstream_body: downstream,
        upstream_request: None,
        upstream_response: Some(RecordedResponse {
            status,
            headers: Default::default(),
            body: resp.to_vec(),
            chunk_stamps: Vec::new(),
        }),
        failure: None,
        released: None,
        disposition: "forwarded".to_string(),
        latency_ms: 0,
    }
}

fn recording(slot: &str, calls: Vec<RecordedCall>) -> Recording {
    Recording {
        slot_id: slot.to_string(),
        provenance: "synthetic".to_string(),
        calls,
    }
}

/// `bm_fixtures()` — the 19 authored branch sequences, deterministic.
pub fn bm_fixtures() -> Vec<BmFixture> {
    let ok = "stop";
    let plain = |cmd: &str| assistant(Json::Arr(vec![tc("call_0", cmd)]), "");
    let empty = vec![];
    let mut out: Vec<BmFixture> = Vec::new();
    // BM1 — single tool call, short observation, then submit.
    out.push(BmFixture {
        id: "bm01",
        branches: &["BM1"],
        recording: recording(
            "bm01",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain("echo hi"), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg(
                        "call_0",
                        &RawObservation {
                            output: "hi".to_string(),
                            returncode: 0,
                            exception_info: None,
                        },
                    )]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM2 — a response whose follow-up carries a ≥10 000-char
    // observation with multi-byte characters straddling the 5 000-char
    // boundary (the inverse's head/tail leg).
    let mut big: String = "é".repeat(4_999);
    big.push_str("a");
    big.push_str(&"".repeat(5_002));
    let big_raw = RawObservation {
        output: big,
        returncode: 0,
        exception_info: None,
    };
    out.push(BmFixture {
        id: "bm02",
        branches: &["BM2"],
        recording: recording(
            "bm02",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain("seq"), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg("call_0", &big_raw)]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM3 — several tool calls in one response.
    out.push(BmFixture {
        id: "bm03",
        branches: &["BM3"],
        recording: recording(
            "bm03",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    assistant(
                        Json::Arr(vec![tc("call_0", "echo a"), tc("call_1", "echo b")]),
                        "",
                    ),
                    ok,
                ),
            )],
        ),
    });
    // BM4 — no tool call.
    out.push(BmFixture {
        id: "bm04",
        branches: &["BM4"],
        recording: recording(
            "bm04",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(assistant(Json::Null, "no call here"), ok),
            )],
        ),
    });
    // BM5 — malformed arguments JSON.
    out.push(BmFixture {
        id: "bm05",
        branches: &["BM5"],
        recording: recording(
            "bm05",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    assistant(Json::Arr(vec![tc_raw("call_0", "bash", "{not json")]), ""),
                    ok,
                ),
            )],
        ),
    });
    // BM6 — unknown tool.
    out.push(BmFixture {
        id: "bm06",
        branches: &["BM6"],
        recording: recording(
            "bm06",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    assistant(
                        Json::Arr(vec![tc_raw("call_0", "not_a_tool", "{\"command\":\"x\"}")]),
                        "",
                    ),
                    ok,
                ),
            )],
        ),
    });
    // BM7 — missing `command`.
    out.push(BmFixture {
        id: "bm07",
        branches: &["BM7"],
        recording: recording(
            "bm07",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    assistant(
                        Json::Arr(vec![tc_raw("call_0", "bash", "{\"cmd\":\"x\"}")]),
                        "",
                    ),
                    ok,
                ),
            )],
        ),
    });
    // BM8 — finish_reason: length, no tool call.
    out.push(BmFixture {
        id: "bm08",
        branches: &["BM8"],
        recording: recording(
            "bm08",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(assistant(Json::Null, "cut off mid"), "length"),
            )],
        ),
    });
    // BM9 — three consecutive format errors.
    out.push(BmFixture {
        id: "bm09",
        branches: &["BM9"],
        recording: recording(
            "bm09",
            (0..3)
                .map(|k| {
                    call(
                        k,
                        request_body(empty.clone()),
                        200,
                        &resp_body(assistant(Json::Null, "still wrong"), ok),
                    )
                })
                .collect(),
        ),
    });
    // BM10 — format error, then a clean step.
    out.push(BmFixture {
        id: "bm10",
        branches: &["BM10"],
        recording: recording(
            "bm10",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(assistant(Json::Null, "wrong"), ok),
                ),
                call(
                    1,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM11 — the follow-up carries a timeout observation (rc -1 +
    // exception_info).
    let to_raw = RawObservation {
        output: "partial".to_string(),
        returncode: -1,
        exception_info: Some(Json::str(
            "An error occurred while executing the command: Command 'sleep 99' timed out",
        )),
    };
    out.push(BmFixture {
        id: "bm11",
        branches: &["BM11"],
        recording: recording(
            "bm11",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain("sleep 99"), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg("call_0", &to_raw)]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM12 — marker on the first line with rc 0 → Submitted.
    out.push(BmFixture {
        id: "bm12",
        branches: &["BM12"],
        recording: recording(
            "bm12",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
            )],
        ),
    });
    // BM13 — marker NOT on the first line → no submit.
    out.push(BmFixture {
        id: "bm13",
        branches: &["BM13"],
        recording: recording(
            "bm13",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain(&format!("echo x && echo {SUBMIT_MARKER}")), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg(
                        "call_0",
                        &RawObservation {
                            output: format!("x\n{SUBMIT_MARKER}"),
                            returncode: 0,
                            exception_info: None,
                        },
                    )]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM14 — submit mid-batch: first action submits, second is padded.
    out.push(BmFixture {
        id: "bm14",
        branches: &["BM14"],
        recording: recording(
            "bm14",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    assistant(
                        Json::Arr(vec![
                            tc("call_0", &format!("echo {SUBMIT_MARKER}")),
                            tc("call_1", "echo padded"),
                        ]),
                        "",
                    ),
                    ok,
                ),
            )],
        ),
    });
    // BM15 — non-UTF-8 output rides the follow-up observation (the
    // errors="replace" replacement char in the rendered content).
    out.push(BmFixture {
        id: "bm15",
        branches: &["BM15"],
        recording: recording(
            "bm15",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain("cat /bin/sh | head -c 4"), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg(
                        "call_0",
                        &RawObservation {
                            output: "\u{FFFD}ELF".to_string(),
                            returncode: 0,
                            exception_info: None,
                        },
                    )]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM16 — provider context-window error → abort (a refused call:
    // no upstream response, the recorded failure rides `failure`).
    out.push(BmFixture {
        id: "bm16",
        branches: &["BM16"],
        recording: recording(
            "bm16",
            vec![{
                let mut c = call(0, request_body(empty.clone()), 400, b"{}");
                c.upstream_response = None;
                c.released = None;
                c.disposition = "upstream_error".to_string();
                c
            }],
        ),
    });
    // BM17 — 429 then success: identical bytes, one retry.
    out.push(BmFixture {
        id: "bm17",
        branches: &["BM17"],
        recording: recording(
            "bm17",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    429,
                    &resp_body(Json::Null, "stop"),
                ),
                call(
                    1,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    // BM18 — prose + tool call with reasoning_content present.
    out.push(BmFixture {
        id: "bm18",
        branches: &["BM18"],
        recording: recording(
            "bm18",
            vec![call(
                0,
                request_body(empty.clone()),
                200,
                &resp_body(
                    {
                        let mut m = assistant(Json::Arr(vec![tc("call_0", "echo hi")]), "let me");
                        if let Json::Obj(o) = &mut m {
                            o.insert(
                                "reasoning_content".to_string(),
                                Json::str("short reasoning"),
                            );
                        }
                        m
                    },
                    ok,
                ),
            )],
        ),
    });
    // BM19 — env-exposing commands.
    out.push(BmFixture {
        id: "bm19",
        branches: &["BM19"],
        recording: recording(
            "bm19",
            vec![
                call(
                    0,
                    request_body(empty.clone()),
                    200,
                    &resp_body(plain("env | sort; pwd; hostname"), ok),
                ),
                call(
                    1,
                    request_body(vec![obs_msg(
                        "call_0",
                        &RawObservation {
                            output: "cell-facts".to_string(),
                            returncode: 0,
                            exception_info: None,
                        },
                    )]),
                    200,
                    &resp_body(plain(&format!("echo {SUBMIT_MARKER}")), ok),
                ),
            ],
        ),
    });
    out
}

/// `census_entries(manifest)` — the turn-1 census generator (spec
/// §7.4 corpus leg 3; REQ-XD-107): one synthetic sequence per
/// manifest task `{id, instruction}`, one response that submits at
/// once (`echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`), the request
/// carrying the task's instruction bytes verbatim (the
/// instruction-dependent bytes the census exists to cover).
/// `manifest` is `{tasks: [{task_id, instruction}]}` — the declared
/// projection the first-pass task manifests are read through.
pub fn census_entries(manifest: &Json) -> Result<Vec<Json>, L1Error> {
    let tasks = manifest
        .get("tasks")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| err("coverage", "manifest.tasks must be an array"))?;
    let mut out = Vec::new();
    for t in tasks {
        let id = t
            .get("task_id")
            .or_else(|| t.get("id"))
            .and_then(Json::as_str)
            .ok_or_else(|| err("coverage", "manifest task carries no task_id"))?;
        let instruction = t
            .get("instruction")
            .and_then(Json::as_str)
            .ok_or_else(|| err("coverage", format!("task {id} carries no instruction")))?;
        let req = Json::obj([
            ("model", Json::str("glm-5.3-flash")),
            (
                "messages",
                Json::Arr(vec![
                    Json::obj([
                        ("role", Json::str("system")),
                        ("content", Json::str("You are mini-swe-agent.")),
                    ]),
                    Json::obj([
                        ("role", Json::str("user")),
                        ("content", Json::str(instruction)),
                    ]),
                ]),
            ),
            ("tools", Json::Arr(vec![crate::l1_drive::bash_tool_pub()])),
        ])
        .to_canonical_string()
        .into_bytes();
        let rec = recording(
            &format!("census:{id}"),
            vec![call(
                0,
                req,
                200,
                &resp_body(
                    assistant(
                        Json::Arr(vec![tc("call_0", &format!("echo {SUBMIT_MARKER}"))]),
                        "",
                    ),
                    "stop",
                ),
            )],
        )
        .to_json();
        out.push(Json::obj([("task_id", Json::str(id)), ("sequence", rec)]));
    }
    Ok(out)
}

/// `corpus_manifest(entries, census_tasks)` — build the
/// `hh-l1-corpus/1` document the coverage check and the conformance
/// record read.
pub fn corpus_manifest(entries: Vec<(&str, &[&str])>, census_tasks: Vec<String>) -> Json {
    Json::obj([
        ("schema", Json::str(CORPUS_SCHEMA)),
        (
            "entries",
            Json::Arr(
                entries
                    .into_iter()
                    .map(|(id, branches)| {
                        Json::obj([
                            ("id", Json::str(id)),
                            (
                                "branches",
                                Json::Arr(branches.iter().map(|b| Json::str(*b)).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "census_tasks",
            Json::Arr(census_tasks.into_iter().map(Json::str).collect()),
        ),
    ])
}

/// `check_corpus(corpus, manifest_tasks)` — the REQ-XD-109/107
/// coverage refusal: every `BM_BRANCHES` id covered by some entry's
/// declared `branches`, every manifest task covered by a census
/// entry. Returns the coverage map the conformance record carries.
pub fn check_corpus(corpus: &Json, manifest_tasks: &[String]) -> Result<Json, L1Error> {
    if corpus.get("schema").and_then(Json::as_str) != Some(CORPUS_SCHEMA) {
        return Err(err("coverage", format!("schema must be {CORPUS_SCHEMA}")));
    }
    let entries = corpus
        .get("entries")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| err("coverage", "entries must be an array"))?;
    let mut map: Vec<(&'static str, String)> = Vec::new();
    for (bm, _) in BM_BRANCHES {
        let hit = entries.iter().find_map(|e| {
            let branches = e.get("branches").and_then(JsonExt::as_arr)?;
            if branches.iter().any(|b| b.as_str() == Some(bm)) {
                e.get("id").and_then(Json::as_str).map(str::to_string)
            } else {
                None
            }
        });
        match hit {
            Some(id) => map.push((bm, id)),
            None => {
                return Err(err(
                    "coverage",
                    format!("missing branch {bm} — a missing BM branch refuses the gate"),
                ))
            }
        }
    }
    let census: Vec<&str> = corpus
        .get("census_tasks")
        .and_then(JsonExt::as_arr)
        .map(|a| a.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    for task in manifest_tasks {
        if !census.contains(&task.as_str()) {
            return Err(err(
                "coverage",
                format!("missing census entry for task {task} — refuses the gate"),
            ));
        }
    }
    Ok(Json::obj(
        map.into_iter().map(|(bm, id)| (bm, Json::str(id))),
    ))
}
