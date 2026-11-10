//! `fp-prereg` — the R6.33 emitter (manifest 291). Reads the committed
//! records (selection, select commit, exclusion record, W1 re-proof
//! packets + registers, live evidence, harvest manifest, declarations)
//! and emits the `hh-fp-prereg/1` packet, its sha256 sidecar, the W2
//! `comparative-quote/1` and the `hh-fp-readiness/1` record under
//! `docs/build/reports/`. Deterministic — same inputs, byte-identical
//! outputs. No network, no clock (RECORDED_AT is the declared
//! constant).
//!
//! ```text
//! fp-prereg emit <repo-root>
//! ```

use std::path::{Path, PathBuf};

use hharness_runner::fp_prereg::*;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    hharness_wire::json::parse(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn sha(repo: &Path, rel: &str) -> String {
    sha256_hex(&std::fs::read(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-prereg: wrote {rel}");
}

fn emit(repo: &Path) {
    let selection = load(repo, SELECTION_PATH);
    let commit = load(repo, SELECT_COMMIT_PATH);
    let exclusion = load(repo, EXCLUSION_PATH);
    let held = load(repo, REPROOF_HELD_PATH);
    let dev = load(repo, REPROOF_DEV_PATH);

    let mut verdicts = reproof_verdicts(&held).unwrap_or_else(|e| panic!("held packet: {e}"));
    verdicts.extend(reproof_verdicts(&dev).unwrap_or_else(|e| panic!("dev packet: {e}")));

    let downloads: std::collections::BTreeMap<String, String> = exclusion
        .get("download_tasks")
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    Some((
                        d.get("task_id")?.as_str()?.to_string(),
                        format!(
                            "excluded_if_drawn_and_replaced — {}",
                            d.get("reason")?.as_str()?
                        ),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let freeze = freeze_post_replacement(&selection, &commit, &verdicts, &downloads)
        .unwrap_or_else(|e| panic!("freeze: {e}"));

    let freeze_digest = held
        .get("freeze")
        .and_then(|f| f.get("digest"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("held packet freeze.digest absent"))
        .to_string();

    let admission_shas = ADMISSION_RECORDS.map(|p| sha(repo, p));
    let packet = packet_with_v5(build_packet(
        freeze.clone(),
        &sha(repo, SELECT_COMMIT_PATH),
        &sha(repo, SELECTION_PATH),
        selection
            .get("selection_id")
            .and_then(Json::as_str)
            .unwrap_or_default(),
        commit
            .get("select_id")
            .and_then(Json::as_str)
            .unwrap_or_default(),
        &sha(repo, EXCLUSION_PATH),
        &sha(repo, REPROOF_HELD_PATH),
        &sha(repo, REPROOF_HELD_REGISTER_PATH),
        &sha(repo, REPROOF_DEV_PATH),
        &sha(repo, REPROOF_DEV_REGISTER_PATH),
        &sha(repo, REPROOF_EVIDENCE_PATH),
        &sha(repo, W1_MANIFEST_PATH),
        &freeze_digest,
        &admission_shas,
    ));

    let canonical = packet.to_canonical_string() + "\n";
    let packet_hash = packet_sha256(&packet);
    write(repo, PACKET_PATH, &canonical);
    write(
        repo,
        PACKET_SHA256_PATH,
        &format!(
            "{packet_hash}  {PACKET_PATH}\n# recompute: shasum -a 256 -c {PACKET_SHA256_PATH} (run from the repo root — the packet is canonical Json::to_canonical_string output, CC1)\n"
        ),
    );

    // The W2 comparative-quote/1 — compute lines cited verbatim from
    // the committed declarations' authorization blocks (C-11: priced
    // only by R6.14's receipt_lib quote path).
    let mut compute = Vec::new();
    for d in WINDOW_DECL_PATHS {
        let dj = hharness_wire::json::parse_provider(
            &std::fs::read_to_string(repo.join(d)).unwrap_or_else(|e| panic!("read {d}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse {d}: {e}"));
        let auth = dj
            .get("authorization")
            .unwrap_or_else(|| panic!("{d}: authorization absent"));
        let quote_id = auth
            .get("quote_id")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("{d}: authorization.quote_id absent"));
        let ceiling = auth
            .get("ceiling_micros")
            .and_then(|v| match v {
                Json::Int(i) => Some(*i),
                Json::Str(s) => s.parse::<i64>().ok(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{d}: authorization.ceiling_micros absent"));
        let window = dj
            .get("window")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("{d}: window absent"))
            .to_string();
        compute.push(hharness_eval::comparative_quote::ComputeLine {
            window,
            quote_id: quote_id.to_string(),
            ceiling_micros: ceiling,
        });
    }
    let quote = w2_quote(compute).unwrap_or_else(|e| panic!("w2 quote: {e}"));
    write(
        repo,
        W2_QUOTE_PATH,
        &(quote.to_json().to_canonical_string() + "\n"),
    );

    // The FP-S readiness record — committed in its pending state:
    // HUMAN-H14 has not run (no readout file exists); the W1 manifest
    // is verified (the R6.32b harvest verified all 32,636 members and
    // the receipts are committed).
    let h14: Option<String> = {
        let p = repo.join("docs/build/readouts/HUMAN-H14.md");
        if p.exists() {
            panic!(
                "HUMAN-H14 readout exists — re-derive the readiness instant per the record rules"
            );
        }
        None
    };
    let w1_verified = Some("2026-11-10 — R6.32b nine-step teardown: 32,636-member sha256 manifest verified byte-exact before instance deletion");
    let task_lists_state = freeze
        .get("state")
        .and_then(Json::as_str)
        .unwrap_or("unknown")
        .to_string();
    let readiness = compute_readiness(
        RECORDED_AT,
        &task_lists_state,
        h14.as_deref(),
        w1_verified,
        Q_X62_TRIGGER_AFTER_DAYS,
    );
    write(
        repo,
        READINESS_PATH,
        &(readiness.to_canonical_string() + "\n"),
    );

    println!("fp-prereg: packet sha256 {packet_hash}");
    println!("fp-prereg: task_lists state {task_lists_state}");
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("emit") if args.len() == 3 => {
            emit(&PathBuf::from(&args[2]));
            std::process::ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: fp-prereg emit <repo-root>");
            std::process::ExitCode::from(2)
        }
    }
}
