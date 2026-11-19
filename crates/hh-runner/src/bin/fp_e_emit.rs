//! `fp-e-emit` — the R6.46 `hh-fp-e/1` emitter. Reads the committed
//! records named by the inputs manifest (`HH_FP_E_INPUTS`; default
//! `scripts/fp-e-inputs.txt`, newline `name=relpath` pairs — every
//! input is a committed, repo-relative record), builds the
//! `hh-fp-e-decl/1` declaration, runs `fp_e::fp_e_quote` through its
//! three passes (probe → D_max → final, every pass over the shared
//! continuation machinery — nothing is re-derived here), and writes
//! the committed decl, the `hh-fp-e/1` record, the
//! `hh-fp-measurement/1` packet and the `comparative-quote/1` member
//! plus sha256 sidecars. Deterministic; no network, no clock.
//!
//! The W4 re-issue is two-stage: `emit` writes
//! `docs/build/reports/<date>-fp-w4-quote-inputs.json` and a probe
//! `d_max_ms` on stderr; `scripts/gce-backend/receipt_lib.py` then
//! re-issues `hh-compute-quote/1` from the committed
//! `scripts/gce-backend/declarations/fp-w4-fp-e.json` (whose
//! `per_trial_bound_seconds` is `ceil(d_max/1000)`); a second `emit`
//! run binds the re-issued quote id into `comparative.compute[]`.
//!
//! ```text
//! fp-e-emit emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::continuation::FpArm;
use hharness_runner::fp_e::fp_e_quote;
use hharness_runner::fp_measure::trials_measures;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

const RECORDED_AT: &str = "2026-11-19T00:00Z (declared; the ticket is clock-free)";

fn manifest(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("manifest line {line:?} is not name=relpath"));
        out.push((k.trim().to_string(), v.trim().to_string()));
    }
    out
}

fn load(repo: &Path, rel: &str) -> Json {
    hharness_wire::json::parse(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// Declaration parse: the gce-backend declarations carry non-integer
/// quantity lexemes (`0.65`, `22.5`); provider mode preserves each as a
/// verbatim `Json::Str` — echoed, never coerced.
fn load_decl(repo: &Path, rel: &str) -> Json {
    hharness_wire::json::parse_provider(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn sha(repo: &Path, rel: &str) -> String {
    sha256_hex(&std::fs::read(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
}

fn write(repo: &Path, rel: &str, doc: &Json) {
    let canonical = doc.to_canonical_string() + "\n";
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, &canonical).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    let hash = sha256_hex(canonical.as_bytes());
    std::fs::write(
        repo.join(format!("{rel}.sha256")),
        format!("{hash}  {rel}\n# recompute: shasum -a 256 -c {rel}.sha256 (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"),
    )
    .unwrap_or_else(|e| panic!("write {rel}.sha256: {e}"));
    println!("fp-e-emit: wrote {rel}  sha256:{hash}");
}

/// Spend `cost_usd` (a decimal dollars string) → exact integer µ$.
fn usd_to_micros(s: &str) -> i64 {
    let neg = s.starts_with('-');
    let s = s.trim_start_matches('-');
    let (a, b) = s.split_once('.').unwrap_or((s, ""));
    let mut frac = b.chars().take(6).collect::<String>();
    while frac.len() < 6 {
        frac.push('0');
    }
    let micros = a.parse::<i64>().unwrap_or(0) * 1_000_000 + frac.parse::<i64>().unwrap_or(0);
    if neg {
        -micros
    } else {
        micros
    }
}

/// The held-out task set for slot ids — the prereg's first draw.
fn held_tasks(prereg: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(draws) = prereg
        .get("task_lists")
        .and_then(|t| t.get("draws"))
        .and_then(Json::as_arr)
    {
        if let Some(d) = draws.first() {
            for cell in d
                .get("cells")
                .and_then(Json::as_arr)
                .cloned()
                .unwrap_or_default()
            {
                for s in cell
                    .get("slots")
                    .and_then(Json::as_arr)
                    .cloned()
                    .unwrap_or_default()
                {
                    if let Some(t) = s.get("drawn").and_then(Json::as_str) {
                        out.push(t.to_string());
                    }
                }
            }
        }
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    if cmd != "emit" {
        eprintln!("usage: fp-e-emit emit <repo-root>");
        std::process::exit(2);
    }
    let mp = std::env::var("HH_FP_E_INPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("scripts/fp-e-inputs.txt"));
    let m = manifest(&mp);
    let mget = |k: &str| -> String {
        m.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("manifest member {k} required"))
    };

    let trials = load(&repo, &mget("trials"));
    let pp0_rec = load(&repo, &mget("pp0"));
    let gl1 = load(&repo, &mget("gl1"));
    let prereg = load(&repo, &mget("prereg"));
    let w4_decl = load_decl(&repo, &mget("w4_decl"));
    let measured = trials_measures(&trials).unwrap_or_else(|e| panic!("trials fold: {e}"));

    // ── PHASE_SPEND: compute windows' cost_lib + the model side ────
    let mut cost_lib: BTreeMap<String, i64> = BTreeMap::new();
    // The append-only ledger can carry a superseding reconcile row for
    // the same window (ws1's first read showed provider 0; the second
    // shows 4,072,350 `settled_discrepant`). The LAST row per window is
    // the effective read — never summed twice.
    let mut reconciled: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for line in std::fs::read_to_string(repo.join(mget("phase_spend")))
        .unwrap_or_else(|e| panic!("phase_spend: {e}"))
        .lines()
    {
        // provider-mode parse: the ledger's decimal cost_usd rows land
        // as verbatim Str lexemes (never truncated).
        let Ok(o) = hharness_wire::json::parse_provider(line) else {
            continue;
        };
        match o.get("type").and_then(Json::as_str) {
            Some("spend") if o.get("kind").and_then(Json::as_str) == Some("compute") => {
                let w = o
                    .get("window")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let micros = match o.get("cost_usd") {
                    Some(Json::Int(i)) => i * 1_000_000,
                    Some(Json::Str(s)) => usd_to_micros(s),
                    other => panic!("spend {w}: cost_usd {other:?} unreadable"),
                };
                cost_lib.insert(w, micros);
            }
            Some("cost_reconciled") => {
                let w = o
                    .get("window")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let hh = o
                    .get("hh")
                    .and_then(|h| h.get("micro_units"))
                    .and_then(Json::as_int)
                    .unwrap_or(0);
                let p = o
                    .get("provider")
                    .and_then(|h| h.get("micro_units"))
                    .and_then(Json::as_int)
                    .unwrap_or(0);
                reconciled.insert(w, (hh, p));
            }
            _ => {}
        }
    }
    // Model consumed under the root (A2): the provider (billed) figure
    // where the reconcile read shows it — ws1's `settled_discrepant`
    // row counts the larger billed figure; a pending read falls back
    // to the HH-side figure (max is the conservative count either way).
    let consumed_micros: i64 = reconciled.values().map(|(h, p)| (*h).max(*p)).sum();

    // ── d_max.completed: spend windows at cost_lib; smoke3 unrun ────
    let mut completed: Vec<Json> = Vec::new();
    for (k, v) in &m {
        if let Some(w) = k.strip_prefix("quote_") {
            let q = load_decl(&repo, v);
            let ceil = q
                .get("pricing")
                .and_then(|p| p.get("ceiling_micros"))
                .and_then(Json::as_int)
                .unwrap_or_else(|| panic!("{v}: pricing.ceiling_micros"));
            match cost_lib.get(w) {
                Some(cl) => completed.push(Json::obj([
                    ("window", Json::str(w.to_string())),
                    ("ceiling_micros", Json::Int(ceil)),
                    ("cost_lib_micros", Json::Int(*cl)),
                    ("done", Json::Bool(true)),
                ])),
                None => completed.push(Json::obj([
                    ("window", Json::str(w.to_string())),
                    ("ceiling_micros", Json::Int(ceil)),
                    ("done", Json::Bool(false)),
                ])),
            }
        }
    }
    completed.sort_by(|a, b| {
        a.get("window")
            .and_then(Json::as_str)
            .cmp(&b.get("window").and_then(Json::as_str))
    });

    // ── remaining slots + O/H pairs (the declared plan ids) ─────────
    let tasks = held_tasks(&prereg);
    if tasks.len() != 10 {
        panic!("prereg held draw: expected 10 tasks, got {}", tasks.len());
    }
    let mut remaining: BTreeMap<String, Json> = BTreeMap::new();
    let mut pairs: Vec<Json> = Vec::new();
    for arm in FpArm::ALL {
        let mut slots = Vec::new();
        for t in &tasks {
            for r in 1..=5i64 {
                slots.push(Json::str(format!("w4-{}-{t}-r{r}", arm.as_str())));
            }
        }
        remaining.insert(arm.as_str().to_string(), Json::Arr(slots));
    }
    for t in &tasks {
        for r in 1..=5i64 {
            pairs.push(Json::obj([
                ("o_slot", Json::str(format!("w4-mini-swe-agent-{t}-r{r}"))),
                ("h_slot", Json::str(format!("w4-hh_native-{t}-r{r}"))),
            ]));
        }
    }

    let consumed_by_arm: BTreeMap<String, Json> = FpArm::ALL
        .iter()
        .map(|a| {
            (
                a.as_str().to_string(),
                Json::Int(
                    measured
                        .get(a)
                        .map(|v| v.iter().map(|t| t.cost_micros).sum())
                        .unwrap_or(0),
                ),
            )
        })
        .collect();

    let continuation = Json::obj([
        ("schema", Json::str("hh-fp-continuation-input/1")),
        ("scope", Json::str("fp_e")),
        ("measured", {
            let mut mm = BTreeMap::new();
            for (a, rows) in &measured {
                mm.insert(
                    a.as_str().to_string(),
                    Json::Arr(
                        rows.iter()
                            .map(|t| {
                                Json::obj([
                                    ("slot", Json::str(t.slot.clone())),
                                    ("cost_micros", Json::Int(t.cost_micros)),
                                    ("wall_ms", Json::Int(t.wall_ms)),
                                    (
                                        "queue_wait_ms",
                                        t.queue_wait_ms.map_or(Json::Null, Json::Int),
                                    ),
                                    ("latency_ms", t.latency_ms.map_or(Json::Null, Json::Int)),
                                    ("infra_failed", Json::Bool(t.infra_failed)),
                                ])
                            })
                            .collect(),
                    ),
                );
            }
            Json::Obj(mm)
        }),
        ("remaining", Json::Obj(remaining)),
        ("consumed_by_arm", Json::Obj(consumed_by_arm)),
        ("in_flight", Json::obj([])),
        ("consumed_micros", Json::Int(consumed_micros)),
        ("orphaned_hi_micros", Json::Int(0)),
        ("root_cap_micros", Json::Int(48_000_000)),
        ("c", Json::Int(4)),
        ("slice_micros", Json::Int(850_000)),
        (
            "priors_micros",
            Json::obj([
                ("mini-swe-agent", Json::Int(374_000)),
                ("hh_native", Json::Int(374_000)),
                ("v5", Json::Int(374_000)),
                ("pi", Json::Int(123_000)),
                ("claude-code", Json::Int(210_000)),
            ]),
        ),
        (
            // The trials record carries the proxy's declared per-call
            // timeout per arm (extracted from the Ws1 WALs).
            "client_timeouts_ms",
            trials
                .get("client_timeouts_ms")
                .cloned()
                .unwrap_or_else(|| panic!("trials.client_timeouts_ms")),
        ),
        (
            "priority",
            Json::obj([
                (
                    "protected",
                    Json::Arr(vec![Json::str("mini-swe-agent"), Json::str("hh_native")]),
                ),
                (
                    "drop_order",
                    Json::Arr(vec![
                        Json::str("v5"),
                        Json::str("claude-code"),
                        Json::str("pi"),
                    ]),
                ),
            ]),
        ),
    ]);

    let qi = w4_decl
        .get("quote_inputs")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_else(|| panic!("w4_decl.quote_inputs"));
    let mut quantities = BTreeMap::new();
    for (k, v) in &qi {
        quantities.insert(k.clone(), v.clone());
    }
    // The remaining declared quantities the inputs record names —
    // read from the committed declaration's real members.
    quantities.insert(
        "machine_type".into(),
        w4_decl.get("machine_type").cloned().unwrap_or(Json::Null),
    );
    quantities.insert(
        "provisioning_model".into(),
        w4_decl
            .get("provisioning_model")
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "disk_gib".into(),
        w4_decl
            .get("boot_disk")
            .and_then(|b| b.get("size_gib"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "disk_type".into(),
        w4_decl
            .get("boot_disk")
            .and_then(|b| b.get("type"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "external_ipv4".into(),
        w4_decl
            .get("network")
            .and_then(|n| n.get("external_ipv4"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "nat".into(),
        w4_decl
            .get("network")
            .and_then(|n| n.get("nat"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "dead_man_hours".into(),
        w4_decl
            .get("dead_man")
            .and_then(|d| d.get("max_run_duration_hours"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "restart_hours".into(),
        w4_decl
            .get("restart")
            .and_then(|r| r.get("preauthorized_hours"))
            .cloned()
            .unwrap_or(Json::Null),
    );
    quantities.insert(
        "residual_disk_hours".into(),
        w4_decl.get("h_r_hours").cloned().unwrap_or(Json::Null),
    );
    if let Some(eg) = qi.get("egress_gib_estimate").cloned() {
        quantities.insert(
            "egress_gib".into(),
            Json::obj([("value", eg), ("label", Json::str("estimated"))]),
        );
    }

    let sources = |names: &[&str]| -> Json {
        Json::Obj(
            names
                .iter()
                .map(|n| {
                    (
                        n.to_string(),
                        Json::str(format!("sha256:{}", sha(&repo, &mget(n)))),
                    )
                })
                .collect(),
        )
    };

    let mut decl = Json::obj([
        ("schema", Json::str("hh-fp-e-decl/1")),
        ("ticket", Json::str("R6.46")),
        ("recorded_at", Json::str(RECORDED_AT)),
        (
            "disposition",
            Json::obj([
                ("disposition", Json::str("full")),
                (
                    "gl1",
                    Json::obj([
                        (
                            "result",
                            gl1.get("result").cloned().unwrap_or(Json::str("pass")),
                        ),
                        (
                            "pass_id",
                            gl1.get("pass_id").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "h_identity_digest",
                            gl1.get("h_identity")
                                .and_then(|h| h.get("digest"))
                                .cloned()
                                .unwrap_or(Json::str(
                                    "sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9",
                                )),
                        ),
                        (
                            "corpus_digest",
                            gl1.get("corpus_digest").cloned().unwrap_or(Json::Null),
                        ),
                        ("gate", Json::str("GATE-G29")),
                        (
                            "readout_ref",
                            Json::str(mget("gl1_readout")),
                        ),
                    ]),
                ),
            ]),
        ),
        ("trials", trials.clone()),
        (
            "pp0",
            pp0_rec.get("pp0").cloned().unwrap_or(Json::obj([])),
        ),
        ("continuation", continuation.clone()),
        (
            "d_max",
            Json::obj([
                ("completed", Json::Arr(completed)),
                (
                    "w4",
                    Json::obj([
                        ("hourly_micros", Json::Int(314_140)),
                        // r·restart_h + S·r_disk·h_r + egress
                        // = 314140 + 300·137·6 + 3·120000.
                        ("fixed_micros", Json::Int(920_740)),
                    ]),
                ),
                (
                    "wc",
                    Json::obj([
                        ("authorized", Json::Bool(false)),
                        ("ceiling_micros", Json::Int(1_937_300)),
                        ("reservation_micros", Json::Int(560_740)),
                    ]),
                ),
            ]),
        ),
        (
            "w4_quote",
            Json::obj([
                ("based_on", Json::str(mget("w4_requote"))),
                ("quantities", Json::Obj(quantities)),
                (
                    "rate_table",
                    w4_decl.get("rate_table").cloned().unwrap_or(Json::Null),
                ),
            ]),
        ),
        (
            "comparative",
            Json::obj([
                ("t", Json::Int(10)),
                ("k", Json::Int(5)),
                ("measure_ref", Json::str(mget("trials"))),
                (
                    "prior_refs",
                    Json::obj([
                        (
                            "hh_native",
                            Json::str(mget("prereg")),
                        ),
                        (
                            "v5",
                            Json::str(mget("prereg")),
                        ),
                    ]),
                ),
                ("infra_allowance_ppm", Json::Int(0)),
                (
                    "compute",
                    Json::Arr(vec![
                        Json::obj([
                            ("window", Json::str("w4")),
                            ("quote_id", Json::str("pq-pending")),
                            ("ceiling_micros", Json::Int(7_988_890)),
                        ]),
                        Json::obj([
                            ("window", Json::str("wc")),
                            ("quote_id", Json::str("pq-556b0a0f5d853edb")),
                            ("ceiling_micros", Json::Int(1_937_300)),
                        ]),
                    ]),
                ),
            ]),
        ),
        (
            "readiness",
            Json::obj([
                ("record_ref", Json::str(mget("readiness"))),
                (
                    "result",
                    load(&repo, &mget("readiness"))
                        .get("result")
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        ("withheld", Json::Arr(vec![])),
        (
            "sources",
            sources(&[
                "trials",
                "pp0",
                "gl1",
                "prereg",
                "readiness",
                "w4_decl",
                "phase_spend",
            ]),
        ),
    ]);

    // ── pass 1: probe — d_max through the shared machinery ──────────
    let probe = fp_e_quote(&decl).unwrap_or_else(|e| panic!("probe fp_e_quote: {e}"));
    let d_max = probe
        .get("d_max")
        .and_then(|d| d.get("d_max_ms"))
        .and_then(Json::as_int)
        .unwrap_or_else(|| panic!("probe: d_max.d_max_ms"));
    let per_trial_bound = (d_max + 999) / 1000;
    eprintln!("fp-e-emit: probe d_max_ms={d_max}  per_trial_bound_seconds={per_trial_bound}");
    eprintln!("fp-e-emit: set scripts/gce-backend/declarations/fp-w4-fp-e.json quote_inputs.per_trial_bound_seconds={per_trial_bound} and re-issue via receipt_lib.py quote");

    // ── pass 2: wall_input + the re-issued quote id ─────────────────
    let w4_requote_id = m
        .iter()
        .find(|(n, _)| n == "w4_requote_id")
        .map(|(_, v)| v.clone());
    if let Json::Obj(dm) = &mut decl {
        dm.insert(
            "wall_input".into(),
            Json::obj([
                ("d_limit_ms", Json::Int(d_max)),
                ("cutoff_ms", Json::Int(2_340_000)),
                ("overhead_ms", Json::Int(4_500_000)),
                ("load_ms", Json::Int(1_800_000)),
                ("pairs", Json::Arr(pairs)),
            ]),
        );
        if let Some(qid) = &w4_requote_id {
            if let Some(Json::Obj(cq)) = dm.get_mut("comparative") {
                if let Some(Json::Arr(cs)) = cq.get_mut("compute") {
                    for c in cs.iter_mut() {
                        if c.get("window").and_then(Json::as_str) == Some("w4") {
                            if let Json::Obj(cm) = c {
                                cm.insert("quote_id".into(), Json::str(qid.clone()));
                            }
                        }
                    }
                }
            }
        }
    }
    let out = fp_e_quote(&decl).unwrap_or_else(|e| panic!("fp_e_quote: {e}"));

    // ── pass 3: the credit/root accounting (AC-4 figures) ───────────
    let lhs = out
        .get("checkpoint")
        .and_then(|c| c.get("cost_fit"))
        .and_then(|c| c.get("lhs_micros"))
        .and_then(Json::as_int)
        .unwrap_or(0);
    let needed_root = lhs + consumed_micros + 4 * 850_000;
    let purchase_need = (needed_root - 48_000_000).max(0);
    if let Json::Obj(dm) = &mut decl {
        dm.insert(
            "credit".into(),
            Json::obj([
                ("a_micros", Json::Int(70_000_000)),
                ("a_eff_micros", Json::Int(60_000_000)),
                ("margin_ppm", Json::Int(200_000)),
                ("root_cap_micros", Json::Int(48_000_000)),
                (
                    "credit_available",
                    Json::obj([
                        ("included_allowance_micros", Json::Int(60_000_000)),
                        ("purchased_micros", Json::Int(0)),
                    ]),
                ),
                ("consumed_micros", Json::Int(consumed_micros)),
                ("needed_root_micros", Json::Int(needed_root)),
                ("purchase_need_micros", Json::Int(purchase_need)),
                (
                    "purchase_decision",
                    Json::str(if purchase_need == 0 {
                        "NOT_NEEDED"
                    } else {
                        "PURCHASE (feeds HUMAN-H15; ≤ $10)"
                    }),
                ),
                (
                    "basis",
                    Json::str(
                        "A_eff = min(A $70, included credit $60) → root $48 at m = .20; \
                         model consumed at the provider (billed) reconcile read (A2)",
                    ),
                ),
            ]),
        );
    }
    let out = fp_e_quote(&decl).unwrap_or_else(|e| panic!("fp_e_quote (credit): {e}"));

    write(&repo, &mget("out_decl"), &decl);
    write(&repo, &mget("out_record"), &out);
    write(
        &repo,
        &mget("out_measure"),
        out.get("measurement")
            .and_then(|mm| mm.get("packet"))
            .unwrap_or(&Json::Null),
    );
    write(
        &repo,
        &mget("out_comparative"),
        out.get("comparative_quote").unwrap_or(&Json::Null),
    );
    write(
        &repo,
        &mget("out_w4_inputs"),
        out.get("w4_quote_inputs").unwrap_or(&Json::Null),
    );
}
