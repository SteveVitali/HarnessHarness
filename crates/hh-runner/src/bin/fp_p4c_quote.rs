//! `fp-p4c-quote` — the GATE-G36 (G-P4-C) quote emitter (manifest
//! 330/R7.4). Reads the committed records named by the inputs
//! manifest — the frozen `hh-p4-c-prereg/1` confirmation
//! pre-registration (whose member list the selection packet's output
//! carries verbatim), the minted `hh-p4-x-exploration/1` packet (the
//! X1-measured per-level $/trial and per-slot wall_ms the model lines
//! are priced from — the first measured figures, never the §10.4
//! declared priors), the p4c `hh-compute-decl/1` declaration and its
//! minted `hh-compute-quote/1` record (receipt_lib's one §3.10 path) —
//! and mints the `comparative-quote/1` packet through
//! `hharness_eval::comparative_quote` (C-11 — the single quote
//! artifact) plus the `hh-p4-c-quote-inputs/1` declared-inputs record
//! it cites, each with its sha256 sidecar at the manifest's output
//! relpaths.
//!
//! Rows are the selection-conditioned scenarios verbatim: one row per
//! confirmation member (`baseline` re-run + each selected member; a
//! `withheld{no_candidate}` outcome prices the baseline re-run alone
//! and says so). Slot-level arms at `t = k = 1` (the R6.32/W2
//! precedent): member : held-out task : replicate, n_trials the real
//! slot count per member (37 × 5 = 185). Model lines are X1-MEASURED
//! per-level $/trial — mean over executed slots and the measured p90
//! over the same per-slot distribution, `measure_ref` says so
//! verbatim; `model_occupied_ms` is the measured per-slot wall mean
//! from the slot matrix. Under `withheld{no_candidate}` the member
//! rows carry measured figures anyway (the envelope the gate could
//! authorize) — the disposition member records that no subject is
//! scheduled. The packet authorizes nothing.
//!
//! `HH_FP_P4C_INPUTS` names the manifest (`scripts/fp-p4c-inputs.txt`
//! by default). Deterministic — same inputs, byte-identical outputs.
//! No network, no clock.
//!
//! ```text
//! fp-p4c-quote emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuoteDecl, ComputeLine, Envelope, MenuRowDecl,
};
use hharness_eval::stats::quantile;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::{parse, parse_provider, Json};
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    parse(&std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn load_provider(repo: &Path, rel: &str) -> Json {
    parse_provider(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-p4c-quote: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest = std::env::var("HH_FP_P4C_INPUTS")
        .unwrap_or_else(|_| "scripts/fp-p4c-inputs.txt".to_string());
    let mut out = BTreeMap::new();
    for line in std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {manifest}: {e}"))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("bad line: {line}"));
        out.insert(k.trim().to_string(), v.trim().to_string());
    }
    out
}

fn expect_schema(rec: &Json, want: &str, rel: &str) {
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(want),
        "{rel} is not {want}"
    );
}

/// An integer member read int-or-str (provider-projection folds
/// non-integer lexemes to strings verbatim — fp_p4x_quote's rule).
fn int_member(j: Option<&Json>, what: &str) -> i64 {
    match j {
        Some(Json::Int(i)) => *i,
        Some(Json::Str(s)) => s
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("{what}: not an integer string")),
        _ => panic!("{what} absent"),
    }
}

/// Per-arm measured figures — mean + p90 per executed slot over the
/// arm's full executed-slot census (captured ∪ detail_lost — a
/// detail_lost slot ran, attributed no calls, and carries 0 µ$;
/// dropping it would overstate the mean). Wall_ms is measured over
/// the captured census only (the lost slots' walls were never
/// recorded — the measured wall is the captured-slot mean, and the
/// label says so).
fn measured(
    wal_map: &BTreeMap<String, i64>,
    captured: &[(String, i64)],
    executed: &[String],
    arm: &str,
) -> (i64, i64, u64, u64, u64) {
    let belongs = |s: &str| {
        hharness_runner::fp_p4x_fold::slot_arm_task(s)
            .map(|(a, _)| a == arm)
            .unwrap_or(false)
    };
    let mut costs: Vec<i64> = executed
        .iter()
        .filter(|s| belongs(s))
        .map(|s| wal_map.get(s).copied().unwrap_or(0))
        .collect();
    costs.sort_unstable();
    let n = costs.len() as i64;
    let mean = if n > 0 {
        costs.iter().sum::<i64>() / n
    } else {
        0
    };
    let p90 = quantile(&costs, 900_000).unwrap_or(0);
    let mine: Vec<i64> = captured
        .iter()
        .filter(|(s, _)| belongs(s))
        .map(|(_, w)| *w)
        .collect();
    let wm = if mine.is_empty() {
        0
    } else {
        mine.iter().sum::<i64>() / mine.len() as i64
    };
    (mean, p90, wm.max(0) as u64, n as u64, mine.len() as u64)
}

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    let cprereg = load(repo, &inputs["c_prereg"]);
    let exploration = load(repo, &inputs["exploration"]);
    let decl = load_provider(repo, &inputs["compute_decl"]);
    let compute_rec = load_provider(repo, &inputs["compute_quote"]);
    expect_schema(&cprereg, "hh-p4-c-prereg/1", &inputs["c_prereg"]);
    expect_schema(
        &exploration,
        "hh-p4-x-exploration/1",
        &inputs["exploration"],
    );
    expect_schema(&decl, "hh-compute-decl/1", &inputs["compute_decl"]);
    expect_schema(&compute_rec, "hh-compute-quote/1", &inputs["compute_quote"]);

    // The member list is the c-prereg's verbatim — never re-derived.
    let members = cprereg
        .get("members")
        .and_then(Json::as_arr)
        .unwrap_or_else(|| panic!("c-prereg.members absent"));
    let held_out = cprereg
        .get("held_out")
        .and_then(Json::as_arr)
        .unwrap_or_else(|| panic!("c-prereg.held_out absent"));
    let k = cprereg
        .get("design")
        .and_then(|d| d.get("k"))
        .and_then(Json::as_int)
        .unwrap_or_else(|| panic!("c-prereg.design.k absent"));

    // The measured per-slot cost/wall surfaces.
    let wal = load(repo, &inputs["wal_slot_costs"]);
    let wal_map: BTreeMap<String, i64> = wal
        .get("slots")
        .and_then(Json::as_obj)
        .unwrap_or_else(|| panic!("wal-slot-costs.slots absent"))
        .iter()
        .map(|(k, v)| (k.clone(), v.as_int().unwrap_or(0)))
        .collect();
    let matrix = std::fs::read_to_string(repo.join(&inputs["slot_matrix"]))
        .unwrap_or_else(|e| panic!("read slot_matrix: {e}"));
    let mut captured: Vec<(String, i64)> = Vec::new();
    for line in matrix.lines() {
        if let Ok(j) = parse_provider(line) {
            if let Some(s) = j.get("slot_id").and_then(Json::as_str) {
                let w = match j.get("wall_ms") {
                    Some(Json::Int(i)) => *i,
                    Some(Json::Str(t)) => t.parse::<i64>().unwrap_or(0),
                    _ => 0,
                };
                captured.push((s.to_string(), w));
            }
        }
    }
    // The executed census = captured slots ∪ the recorded detail_lost
    // ids (each ran under the envelope, attributed no calls).
    let mut executed: Vec<String> = captured.iter().map(|(s, _)| s.clone()).collect();
    let lost = load(repo, &inputs["slots_lost"]);
    for s in lost
        .as_arr()
        .unwrap_or_else(|| panic!("slots_lost is not an array"))
    {
        executed.push(
            s.as_str()
                .unwrap_or_else(|| panic!("slots_lost member is not a string"))
                .to_string(),
        );
    }

    let disposition = cprereg
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap_or("");
    let no_candidate = disposition.starts_with("withheld");

    let mut rows = Vec::new();
    for m in members {
        let member = m
            .get("member")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("member without id"));
        if member == "C₁₂" {
            continue; // the combination prices under its member rows.
        }
        let (mean, p90, wall, n, n_wall) = measured(&wal_map, &captured, &executed, member);
        assert!(n > 0, "member {member} has no measured slots");
        let measure_ref = format!(
            "X1-measured per-slot µ$ — WAL attribution over {n} executed slots (captured ∪ detail_lost; a lost slot carried 0 µ$) (mean {mean}, p90 {p90}); wall mean {wall} ms over {n_wall} captured slots — MEASURED, never a declared planning prior"
        );
        let mut arms = Vec::new();
        for t in held_out {
            let task = t.as_str().unwrap_or("");
            for r in 0..k {
                arms.push(ArmMeasure {
                    arm_id: format!("{member}:{task}:r{r}"),
                    measure_ref: measure_ref.clone(),
                    mean_trial_micros: mean,
                    p90_trial_micros: p90,
                    model_occupied_ms: wall,
                    queue_wait_ms: 0,
                });
            }
        }
        rows.push(MenuRowDecl {
            row_id: format!("p4c-{member}"),
            arms,
            infra_allowance_ppm: 0,
            sim: None,
        });
    }
    // Under no_candidate the c-prereg's member list is [baseline] —
    // the quote prices the pre-registered envelope as presented and
    // the scenario member records that no subject is scheduled.

    let auth = decl
        .get("authorization")
        .unwrap_or_else(|| panic!("compute decl: authorization absent"));
    let quote_id = auth
        .get("quote_id")
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("compute decl: authorization.quote_id absent"))
        .to_string();
    let ceiling = int_member(
        auth.get("ceiling_micros"),
        "compute decl: authorization.ceiling_micros",
    );
    let window = decl
        .get("window")
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("compute decl: window absent"))
        .to_string();
    let rec_ceiling = int_member(
        compute_rec
            .get("pricing")
            .and_then(|p| p.get("ceiling_micros")),
        "compute quote: pricing.ceiling_micros",
    );
    assert_eq!(
        ceiling, rec_ceiling,
        "compute decl ceiling disagrees with the hh-compute-quote/1 record"
    );
    assert_eq!(
        compute_rec.get("window").and_then(Json::as_str),
        Some(window.as_str()),
        "compute quote window disagrees with the declaration"
    );
    let compute = vec![ComputeLine {
        window,
        quote_id: quote_id.clone(),
        ceiling_micros: ceiling,
    }];

    let decl_q = ComparativeQuoteDecl {
        t: 1,
        k: 1,
        concurrency_c: 4,
        rows,
        compute,
        envelope: Envelope::PreQuote,
    };
    let quote = build_quote(&decl_q).unwrap_or_else(|e| panic!("comparative quote: {e}"));
    assert!(quote.check_id(), "quote id failed self-check");

    let sha_of = |rel: &str| -> String {
        sha256_hex(
            std::fs::read(repo.join(rel))
                .unwrap_or_else(|e| panic!("read {rel}: {e}"))
                .as_slice(),
        )
    };
    let member_ids: Vec<Json> = members
        .iter()
        .filter_map(|m| m.get("member").and_then(Json::as_str))
        .map(Json::str)
        .collect();
    let inputs_rec = Json::obj([
        ("schema", Json::str("hh-p4-c-quote-inputs/1")),
        ("emitted_by", Json::str("fp_p4c_quote emit (crates/hh-runner/src/bin/fp_p4c_quote.rs)")),
        ("ticket", Json::str("330_R7.4 — GATE-G36 presentation")),
        (
            "records",
            Json::obj([
                (
                    "c_prereg",
                    Json::obj([
                        ("path", Json::str(&inputs["c_prereg"])),
                        ("sha256", Json::str(sha_of(&inputs["c_prereg"]))),
                        ("schema", Json::str("hh-p4-c-prereg/1")),
                    ]),
                ),
                (
                    "exploration",
                    Json::obj([
                        ("path", Json::str(&inputs["exploration"])),
                        ("sha256", Json::str(sha_of(&inputs["exploration"]))),
                        ("schema", Json::str("hh-p4-x-exploration/1")),
                    ]),
                ),
                (
                    "compute_decl",
                    Json::obj([
                        ("path", Json::str(&inputs["compute_decl"])),
                        ("sha256", Json::str(sha_of(&inputs["compute_decl"]))),
                        ("schema", Json::str("hh-compute-decl/1")),
                    ]),
                ),
                (
                    "compute_quote",
                    Json::obj([
                        ("path", Json::str(&inputs["compute_quote"])),
                        ("sha256", Json::str(sha_of(&inputs["compute_quote"]))),
                        ("schema", Json::str("hh-compute-quote/1")),
                        ("quote_id", Json::str(&quote_id)),
                        ("priced_by", Json::str("receipt_lib.py quote_doc — the one §3.10 path (R6.14)")),
                    ]),
                ),
            ]),
        ),
        (
            "model_line_basis",
            Json::obj([
                ("basis", Json::str("X1-measured per-slot µ$ — the session WAL's authoritative attribution (mean and p90 over executed slots per member level); the §10.4 declared planning priors do not feed this packet")),
                ("per_trial_bound_seconds", Json::Int(3738)),
            ]),
        ),
        (
            "scenarios",
            Json::obj([
                ("selection_conditioned", Json::Bool(true)),
                ("disposition", Json::str(disposition)),
                ("members", Json::Arr(member_ids)),
                (
                    "note",
                    Json::str(if no_candidate {
                        "withheld{no_candidate} — the rows price the pre-registered envelope as presented; no subject is scheduled and GATE-G36 presents that state verbatim"
                    } else {
                        "the member list equals the selection packet's output verbatim; the declared C₁₂ combination rides only when the packet's combination member fired (it did not — one factor qualified)"
                    }),
                ),
            ]),
        ),
        (
            "envelope",
            Json::obj([
                ("held_out_tasks", Json::Int(held_out.len() as i64)),
                ("k", Json::Int(k)),
                ("slots_per_member", Json::Int(held_out.len() as i64 * k)),
                ("concurrency_c", Json::Int(4)),
                ("envelope_label", Json::str("pre-quote")),
            ]),
        ),
        (
            "honesty",
            Json::obj([
                ("authorizes_nothing", Json::Bool(true)),
                ("model_line_source", Json::str("X1-measured — not a declared planning prior")),
                (
                    "note",
                    Json::str("the packet mints the priced menu only; REQ-XH-004's verbatim model + compute authorizations are recorded at GATE-G36 by the operator"),
                ),
            ]),
        ),
    ]);

    for (key, doc) in [("quote", quote.to_json()), ("quote_inputs", inputs_rec)] {
        let rel = &inputs[key];
        let bytes = format!("{}\n", doc.to_canonical_string());
        write(repo, rel, &bytes);
        let sha_key = format!("{key}_sha256");
        let sha_rel = &inputs[&sha_key];
        write(
            repo,
            sha_rel,
            &sidecar(&sha256_hex(bytes.as_bytes()), rel, sha_rel),
        );
    }
    println!(
        "fp-p4c-quote: comparative-quote/1 id {} — {disposition}, compute {quote_id} {ceiling} µ$",
        quote.quote_id
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".to_string());
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let inputs = inputs();
    match cmd.as_str() {
        "emit" => emit(&repo, &inputs),
        other => panic!("unknown command: {other}"),
    }
}
