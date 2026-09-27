//! `hh-web` — the C1 read-only web instrument binary (spec §7.2).
//!
//! ```text
//! hh-web --kernel <addr> --kernel-token <t>|--kernel-token-file <f>
//!        [--bind 127.0.0.1:PORT] [--token <t>|--token-file <f>]
//!        [--trusted-origins <csv>] [--sink <json>] [--subject <ref>]
//! ```
//!
//! Token delivery (P9): the bound line + minted token print to **stderr
//! only** — never stdout (composable), never a ledger, never a URL.

use std::net::{SocketAddr, TcpListener};
use std::process::ExitCode;

use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

use hh_web::gate::{mint_token, GateConfig};
use hh_web::server;
use hh_web::session::Sessions;
use hh_web::sink::parse_policy;

fn usage() -> ! {
    eprintln!(
        "usage: hh-web --kernel <addr> (--kernel-token <t>|--kernel-token-file <f>) \
         [--bind 127.0.0.1:PORT] [--token <t>|--token-file <f>] \
         [--trusted-origins <csv>] [--sink <json>] [--subject <ref>]"
    );
    std::process::exit(2)
}

fn read_token(flag: &str, val: Option<&String>, file: Option<&String>) -> Option<String> {
    match (val, file) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(f)) => match std::fs::read_to_string(f) {
            Ok(s) => Some(s.trim().to_string()),
            Err(e) => {
                eprintln!("hh-web: {flag}-file {f}: {e}");
                std::process::exit(2)
            }
        },
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut kernel_addr: Option<String> = None;
    let mut kernel_token: Option<String> = None;
    let mut kernel_token_file: Option<String> = None;
    let mut bind = "127.0.0.1:0".to_string();
    let mut token: Option<String> = None;
    let mut token_file: Option<String> = None;
    let mut trusted: Vec<String> = Vec::new();
    let mut sink_json: Option<String> = None;
    let mut subject = "human:principal".to_string();
    let mut allow_missing = false;
    let mut canaries: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kernel" => {
                kernel_addr = args.get(i + 1).cloned();
                i += 2;
            }
            "--kernel-token" => {
                kernel_token = args.get(i + 1).cloned();
                i += 2;
            }
            "--kernel-token-file" => {
                kernel_token_file = args.get(i + 1).cloned();
                i += 2;
            }
            "--bind" => {
                bind = args.get(i + 1).cloned().unwrap_or_else(|| usage());
                i += 2;
            }
            "--token" => {
                token = args.get(i + 1).cloned();
                i += 2;
            }
            "--token-file" => {
                token_file = args.get(i + 1).cloned();
                i += 2;
            }
            "--trusted-origins" => {
                trusted = args
                    .get(i + 1)
                    .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                    .unwrap_or_default();
                i += 2;
            }
            "--sink" => {
                sink_json = args.get(i + 1).cloned();
                i += 2;
            }
            "--subject" => {
                subject = args.get(i + 1).cloned().unwrap_or_else(|| usage());
                i += 2;
            }
            "--canary" => {
                // `<id>:<value>` — a planted canary for the serving scrub
                // (AC-6(g): the LT fixtures' values must never leave the
                // surface; the detector set masks them verbatim).
                canaries.push(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--allow-missing-fetch-metadata" => {
                // Test/CLI-driver mode — browser-facing runs keep the
                // fetch-metadata legs (P4).
                allow_missing = true;
                i += 1;
            }
            _ => usage(),
        }
    }
    let kernel_addr = kernel_addr.unwrap_or_else(|| usage());
    let ktok = read_token(
        "--kernel-token",
        kernel_token.as_ref(),
        kernel_token_file.as_ref(),
    )
    .unwrap_or_else(|| usage());
    let stok = match read_token("--token", token.as_ref(), token_file.as_ref()) {
        Some(t) => t,
        None => match mint_token() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("hh-web: mint token: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    let addr: SocketAddr = match bind.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hh-web: --bind {bind}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let loopback = addr.ip().is_loopback();
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("hh-web: bind {addr}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let bound = listener.local_addr().unwrap_or(addr);
    let gate = GateConfig {
        authority: bound.to_string(),
        loopback,
        token: stok.clone(),
        trusted_origins: trusted,
        allow_missing_fetch_metadata: allow_missing,
    };
    if let Err(e) = gate.validate() {
        eprintln!("hh-web: {e}");
        return ExitCode::FAILURE;
    }
    // The declared SinkPolicy — `{accounting}+content` is the surface's
    // default (a local display *is* the consent context; the policy's
    // `requires_consent` is asserted by the launcher for content).
    let sink = match &sink_json {
        Some(s) => match hh_wire::json::parse(s)
            .ok()
            .and_then(|j: Json| parse_policy(&j).ok())
        {
            Some(p) => p,
            None => {
                eprintln!("hh-web: --sink is not a valid SinkPolicy");
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut p = SinkPolicy::accounting("hh-web");
            p.content_classes
                .insert(hh_telemetry::sinks::ContentClass::Structural);
            p.content_classes
                .insert(hh_telemetry::sinks::ContentClass::Content);
            p.requires_consent = true;
            p
        }
    };
    let kaddr: SocketAddr = match kernel_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hh-web: --kernel {kernel_addr}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let client = server::connect_kernel(kaddr, &ktok);
    let svc = Sessions::new(client, sink, subject);
    // The serving scrub — registered patterns + declared canaries (the
    // mask set is empty: live credentials are the kernel broker's; the
    // surface's sweep is defence in depth, AC-6(g)).
    let mut detectors = hh_secrets::DetectorSet::standard(hh_secrets::MaskSet::default());
    for c in &canaries {
        match c.split_once(':') {
            Some((id, value)) if !value.is_empty() => detectors.canaries.push(hh_secrets::Canary {
                id: id.into(),
                value: value.into(),
            }),
            _ => {
                eprintln!("hh-web: --canary wants <id>:<value>");
                return ExitCode::FAILURE;
            }
        }
    }
    // Token delivery: stderr only (P9 — never stdout, never a URL).
    eprintln!("hh-web bound=http://{bound} token={stok}");
    match server::serve(listener, gate, svc, detectors) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hh-web: serve: {e}");
            ExitCode::FAILURE
        }
    }
}
