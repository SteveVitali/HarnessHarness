//! S2 boundary-crossing spike — the S0.1 first measurement of AC-R-2.11.4-9.
//!
//! Drives `hh-kernel serve` over stdio (binding (b)) with the *generated* client and measures
//! per-crossing overhead of the `hello` round trip, plus the codegen round-trip wall time
//! (§10.6 M-S2-1 / M-S2-3). Durable-frame delivery latency (p50/p95) and ephemeral-drop rate
//! are `n/a` at Stage 0 — there is no event stream yet (Groups R land at Stage 1); the S0.1
//! DEFERRALS row tracks completing them.
//!
//! Args: <kernel-bin> <codegen-bin>. Env: SPIKE_N (crossings, default 200).
//! Output: `key value` lines consumed by `scripts/spike-s2-boundary.sh`.

use hh_embed_client_generated as client;
use std::io::BufReader;
use std::process::{Command, Stdio};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let kernel = args.next().unwrap_or_else(|| "target/release/hh-kernel".to_string());
    let codegen = args.next().unwrap_or_else(|| "target/release/hh-codegen".to_string());
    let n: usize = std::env::var("SPIKE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    // One persistent serve process; N hello crossings over the same stdio pipe.
    let mut child = Command::new(&kernel)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn kernel serve");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    // Warm up (exclude first-call/spawn effects from the crossing measurement).
    let warm = client::negotiate(&mut reader, &mut stdin, "spike", "0").expect("warmup hello");
    assert_eq!(warm.schema_hash, client::EXPECTED_SCHEMA_HASH);

    let mut hash_ok = 0usize;
    let mut samples = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        let ci = client::negotiate(&mut reader, &mut stdin, "spike", "0").expect("hello crossing");
        samples.push(t.elapsed().as_micros() as u64);
        if ci.contract_major == client::CONTRACT_MAJOR && ci.schema_hash == client::EXPECTED_SCHEMA_HASH {
            hash_ok += 1;
        }
    }
    drop(stdin);
    let _ = child.wait();

    samples.sort_unstable();
    let pct = |p: usize| samples[((n * p) / 100).min(n - 1)];

    // Codegen round-trip wall time (M-S2-3): one full export → codegen against a temp root.
    let tmp = std::env::temp_dir().join(format!("hh-codegen-spike-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let t = Instant::now();
    let status = Command::new(&codegen)
        .arg("--root")
        .arg(&tmp)
        .stderr(Stdio::null())
        .status()
        .expect("run codegen");
    let codegen_ms = t.elapsed().as_millis();
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(status.success(), "codegen round trip failed");

    println!("crossings {n}");
    println!("per_crossing_overhead_us_min {}", samples[0]);
    println!("per_crossing_overhead_us_median {}", pct(50));
    println!("per_crossing_overhead_us_p95 {}", pct(95));
    println!("per_crossing_overhead_us_max {}", samples[n - 1]);
    println!("codegen_round_trip_ms {codegen_ms}");
    println!("hash_equality_pct {}", (hash_ok * 100) / n);
    // Stage-0: no event stream yet.
    println!("durable_frame_latency_p50_us n/a{{stage_0_no_stream}}");
    println!("durable_frame_latency_p95_us n/a{{stage_0_no_stream}}");
    println!("ephemeral_drop_rate n/a{{stage_0_no_stream}}");
}
