//! `hh-l1-drive` — the Ws1 L1 host surface (R6.45q; spec §7.4
//! REQ-XD-103): drive the frozen HIR doc through an in-process
//! `EmbedService` against a declared model endpoint (the ws1 replay
//! session's listener) with the exec seam bound per leg:
//!
//! - `--mode obs_replay` (L1.1): after `open_session` arms the exec
//!   caps, every bound surface's executor is swapped to one
//!   `SharedObsReplayExecutor` serving O's recorded raw observations
//!   positionally (`rebind_exec_executor` re-runs the requirement
//!   bind — an insufficiency refuses typed, never a downgrade).
//! - `--mode live` (L1.2): the session's declared environment
//!   executes — the fresh HH cell the leg names.
//!
//! The run's model calls ride the `model_port` slot — the endpoint
//! reaches it through the slot's own `env:` param resolution (the
//! host exports `HH_L1_ENDPOINT`/`HH_L1_MODEL` before open) or a
//! `--set <ptr>=<json>` literal override into the document's
//! `assembly.values` mount / doc pointer space (the widening gate
//! stays armed). The replay proxy receipts every upstream-bound
//! request — those captures are the L1Drive comparison input; this
//! surface emits the run's durable event rows + terminal state for
//! the driver's fold.
//!
//! usage:
//!   hh-l1-drive --store-root D --workspace-root D --holder S \
//!     --definition F.json [--override F.json] \
//!     --environment <connection_info json> \
//!     --exec-surfaces s1,s2 --instruction F \
//!     --endpoint URL --model M \
//!     --mode live|obs_replay [--obs raws.json] \
//!     --max-submits N --out D

use std::path::PathBuf;

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::errors::EmbedError;
use hharness_env::obs_replay::{RawObservation, SharedObsReplayExecutor};
use hharness_wire::json::{parse, Json};
use hharness_wire::jsonrpc::Request;

fn usage() -> ! {
    eprintln!("{}", std::env::args().next().unwrap());
    eprintln!(
        "usage: hh-l1-drive --store-root D --workspace-root D --holder S \
         --definition F [--override F] --environment J \
         --exec-surfaces CSV --instruction F --endpoint URL --model M \
         --mode live|obs_replay [--obs F] [--max-submits N] --out D"
    );
    std::process::exit(2)
}

fn req_arg(args: &[String], name: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("missing {name}");
            usage()
        })
}

fn opt_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn read_json_file(p: &str) -> Result<Json, String> {
    let b = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    parse(&String::from_utf8_lossy(&b)).map_err(|e| format!("{p}: {e:?}"))
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Result<Json, EmbedError> {
    let r = svc.handle(&Request {
        id: Json::str(format!("l1-{method}")),
        method: method.to_string(),
        params,
    });
    if let Some(e) = r.get("error") {
        return Err(EmbedError::Refused {
            reason: e.to_canonical_string(),
        });
    }
    Ok(r.get("result").cloned().unwrap_or(Json::Null))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let store_root = PathBuf::from(req_arg(&args, "--store-root"));
    let ws_root = PathBuf::from(req_arg(&args, "--workspace-root"));
    let holder = req_arg(&args, "--holder");
    let definition_path = req_arg(&args, "--definition");
    let environment = opt_arg(&args, "--environment").unwrap_or_else(|| {
        "{\"kind\":\"connection_info\",\"connection_info\":{\"class\":\"local_host\"}}".to_string()
    });
    let surfaces_csv = opt_arg(&args, "--exec-surfaces").unwrap_or_default();
    let instruction_path = req_arg(&args, "--instruction");
    let endpoint = req_arg(&args, "--endpoint");
    let model = req_arg(&args, "--model");
    let mode = req_arg(&args, "--mode");
    let max_submits: usize = opt_arg(&args, "--max-submits")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let out_dir = PathBuf::from(req_arg(&args, "--out"));
    let overrides_path = opt_arg(&args, "--override");

    if !["live", "obs_replay"].contains(&mode.as_str()) {
        eprintln!("mode must be live|obs_replay");
        std::process::exit(2);
    }
    let raws: Option<Vec<RawObservation>> = if mode == "obs_replay" {
        let p = req_arg(&args, "--obs");
        match read_json_file(&p) {
            Ok(Json::Arr(items)) => Some(
                items
                    .iter()
                    .map(|j| RawObservation {
                        output: j
                            .get("output")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        returncode: j
                            .get("returncode")
                            .and_then(Json::as_int)
                            .unwrap_or_default(),
                        exception_info: j.get("exception_info").cloned(),
                    })
                    .collect(),
            ),
            other => {
                eprintln!("--obs must be a JSON array ({other:?})");
                std::process::exit(2)
            }
        }
    } else {
        None
    };

    // The `model_port` slot's `env:` params resolve at arm from the
    // host environment — export the realized endpoint + model (the
    // slot binds `env:HH_L1_ENDPOINT`/`env:HH_L1_MODEL` spellings; a
    // doc that binds literals instead needs `--override` values or a
    // pointer write — the widening gate stays armed either way).
    // SAFETY: single-threaded argument setup before the service opens.
    unsafe {
        std::env::set_var("HH_L1_ENDPOINT", &endpoint);
        std::env::set_var("HH_L1_MODEL", &model);
    }

    let document = match read_json_file(&definition_path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("definition: {e}");
            std::process::exit(2)
        }
    };
    let overrides = match &overrides_path {
        Some(p) => match read_json_file(p) {
            Ok(Json::Arr(a)) => a,
            _ => {
                eprintln!("--override must be a JSON array of Override docs");
                std::process::exit(2)
            }
        },
        None => vec![],
    };
    let instruction = match std::fs::read_to_string(&instruction_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("instruction: {e}");
            std::process::exit(2)
        }
    };
    let env_json = match parse(&environment) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("environment json: {e:?}");
            std::process::exit(2)
        }
    };

    let mut spec_members = vec![
        ("kind".to_string(), Json::str("new")),
        (
            "definition".to_string(),
            Json::obj([("kind", Json::str("document")), ("document", document)]),
        ),
        ("overrides".to_string(), Json::Arr(overrides)),
        ("environment".to_string(), env_json),
        (
            "attendance".to_string(),
            Json::obj([
                ("value", Json::str("async")),
                ("source", Json::str("declared")),
            ]),
        ),
    ];
    let surfaces: Vec<Json> = surfaces_csv
        .split(',')
        .filter(|s| !s.is_empty())
        .map(Json::str)
        .collect();
    if !surfaces.is_empty() {
        spec_members.push((
            "exec".to_string(),
            Json::obj([
                ("env", Json::str("session")),
                ("surfaces", Json::Arr(surfaces)),
            ]),
        ));
    }
    let spec = Json::Obj(spec_members.into_iter().collect());

    let mut svc = match EmbedService::open(ServiceConfig {
        store_root: store_root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: ws_root,
        holder,
    }) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("service open: {e:?}");
            std::process::exit(1)
        }
    };

    let opened = match call(
        &mut svc,
        "open_session",
        Json::obj([("spec", spec), ("idempotency_key", Json::str("l1-drive"))]),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("open_session: {e:?}");
            std::process::exit(1)
        }
    };
    let sess_id = opened
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let run_id = opened
        .get("run_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();

    // L1.1 — the observation-replay executor double: one shared queue
    // across every armed exec surface (spec §7.4: "an
    // observation-replay executor fed O's raw observations").
    if let Some(raws) = raws {
        let shared = SharedObsReplayExecutor::new(raws);
        for sid in svc.exec_surface_ids(&sess_id) {
            if let Err(e) = svc.rebind_exec_executor(&sess_id, &sid, Box::new(shared.clone())) {
                eprintln!("rebind {sid}: {e:?}");
                std::process::exit(1)
            }
        }
    }

    let mut finished = false;
    let mut submits = 0;
    while submits < max_submits && !finished {
        submits += 1;
        match call(
            &mut svc,
            "submit",
            Json::obj([
                ("session_id", Json::str(&sess_id)),
                (
                    "input",
                    Json::Arr(vec![Json::obj([
                        ("kind", Json::str("text")),
                        ("text", Json::str(&instruction)),
                    ])]),
                ),
                ("idempotency_key", Json::str(format!("l1-submit-{submits}"))),
            ]),
        ) {
            Ok(_) => {}
            Err(EmbedError::Draining) => {
                finished = true;
            }
            Err(e) => {
                eprintln!("submit {submits}: {e:?}");
                std::process::exit(1)
            }
        }
        if let Some((fin, active)) = svc.session_state(&sess_id) {
            finished = finished || fin;
            // A parked turn awaiting input means the drive went off
            // the recorded trajectory — stop honestly, the diff is
            // the fold's.
            if !finished && !active {
                break;
            }
        }
    }

    std::fs::create_dir_all(&out_dir).ok();
    let events = svc
        .surface_events(&run_id)
        .unwrap_or_default()
        .iter()
        .map(|e| {
            Json::obj([
                ("class", Json::str(&e.class)),
                ("payload", e.payload.clone()),
            ])
        })
        .collect::<Vec<_>>();
    let events_j = Json::Arr(events);
    let _ = std::fs::write(out_dir.join("events.json"), events_j.to_canonical_string());
    let report = Json::obj([
        ("schema", Json::str("hh-l1-drive-report/1")),
        ("run_id", Json::str(&run_id)),
        ("session_id", Json::str(&sess_id)),
        ("mode", Json::str(&mode)),
        ("endpoint", Json::str(&endpoint)),
        ("submits", Json::Int(submits as i64)),
        ("finished", Json::Bool(finished)),
    ]);
    let _ = std::fs::write(out_dir.join("drive.json"), report.to_canonical_string());
    println!(
        "{}",
        Json::obj([
            ("run_id", Json::str(&run_id)),
            ("finished", Json::Bool(finished)),
            ("submits", Json::Int(submits as i64)),
        ])
        .to_canonical_string()
    );
}
