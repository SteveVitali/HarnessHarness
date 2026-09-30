//! `hh-bench-adapter` — the out-of-process benchmark-adapter binary
//! (R-2.9.4⁰ᵇ; spec §5h.4 §2; S3.3).
//!
//! The transport is canonical JSON: a request `{"schema":"adapter_request/1",
//! "adapter":"<id>","op":"<op>", …}` on stdin, a response object (or a typed
//! `adapter_error/1` envelope) on stdout. Instrument-plane ops
//! (`collect_submission`, `grade`) are refused when the request carries
//! `"caller":"participant"` — the participant never reaches them.
//!
//! Ops: `declare | discover | import_task | materialize | expose |
//! collect_submission | grade | export | parity`.

use std::io::Read;

use hh_bench::adapter::{AdapterError, AdapterOp, BenchmarkAdapter};
use hh_bench::adapters;
use hh_bench::grade::GradeRequest;
use hh_bench::records::{EnvironmentHandle, Surface};
use hh_ontology::lab::{BenchmarkNetworkMode, EnvironmentFamily, VerifierIsolation};
use hh_wire::Json;

fn main() {
    let mut buf = String::new();
    let code = match std::io::stdin().read_to_string(&mut buf) {
        Ok(_) => run(&buf),
        Err(e) => fail("MalformedRequest", &format!("stdin: {e}")),
    };
    std::process::exit(code);
}

fn run(input: &str) -> i32 {
    let req = match hh_wire::parse(input) {
        Ok(j) => j,
        Err(e) => return fail("MalformedRequest", &format!("parse: {e}")),
    };
    let adapter_id = match req.get("adapter").and_then(Json::as_str) {
        Some(a) => a,
        None => return fail("MalformedRequest", "missing `adapter`"),
    };
    let op = match req
        .get("op")
        .and_then(Json::as_str)
        .and_then(AdapterOp::parse)
    {
        Some(o) => o,
        None => return fail("MalformedRequest", "unknown `op`"),
    };
    let participant = req.get("caller").and_then(Json::as_str) == Some("participant");
    if participant && op.instrument_plane() {
        return fail(
            "InstrumentOpFromParticipant",
            &format!("participant invoked {}", op.as_str()),
        );
    }
    let adapter = match adapters::FixtureAdapter::by_id(adapter_id) {
        Some(a) => a,
        None => return fail("UnknownOp", &format!("no adapter `{adapter_id}`")),
    };
    dispatch(&adapter, op, &req)
}

fn env_from_json(j: &Json) -> Result<EnvironmentHandle, String> {
    let get = |k: &str| -> Result<String, String> {
        j.get(k)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("handle missing `{k}`"))
    };
    Ok(EnvironmentHandle {
        handle_id: get("handle_id")?,
        task_id: get("task_id")?,
        family: EnvironmentFamily::parse(&get("family")?).ok_or("bad family")?,
        root: get("root")?,
        network_mode: BenchmarkNetworkMode::parse(&get("network_mode")?)
            .ok_or("bad network_mode")?,
        resolved_image_digest: j
            .get("resolved_image_digest")
            .and_then(Json::as_str)
            .map(str::to_string),
        surface: Surface::parse(&get("surface")?).ok_or("bad surface")?,
    })
}

fn dispatch(adapter: &impl BenchmarkAdapter, op: AdapterOp, req: &Json) -> i32 {
    let task_id = || req.get("task_id").and_then(Json::as_str);
    let out: Result<Json, AdapterError> = (|| match op {
        AdapterOp::Declare => {
            let unpinned = adapter.unpinned_tasks();
            Ok(Json::obj([
                ("adapter", Json::str(adapter.adapter_id())),
                ("family", Json::str(adapter.family().name())),
                ("out_of_process", Json::Bool(true)),
                // AC-R-2.9.4-6: tag-only imports land in `unpinned[]` and
                // cap the admissible `claimed_level` at R1/R3 (R2 refuses).
                (
                    "unpinned",
                    Json::Arr(unpinned.iter().map(Json::str).collect()),
                ),
                (
                    "claimed_levels",
                    Json::Arr(
                        adapter
                            .claimed_levels()
                            .into_iter()
                            .map(Json::str)
                            .collect(),
                    ),
                ),
            ]))
        }
        AdapterOp::Discover => Ok(Json::obj([(
            "task_ids",
            Json::Arr(adapter.task_ids().iter().map(Json::str).collect()),
        )])),
        AdapterOp::ImportTask => {
            let t = adapter.task(
                task_id()
                    .ok_or_else(|| AdapterError::MalformedRequest("missing `task_id`".into()))?,
            )?;
            Ok(Json::obj([("task", t.to_json())]))
        }
        AdapterOp::Materialize => {
            let root = req
                .get("root")
                .and_then(Json::as_str)
                .ok_or_else(|| AdapterError::MalformedRequest("missing `root`".into()))?;
            let verifier = matches!(req.get("verifier"), Some(Json::Bool(true)));
            let h = adapter.materialize(
                task_id()
                    .ok_or_else(|| AdapterError::MalformedRequest("missing `task_id`".into()))?,
                root,
                verifier,
            )?;
            Ok(Json::obj([("handle", h.to_json())]))
        }
        AdapterOp::Expose => {
            let env = env_from_json(
                req.get("handle")
                    .ok_or_else(|| AdapterError::MalformedRequest("missing `handle`".into()))?,
            )
            .map_err(AdapterError::MalformedRequest)?;
            let profile = req
                .get("profile_ref")
                .and_then(Json::as_str)
                .unwrap_or("profile/default");
            let ex = adapter.expose(&env, profile)?;
            Ok(Json::obj([("exposed", ex.to_json())]))
        }
        AdapterOp::CollectSubmission => {
            let env = env_from_json(
                req.get("handle")
                    .ok_or_else(|| AdapterError::MalformedRequest("missing `handle`".into()))?,
            )
            .map_err(AdapterError::MalformedRequest)?;
            let s = adapter.collect_submission(&env)?;
            Ok(Json::obj([("submission", s.to_json())]))
        }
        AdapterOp::Grade => {
            let env = env_from_json(
                req.get("handle")
                    .ok_or_else(|| AdapterError::MalformedRequest("missing `handle`".into()))?,
            )
            .map_err(AdapterError::MalformedRequest)?;
            let sub = adapter
                .collect_submission(&env)
                .map_err(|e| AdapterError::MalformedRequest(format!("collect: {e}")))?;
            let task = adapter.task(&env.task_id)?;
            let isolation = task
                .instrument
                .verifier_isolation
                .unwrap_or(VerifierIsolation::Separate);
            let result = adapter
                .grade(&GradeRequest {
                    submission: sub,
                    task,
                    isolation,
                })
                .map_err(|e| AdapterError::MalformedRequest(format!("grade: {e}")))?;
            Ok(Json::obj([("grade", result.to_json())]))
        }
        AdapterOp::Export | AdapterOp::Parity => Err(AdapterError::UnknownOp(format!(
            "{} rides the export/parity driver, not this binary",
            op.as_str()
        ))),
    })();
    match out {
        Ok(j) => {
            println!("{}", j.to_canonical_string());
            0
        }
        Err(e) => fail(&format!("{e:?}"), &e.to_string()),
    }
}

fn fail(kind: &str, msg: &str) -> i32 {
    let env = Json::obj([
        ("schema", Json::str("adapter_error/1")),
        ("kind", Json::str(kind)),
        ("detail", Json::str(msg)),
    ]);
    println!("{}", env.to_canonical_string());
    2
}
