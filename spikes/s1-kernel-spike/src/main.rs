//! S1 kernel-slice spike runner (throwaway; ADR-0050 R1–R6). Runs the offline/hermetic subset of
//! the l1-spike-spec §3 workload for the **E1** candidate and prints `key value` lines for
//! M-S1-1…9. The correctness gates G1–G4 are the executable unit tests in `lib.rs` (run by the
//! shell runner via `cargo test`). Args: `[helper-bin]`; subcommand `startup-probe <root>` is the
//! spawned child used to time process-start → first-visible (M-S1-1).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use s1_kernel_spike as spike;
use spike::{bare_command, pct, run_session, HelperConn, Ledger};

const N_LARGE: usize = 50; // l1-spike-spec §3.3 fan-out point
const EVENTS_PER_SESSION: usize = 200;
const BATCH: usize = 10; // atomic multi-event append granularity

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("startup-probe") {
        startup_probe(Path::new(args.get(2).map(String::as_str).unwrap_or(".")));
        return;
    }
    let helper_bin = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "target/release/s1-helper".to_string());
    let self_bin = std::env::current_exe().expect("current exe");

    let root = std::env::temp_dir().join(format!("s1-spike-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    println!("cores {cores}");
    println!("n_large {N_LARGE}");
    println!("events_per_session {EVENTS_PER_SESSION}");

    m_s1_1_startup(&self_bin, &root);
    let (wall_n1, wall_n50, busy_n50) = m_s1_5_fanout(&root);
    m_s1_3_throughput(wall_n50);
    m_s1_5_report(wall_n1, wall_n50, busy_n50, cores);
    m_s1_5_cpu_fanout(cores);
    m_s1_2_rss(&root);
    m_s1_4_project(&root);
    m_s1_6_helper(Path::new(&helper_bin), &root);
    m_s1_8_cancel(&root);
    m_s1_9_tail_lag(&root);
    // M-S1-7: MCP call + ACP session — needs the candidate's official MCP/ACP SDKs over the
    // network and local reference peers; not runnable offline/hermetic (operator gate). Deferred.
    println!("m_s1_7_mcp_acp n/a{{offline_no_official_sdk}}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Child process: open a ledger, append one event, make it visible, print READY. Times nothing
/// itself — the parent times spawn → READY (M-S1-1).
fn startup_probe(root: &Path) {
    let dir = root.join(format!("startup-{}", std::process::id()));
    let mut ledger = Ledger::open(&dir, "startup", 1).expect("open");
    let w = ledger.writer();
    ledger
        .append_batch(w, &spike::session_corpus("startup")[0..1])
        .expect("append");
    assert_eq!(ledger.visible().len(), 1);
    println!("READY");
    let _ = std::fs::remove_dir_all(&dir);
}

/// M-S1-1 — process start to first ledger event visible (ms), median of repeated spawns.
fn m_s1_1_startup(self_bin: &Path, root: &Path) {
    let mut samples = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let out = std::process::Command::new(self_bin)
            .arg("startup-probe")
            .arg(root)
            .output()
            .expect("spawn startup-probe");
        let elapsed = t.elapsed();
        assert!(String::from_utf8_lossy(&out.stdout).contains("READY"));
        samples.push(elapsed.as_micros());
    }
    samples.sort_unstable();
    let median_us = pct(&samples, 50);
    println!("m_s1_1_startup_ms {:.3}", median_us as f64 / 1000.0);
}

/// M-S1-5 — fan-out cost: wall time N=50 vs N=1. Returns `(wall_n1_us, wall_n50_us, busy_n50_us)`
/// where `busy_n50_us` is the summed per-session durations at N=50 (achieved-parallelism input).
fn m_s1_5_fanout(root: &Path) -> (u128, u128, u128) {
    // N = 1
    let t = Instant::now();
    run_session(&root.join("n1"), "s-n1", BATCH).unwrap();
    let wall_n1 = t.elapsed().as_micros();

    // N = 50 concurrent, each session on its own thread; record each session's own duration.
    let root50 = Arc::new(root.join("n50"));
    let t = Instant::now();
    let handles: Vec<_> = (0..N_LARGE)
        .map(|i| {
            let r = Arc::clone(&root50);
            std::thread::spawn(move || {
                let ts = Instant::now();
                run_session(&r, &format!("s-{i}"), BATCH).unwrap();
                ts.elapsed().as_micros()
            })
        })
        .collect();
    let mut busy = 0u128;
    for h in handles {
        busy += h.join().unwrap();
    }
    let wall_n50 = t.elapsed().as_micros();
    (wall_n1, wall_n50, busy)
}

/// M-S1-3 — sustained append throughput at N=50 (events/s aggregate), from the fan-out run's wall.
fn m_s1_3_throughput(wall_n50_us: u128) {
    let total = (N_LARGE * EVENTS_PER_SESSION) as f64;
    let eps = total / (wall_n50_us as f64 / 1_000_000.0);
    println!("m_s1_3_append_throughput_eps {:.0}", eps);
}

fn m_s1_5_report(wall_n1: u128, wall_n50: u128, busy_n50: u128, _cores: usize) {
    // DURABLE fan-out (durable-before-visible, per-session WAL, fsync per batch). This is
    // storage-fsync-serialization-bound on the recorded host, NOT the ecosystem concurrency
    // mechanism — reported for completeness with that caveat (see the sheet's scope note).
    let ratio_wall = wall_n50 as f64 / wall_n1.max(1) as f64;
    let achieved = busy_n50 as f64 / wall_n50.max(1) as f64;
    println!("m_s1_5_durable_fanout_ratio_wall {:.3}", ratio_wall);
    println!("m_s1_5_achieved_parallelism {:.2}", achieved);
    println!("m_s1_5_wall_n1_us {wall_n1}");
    println!("m_s1_5_wall_n50_us {wall_n50}");
}

/// M-S1-5 (C5 input) — CPU/concurrency fan-out: the in-memory chain build (canonical form + hash
/// chain + fold, no fsync) at N=50 vs N=1, isolating the ecosystem's concurrency mechanism from
/// storage. Core-normalized (`ratio × cores/50`) is the "ideal 1.0 if perfectly parallel on ≥50
/// cores" figure the band assumes.
fn m_s1_5_cpu_fanout(cores: usize) {
    let run = |n: usize| -> u128 {
        let t = Instant::now();
        let handles: Vec<_> = (0..n)
            .map(|i| {
                std::thread::spawn(move || {
                    s1_kernel_spike::build_chain_in_memory(&format!("cpu-{i}"), || false)
                })
            })
            .collect();
        for h in handles {
            std::hint::black_box(h.join().unwrap());
        }
        t.elapsed().as_micros()
    };
    // Warm caches, then measure.
    let _ = run(1);
    let cpu_n1 = run(1);
    let cpu_n50 = run(N_LARGE);
    let ratio = cpu_n50 as f64 / cpu_n1.max(1) as f64;
    let s_over_p = N_LARGE as f64 / cores.min(N_LARGE) as f64;
    let core_normalized = ratio / s_over_p;
    println!("m_s1_5_cpu_fanout_ratio_wall {:.3}", ratio);
    println!("m_s1_5_cpu_fanout_core_normalized {:.3}", core_normalized);
}

/// M-S1-2 — resident memory per idle session at N=50 (total RSS / 50) MiB, after a short idle.
/// Idle window is 2 s here, not the spec's 30 s (a throwaway-spike footprint proxy — recorded).
fn m_s1_2_rss(root: &Path) {
    let park = Arc::new(AtomicBool::new(true));
    let root2 = Arc::new(root.join("rss"));
    let handles: Vec<_> = (0..N_LARGE)
        .map(|i| {
            let r = Arc::clone(&root2);
            let p = Arc::clone(&park);
            std::thread::spawn(move || {
                let ledger = run_session(&r, &format!("r-{i}"), BATCH).unwrap();
                while p.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Keep the ledger (and its visible events) resident until released.
                let _ = ledger.visible().len();
            })
        })
        .collect();
    std::thread::sleep(Duration::from_secs(2)); // idle window
    let rss_kib = read_rss_kib();
    park.store(false, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }
    let per_session_mib = (rss_kib as f64 / 1024.0) / N_LARGE as f64;
    println!("m_s1_2_rss_total_mib {:.1}", rss_kib as f64 / 1024.0);
    println!("m_s1_2_rss_per_session_mib {:.3}", per_session_mib);
}

/// Read this process's resident set size in KiB via `ps` (macOS/Linux report RSS in KiB).
fn read_rss_kib() -> u64 {
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output();
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// M-S1-4 — project() rebuild latency for 200 events + incremental update latency per event.
fn m_s1_4_project(root: &Path) {
    let ledger = run_session(&root.join("proj"), "proj", BATCH).unwrap();
    // Rebuild: fold all 200 events from scratch, repeated to get a stable per-rebuild number.
    let reps = 2000u32;
    let t = Instant::now();
    let mut sink = 0i64;
    for _ in 0..reps {
        let (total, _) = ledger.project_cost_totals(None);
        sink ^= total;
    }
    std::hint::black_box(sink);
    let rebuild_us = t.elapsed().as_micros() as f64 / reps as f64;
    println!("m_s1_4_project_rebuild_ms {:.4}", rebuild_us / 1000.0);
    // Incremental: cost of folding one more event (project up to last seq minus up to last-1).
    let last = ledger.visible().last().unwrap().seq;
    let t = Instant::now();
    let mut sink = 0i64;
    for _ in 0..reps {
        let (a, _) = ledger.project_cost_totals(Some(last));
        let (b, _) = ledger.project_cost_totals(Some(last - 1));
        sink ^= a - b;
    }
    std::hint::black_box(sink);
    let incr_us = t.elapsed().as_micros() as f64 / reps as f64;
    println!("m_s1_4_project_incremental_us {:.3}", incr_us);
}

/// M-S1-6 — sandboxed call overhead: helper round-trip minus the bare command time (median of 20).
fn m_s1_6_helper(helper_bin: &Path, root: &Path) {
    let cwd = root.to_string_lossy().to_string();
    let mut conn = HelperConn::spawn(helper_bin).expect("spawn helper");
    // Warm up (exclude first-call effects).
    let _ = conn.execute("true", &cwd).expect("warmup");
    let mut helper_us = Vec::new();
    for _ in 0..20 {
        let (reply, rt) = conn.execute("true", &cwd).expect("execute");
        assert_eq!(reply.exit_code, 0, "sandboxed `true` should exit 0");
        helper_us.push(rt);
    }
    let mut bare_us = Vec::new();
    for _ in 0..20 {
        bare_us.push(bare_command("true", &cwd).expect("bare"));
    }
    helper_us.sort_unstable();
    bare_us.sort_unstable();
    let overhead = pct(&helper_us, 50) as i128 - pct(&bare_us, 50) as i128;
    println!("m_s1_6_helper_roundtrip_us {}", pct(&helper_us, 50));
    println!("m_s1_6_bare_command_us {}", pct(&bare_us, 50));
    println!("m_s1_6_helper_overhead_ms {:.3}", overhead as f64 / 1000.0);
}

/// M-S1-8 — cancellation latency: `cancel` to all N=50 session tasks stopped (ms). Measures the
/// cancellation-propagation *mechanism*: each session runs the in-memory chain work checking the
/// cancel flag between events (a decision point), so the figure is the ecosystem's cancel
/// propagation, not the completion of an in-flight durable fsync under storage contention.
fn m_s1_8_cancel(_root: &Path) {
    let cancel = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicU64::new(0));
    let stop_us = Arc::new(std::sync::Mutex::new(Vec::<u128>::new()));
    let t0 = Instant::now();
    let handles: Vec<_> = (0..N_LARGE)
        .map(|i| {
            let c = Arc::clone(&cancel);
            let s = Arc::clone(&started);
            let m = Arc::clone(&stop_us);
            std::thread::spawn(move || {
                s.fetch_add(1, Ordering::SeqCst);
                // Loop the in-memory chain work indefinitely, polling the flag between events.
                loop {
                    let mut cancelled = false;
                    let _ = spike::build_chain_in_memory(&format!("c-{i}"), || {
                        if c.load(Ordering::Relaxed) {
                            cancelled = true;
                            true
                        } else {
                            false
                        }
                    });
                    if cancelled {
                        m.lock().unwrap().push(t0.elapsed().as_micros());
                        break;
                    }
                }
            })
        })
        .collect();
    while started.load(Ordering::SeqCst) < N_LARGE as u64 {
        std::thread::yield_now();
    }
    std::thread::sleep(Duration::from_millis(50));
    let cancel_issued = t0.elapsed().as_micros();
    cancel.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }
    let stops = stop_us.lock().unwrap();
    let last_stop = stops.iter().max().copied().unwrap_or(cancel_issued);
    let latency_us = last_stop.saturating_sub(cancel_issued);
    println!("m_s1_8_cancel_ms {:.3}", latency_us as f64 / 1000.0);
}

/// M-S1-9 — streaming tail lag: `subscribe` delivery latency at N=50 (median, p95) µs. A single
/// subscriber consumes visibility notifications from 50 producers over a channel; lag = delivery
/// instant − visible instant.
fn m_s1_9_tail_lag(root: &Path) {
    let (tx, rx) = mpsc::channel::<Instant>();
    let root9 = Arc::new(root.join("tail"));
    let producers: Vec<_> = (0..N_LARGE)
        .map(|i| {
            let tx = tx.clone();
            let r = Arc::clone(&root9);
            std::thread::spawn(move || {
                let dir = r.join(format!("t-{i}"));
                let mut ledger = Ledger::open(&dir, &format!("t-{i}"), 1).unwrap();
                let w = ledger.writer();
                let corpus = spike::session_corpus(&format!("t-{i}"));
                for chunk in corpus.chunks(BATCH) {
                    ledger.append_batch(w, chunk).unwrap();
                    // Event(s) now visible — publish the visibility instant to the subscriber.
                    tx.send(Instant::now()).unwrap();
                }
            })
        })
        .collect();
    drop(tx);
    let mut lags = Vec::new();
    for visible_at in rx {
        lags.push(Instant::now().duration_since(visible_at).as_micros());
    }
    for h in producers {
        h.join().unwrap();
    }
    lags.sort_unstable();
    println!("m_s1_9_tail_lag_median_us {}", pct(&lags, 50));
    println!("m_s1_9_tail_lag_p95_us {}", pct(&lags, 95));
}

// Silence unused warnings for the helper path constant on platforms where main branches early.
#[allow(dead_code)]
fn _unused(_p: PathBuf) {}
