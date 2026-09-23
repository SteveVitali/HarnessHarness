//! S2 boundary-slice spike (throwaway; ADR-0050 R1–R6; §10.6 S2 slice; `l1-spike-spec.md` §4).
//!
//! Drives the persistent `hh-kernel serve` over stdio (binding (b), JSON-RPC 2.0) with the
//! *generated* client and reports the C12 measurements M-S2-1…7 for the **winning split's
//! crossing mechanism**. The split (`E5a`) is E1 kernel + E2 lab; offline/hermetic (operator
//! gate) the E2 lab is not runnable, so the crossing is measured **matched E1↔E1** (both arms E1
//! — CC9 matched-budget) which isolates the *transport* cost. The polyglot-specific components
//! (M-S2-5 two-toolchain CI; the cross-ecosystem serialization multiplier) need the E2 lab and
//! are DEFERRED (DF-S0.3-2). Args: `<kernel-bin> <codegen-bin>`. Env: `SPIKE_N` (crossings,
//! default 200). Output: `key value` lines consumed by `scripts/spike-s0.3-measurement-sheet.sh`.

use hh_embed_client_generated as client;
use hh_wire::json::Json;
use hh_wire::sha256_hex;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

const K_PER_RUN: usize = 4; // admissible per-run pattern (ADR-0045 §9): handle + cursor + metric + append
const N_RUNS: usize = 50;

struct Serve {
    child: Child,
}

impl Serve {
    fn spawn(kernel: &str) -> Serve {
        let child = Command::new(kernel)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn kernel serve");
        Serve { child }
    }
    fn pipes(
        &mut self,
    ) -> (
        BufReader<&mut std::process::ChildStdout>,
        &mut std::process::ChildStdin,
    ) {
        // Safety: stdin/stdout were set to piped at spawn.
        let stdout = self.child.stdout.as_mut().expect("stdout");
        let stdin = self.child.stdin.as_mut().expect("stdin");
        (BufReader::new(stdout), stdin)
    }
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The spike's `hello` capability descriptor — a minimal Stage-0 host (no channels served).
fn spike_caps() -> client::HostCapabilities {
    client::HostCapabilities {
        experimental: false,
        opt_out_notifications: vec![],
        serves_permission_channel: false,
        serves_host_executor: false,
        serves_hook_observer: false,
        serves_elicitation: false,
        serves_measurement: false,
        serves_principal_channel: false,
        accepts_ephemeral_frames: true,
        max_in_flight_sessions: None,
        extensions: std::collections::BTreeMap::new(),
    }
}

/// A well-formed `hello` request for this contract (asserts `(contract_major, schema_hash)`).
fn hello_params() -> client::HelloParams {
    client::HelloParams {
        contract_major: client::CONTRACT_MAJOR,
        client: client::ClientDescriptor {
            name: "spike".into(),
            version: "0".into(),
            kind: client::ClientKind::Test,
        },
        capabilities: spike_caps(),
        schema_hash: Some(client::EXPECTED_SCHEMA_HASH.to_string()),
        kernel_floor: None,
    }
}

/// One `hello` crossing over the persistent serve pipe using the generated client. Borrows the
/// pipes for the duration of the request/response ping-pong, so it can be called repeatedly on
/// one connection. `hello` re-verifies `(contract_major, schema_hash)` and returns a typed
/// error on any mismatch — never a silent fallback.
fn negotiate<R: BufRead, W: Write>(
    reader: R,
    writer: W,
) -> Result<client::HelloResult, client::ClientError> {
    let mut c = client::Client::new(reader, writer);
    c.hello(&hello_params())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let kernel = args
        .next()
        .unwrap_or_else(|| "target/release/hh-kernel".to_string());
    let codegen = args
        .next()
        .unwrap_or_else(|| "target/release/hh-codegen".to_string());
    let n: usize = std::env::var("SPIKE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    // ---- M-S2-2: per-sample crossing overhead (the *disallowed* hot path) --------------------
    // Median round-trip of one crossing over the persistent serve process.
    let mut serve = Serve::spawn(&kernel);
    let (mut reader, mut stdin) = serve.pipes();
    let warm = negotiate(&mut reader, &mut stdin).expect("warmup hello");
    assert_eq!(warm.kernel.schema_hash, client::EXPECTED_SCHEMA_HASH);
    let mut hash_ok = 0usize;
    let mut samples = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        let ci = negotiate(&mut reader, &mut stdin).expect("hello crossing");
        samples.push(t.elapsed().as_micros());
        if ci.kernel.contract_major == client::CONTRACT_MAJOR
            && ci.kernel.schema_hash == client::EXPECTED_SCHEMA_HASH
        {
            hash_ok += 1; // M-S2-7: every far-side identity re-verified
        }
    }
    samples.sort_unstable();
    let pct = |p: usize| samples[((n * p) / 100).min(n - 1)];
    println!("m_s2_2_per_sample_overhead_us_min {}", samples[0]);
    println!("m_s2_2_per_sample_overhead_us_median {}", pct(50));
    println!("m_s2_2_per_sample_overhead_us_p95 {}", pct(95));
    println!("m_s2_2_per_sample_overhead_us_max {}", samples[n - 1]);

    // ---- M-S2-1: per-run crossing overhead — split (over stdio) vs native (in-process) --------
    // Matched E1↔E1: the split arm drives K_PER_RUN crossings/run over the pipe; the native arm
    // does the identical serialize+parse work in-process (no subprocess). Overhead = the pure
    // transport/subprocess crossing cost. Transport component only (polyglot penalty deferred).
    let total_crossings = K_PER_RUN * N_RUNS;
    let t = Instant::now();
    for _ in 0..total_crossings {
        let _ = negotiate(&mut reader, &mut stdin).expect("split crossing");
    }
    let split_us = t.elapsed().as_micros();
    drop(reader);
    let native_us = native_roundtrips(total_crossings);
    let overhead = split_us.saturating_sub(native_us);
    println!("m_s2_1_split_wall_us {split_us}");
    println!("m_s2_1_native_wall_us {native_us}");
    println!("m_s2_1_per_run_overhead_us {}", overhead / N_RUNS as u128);
    println!("m_s2_1_crossings_per_run {K_PER_RUN}");

    // ---- M-S2-3: serialization + validation per event on the far side (16 KiB, 64 KiB) --------
    println!(
        "m_s2_3_serialize_validate_16k_us {:.3}",
        serialize_validate_us(16 * 1024)
    );
    println!(
        "m_s2_3_serialize_validate_64k_us {:.3}",
        serialize_validate_us(64 * 1024)
    );

    // ---- M-S2-7: hash-equality of every far-side event (100 % required) -----------------------
    println!("m_s2_7_hash_equality_pct {}", (hash_ok * 100) / n);

    // Close the first serve process (stdin is a borrow; kill the throwaway peer to release it).
    let _ = stdin;
    serve.kill();

    // ---- M-S2-6: version-mismatch refusal + resume after a killed peer (pass/fail) ------------
    let refusal = version_mismatch_refused(&kernel);
    let resume = resume_after_killed_peer(&kernel);
    println!(
        "m_s2_6_version_refusal {}",
        if refusal { "pass" } else { "fail" }
    );
    println!(
        "m_s2_6_resume_after_kill {}",
        if resume { "pass" } else { "fail" }
    );
    println!("m_s2_6 {}", if refusal && resume { "pass" } else { "fail" });

    // ---- M-S2-4: codegen round-trip (export → codegen against a temp root) --------------------
    let tmp = std::env::temp_dir().join(format!("hh-codegen-s0.3-{}", std::process::id()));
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
    println!("m_s2_4_codegen_round_trip_ms {codegen_ms}");

    // ---- M-S2-5: two-toolchain CI — offline only the E1 toolchain is present → deferred -------
    println!("m_s2_5_two_toolchain_ci n/a{{offline_single_toolchain}}");

    // ---- DF-S0.1-1 durable-frame / ephemeral columns: measured by the S1 spike's M-S1-9 -------
    // (durable-frame delivery latency p50/p95 = the S1 subscribe tail lag; ephemeral-drop = 0).
    println!("durable_frame_note see_s1_m_s1_9_and_ephemeral_drop_0");
}

/// The native (in-process) arm: `count` full serialize→parse round-trips of the `hello`
/// request and a canned identity response, with NO subprocess. Both directions serialize +
/// parse, so `split − native` is the pure crossing (subprocess + pipe + scheduling) cost.
fn native_roundtrips(count: usize) -> u128 {
    let params = hello_params();
    let identity = client::ContractIdentity {
        contract_major: client::CONTRACT_MAJOR,
        schema_hash: client::EXPECTED_SCHEMA_HASH.to_string(),
        kernel_version_id: "0.0.1".into(),
    };
    let resp = Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(1)),
        (
            "result",
            Json::obj([("contract_identity", identity.to_json())]),
        ),
    ]);
    let t = Instant::now();
    let mut sink = 0usize;
    for _ in 0..count {
        // Client → kernel: build + canonicalize the request, parse it (far-side receive).
        let req = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(1)),
            ("method", Json::str("hello")),
            ("params", params.to_json()),
        ]);
        let req_line = req.to_canonical_string();
        let parsed_req = hh_wire::parse(&req_line).unwrap();
        sink += parsed_req
            .get("method")
            .and_then(Json::as_str)
            .map(str::len)
            .unwrap_or(0);
        // Kernel → client: canonicalize the response, parse it, re-verify identity.
        let resp_line = resp.to_canonical_string();
        let parsed = hh_wire::parse(&resp_line).unwrap();
        let ci = client::ContractIdentity::from_json(
            parsed
                .get("result")
                .and_then(|r| r.get("contract_identity"))
                .unwrap(),
        )
        .unwrap();
        sink += (ci.schema_hash == client::EXPECTED_SCHEMA_HASH) as usize;
    }
    std::hint::black_box(sink);
    t.elapsed().as_micros()
}

/// M-S2-3: cost (µs) of the far side receiving one ~`size`-byte event: canonical-form parse +
/// hash verify (re-canonicalize + SHA-256, the I4/CF-058 check). Median of repeats.
fn serialize_validate_us(size: usize) -> f64 {
    let body = "x".repeat(size);
    let event = Json::obj([
        ("kind", Json::str("work.step")),
        ("seq", Json::Int(1)),
        ("body", Json::str(body)),
    ]);
    let line = event.to_canonical_string();
    let reps = 1000u32;
    let t = Instant::now();
    let mut sink = 0usize;
    for _ in 0..reps {
        let parsed = hh_wire::parse(&line).unwrap();
        let recanon = parsed.to_canonical_string();
        let h = sha256_hex(recanon.as_bytes());
        sink += h.len();
    }
    std::hint::black_box(sink);
    t.elapsed().as_micros() as f64 / reps as f64
}

/// M-S2-6a: a `hello` asserting an unsupported `contract_major` is refused with a typed error
/// (the closed error sum; ADR-0178 D2), never a silent fallback.
fn version_mismatch_refused(kernel: &str) -> bool {
    let mut serve = Serve::spawn(kernel);
    let (mut reader, stdin) = serve.pipes();
    // Hello asserting a wrong major (well-formed params otherwise).
    let mut bad = hello_params();
    bad.contract_major = 42;
    let req = Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(1)),
        ("method", Json::str("hello")),
        ("params", bad.to_json()),
    ]);
    stdin
        .write_all(format!("{}\n", req.to_canonical_string()).as_bytes())
        .unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp = hh_wire::parse(line.trim()).unwrap();
    let refused = resp
        .get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        == Some("ContractMajorUnsupported");
    drop(reader);
    let _ = stdin;
    serve.kill();
    refused
}

/// M-S2-6b: after the peer is killed, a fresh peer re-handshakes successfully — the Stage-0 form
/// of `open_session(resume=…)` after a killed peer (there is no session state at Stage 0; the
/// full resume semantics land at Stage 1 — noted). The boundary recovers, never wedges.
fn resume_after_killed_peer(kernel: &str) -> bool {
    // First peer: hello ok, then kill it mid-connection.
    let mut serve = Serve::spawn(kernel);
    {
        let (mut reader, mut stdin) = serve.pipes();
        let ok = negotiate(&mut reader, &mut stdin).is_ok();
        if !ok {
            serve.kill();
            return false;
        }
    }
    serve.kill(); // peer dies

    // Fresh peer recovers the boundary: hello negotiates again.
    let mut serve2 = Serve::spawn(kernel);
    let recovered = {
        let (mut reader, mut stdin) = serve2.pipes();
        negotiate(&mut reader, &mut stdin).is_ok()
    };
    serve2.kill();
    recovered
}
