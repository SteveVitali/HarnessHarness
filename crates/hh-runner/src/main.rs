//! `hh-runner` — the committed runner binary (§3.2): `coordinator` drives
//! an experiment drive packet end to end; `slot-worker` runs one slot
//! attempt against its declared packet; `store-probe` is the topology
//! spike leg (cross-process read of a root another process writes, and
//! the second-writer refusal).

use std::path::PathBuf;
use std::process::ExitCode;

use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_runner::coordinator::{self, CoordinatorConfig};
use hharness_runner::packet::{DrivePacket, SlotPacket};
use hharness_runner::slot_record::RunnerIdentity;
use hharness_runner::worker::run_slot_worker;
use hharness_wire::json::{self, Json};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  hh-runner coordinator --packet <drive.json> --experiment-root <dir> --slots-dir <dir> --packet-dir <dir>\n  hh-runner slot-worker --packet <slot.json>\n  hh-runner store-probe --root <dir> --mode <read|write> [--run <id>] [--writer-ms <n>] [--appends <n>]"
    );
    ExitCode::from(2)
}

fn read_json(path: &str) -> Result<Json, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    json::parse(&text).map_err(|e| format!("parse {path}: {e:?}"))
}

fn coordinator(args: &[String]) -> Result<String, String> {
    let packet_path = arg(args, "--packet").ok_or("missing --packet")?;
    let experiment_root = arg(args, "--experiment-root").ok_or("missing --experiment-root")?;
    let slots_dir = arg(args, "--slots-dir").ok_or("missing --slots-dir")?;
    let packet_dir = arg(args, "--packet-dir").ok_or("missing --packet-dir")?;
    let packet = DrivePacket::from_json(&read_json(&packet_path)?).map_err(|e| e.to_string())?;
    let cfg = CoordinatorConfig {
        experiment_root: PathBuf::from(&experiment_root),
        slots_dir: PathBuf::from(&slots_dir),
        packet_dir: PathBuf::from(&packet_dir),
        runner: RunnerIdentity::detect(),
        max_parallel: packet.max_parallel_slots as usize,
    };
    let mut store = Store::open(&cfg.experiment_root).map_err(|e| format!("store: {e}"))?;
    let docs = hharness_experiment::docs::LabDocs::open(&cfg.experiment_root)
        .map_err(|e| format!("docs: {e:?}"))?;
    let bin = std::env::current_exe().map_err(|e| format!("exe: {e}"))?;
    let report =
        coordinator::drive(&mut store, &docs, &packet, &cfg, &bin).map_err(|e| e.to_string())?;
    let rows: Vec<Json> = report
        .settled
        .iter()
        .map(|(id, row)| {
            let mut m = std::collections::BTreeMap::new();
            m.insert("run_plan_id".to_string(), Json::str(id));
            if let Json::Obj(row) = row {
                for (k, v) in row {
                    m.insert(k.clone(), v.clone());
                }
            }
            Json::Obj(m)
        })
        .collect();
    Ok(Json::obj([
        ("run_id", Json::str(&report.run_id)),
        ("close_status", Json::str(&report.close_status)),
        ("settled", Json::Arr(rows)),
        ("runner", cfg.runner.to_json()),
    ])
    .to_canonical_string())
}

fn slot_worker(args: &[String]) -> Result<String, String> {
    let packet_path = arg(args, "--packet").ok_or("missing --packet")?;
    let packet = SlotPacket::from_json(&read_json(&packet_path)?).map_err(|e| e.to_string())?;
    let root = PathBuf::from(&packet.slot_root);
    let seal = run_slot_worker(&packet, &root).map_err(|e| e.to_string())?;
    Ok(Json::obj([
        ("slot_run_id", Json::str(&seal.slot_run_id)),
        ("slot_ref", Json::str(&seal.slot_ref)),
        ("record_ref", Json::str(&seal.record_ref)),
        ("disposition", Json::str(seal.disposition.as_str())),
        ("seal_head_hash", Json::str(&seal.seal_head_hash)),
    ])
    .to_canonical_string())
}

/// The topology probe leg: `--mode read` opens the root and reports the
/// committed head of `--run` (a reader writes nothing — `Store::open`
/// only creates `runs/`/`blobs/` and `store.tag` on a fresh root);
/// `--mode write` tries `acquire_writer` on the run and reports the
/// refusal or the acquisition. The spike's evidence is the JSON line.
fn store_probe(args: &[String]) -> Result<String, String> {
    let root = arg(args, "--root").ok_or("missing --root")?;
    let mode = arg(args, "--mode").ok_or("missing --mode")?;
    let run = arg(args, "--run").unwrap_or_else(|| "probe-run".to_string());
    let writer_ms: u64 = arg(args, "--writer-ms")
        .and_then(|v| v.parse().ok())
        .unwrap_or(60_000);
    let appends: u64 = arg(args, "--appends")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    match mode.as_str() {
        "read" => {
            let store = Store::open(&root).map_err(|e| format!("open: {e}"))?;
            let (head, seqs) = if store.has_run(&run) {
                let evs = store.events(&run).map_err(|e| format!("events: {e}"))?;
                (
                    store.head_event_id(&run).unwrap_or_default(),
                    evs.len() as u64,
                )
            } else {
                ("absent".to_string(), 0)
            };
            Ok(Json::obj([
                ("mode", Json::str("read")),
                ("run", Json::str(&run)),
                ("head", Json::str(&head)),
                ("committed_events", Json::Int(seqs as i64)),
            ])
            .to_canonical_string())
        }
        "write" => {
            let mut store = Store::open(&root).map_err(|e| format!("open: {e}"))?;
            if !store.has_run(&run) {
                // Creating the run acquires a writer for `probe` — release
                // it so the probe's own second-acquire measures the fence,
                // never its own hold.
                let (_rid, lease) = store
                    .open_run_with_id(&run, RunManifest::minimal(RunKind::Agent), "probe")
                    .map_err(|e| format!("open_run: {e}"))?;
                store
                    .release(&lease, "probe_setup")
                    .map_err(|e| format!("release: {e}"))?;
            }
            match store.acquire_writer("probe-2", &run, writer_ms) {
                Ok(lease) => {
                    for _ in 0..appends {
                        let e = hharness_ledger::event::Event {
                            event_id: store.alloc_id("evt"),
                            class: "lifecycle.replay.started".to_string(),
                            ts: store.ts_now(),
                            hlc: None,
                            producer: hharness_ledger::event::Producer::kernel("hh-runner/1"),
                            scope: hharness_ledger::event::Scope::default(),
                            parent_event_id: store.head_event_id(&run).unwrap_or_default(),
                            causes: vec![],
                            refs: vec![],
                            ir_refs: vec![],
                            surface_ids: std::collections::BTreeMap::new(),
                            provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
                                "hh-runner/1",
                                store.now_ms(),
                            )),
                            content_kind: None,
                            payload: Json::obj([("probe", Json::Bool(true))]),
                        };
                        store
                            .append(&run, &lease, vec![e])
                            .map_err(|e| format!("append: {e}"))?;
                    }
                    Ok(Json::obj([
                        ("mode", Json::str("write")),
                        ("run", Json::str(&run)),
                        ("writer", Json::str("acquired")),
                        ("appends", Json::Int(appends as i64)),
                    ])
                    .to_canonical_string())
                }
                Err(hharness_ledger::errors::LedgerError::WouldBlock { active_holder }) => {
                    Ok(Json::obj([
                        ("mode", Json::str("write")),
                        ("run", Json::str(&run)),
                        ("writer", Json::str("refused")),
                        ("active_holder", Json::str(&active_holder)),
                    ])
                    .to_canonical_string())
                }
                Err(e) => Err(format!("acquire_writer: {e}")),
            }
        }
        other => Err(format!("mode {other} unknown")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        return usage();
    };
    let rest = &args[1..];
    let out = match cmd.as_str() {
        "coordinator" => coordinator(rest),
        "slot-worker" => slot_worker(rest),
        "store-probe" => store_probe(rest),
        _ => return usage(),
    };
    match out {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hh-runner: {e}");
            ExitCode::from(1)
        }
    }
}
