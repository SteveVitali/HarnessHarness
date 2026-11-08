//! R6.27 — the `comparative-battery/1` executor (offline D2 tranche —
//! same cited envelope as `r5_40_battery`). Runs the frozen scenario
//! table over `std::thread` workers and writes the retained
//! integer-canonical record.
//!
//!   cargo run -p hharness-eval --release --example r6_27_battery -- <out.json> [threads] [deadline_ms]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use hharness_eval::comparative_battery::{
    cb_rollup, cb_scenarios, run_cb_scenario, CBatteryReport, CScenarioReport,
};
use hharness_eval::sim_battery::Cap;

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: r6_27_battery <out.json> [threads] [deadline_ms]");
    let threads = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
        .min(12);
    let budget_ms: Option<u64> = std::env::args().nth(3).and_then(|s| s.parse().ok());
    let start = Instant::now();
    let cap = Cap {
        max_datasets: u64::MAX,
        deadline: budget_ms.map(|ms| start + std::time::Duration::from_millis(ms)),
    };
    let cells = cb_scenarios();
    let next = AtomicUsize::new(0);
    let reports: Vec<CScenarioReport> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..threads {
            handles.push(scope.spawn(|| {
                let mut mine = Vec::new();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= cells.len() {
                        break;
                    }
                    mine.push((i, run_cb_scenario(&cells[i], &cap)));
                }
                mine
            }));
        }
        let mut all: Vec<(usize, CScenarioReport)> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker panicked"))
            .collect();
        all.sort_by_key(|(i, _)| *i);
        all.into_iter().map(|(_, r)| r).collect()
    });
    let report: CBatteryReport = cb_rollup(reports);
    let wall = start.elapsed();
    let json = report.to_json().to_canonical_string();
    std::fs::write(&out, format!("{json}\n")).expect("write record");
    let datasets: u64 = report.scenarios.iter().map(|s| s.datasets_run).sum();
    eprintln!(
        "rollup={} scenarios={} datasets={} wall={:?} threads={}",
        report.rollup,
        report.scenarios.len(),
        datasets,
        wall,
        threads
    );
    std::process::exit(if report.is_complete() { 0 } else { 1 });
}
