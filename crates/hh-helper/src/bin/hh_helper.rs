//! `hh-helper` — the helper binary's entrypoint (S2.1; §5d.5 §4).
//!
//! ```text
//! hh-helper --socket <path> [--backend seatbelt|podman|direct]
//!           [--container <name>] [--image <ref>] [--workspace <dir>]
//!           [--network none|bridge]
//! ```
//!
//! `--network` (default `none`, `S4_HELPER_NETWORK` env fallback — the
//! flag wins when both are present) is the admitted `declared_deps`
//! egress knob (R5.56; ADR-0421): `bridge` runs the podman cell on
//! `--network bridge`; `none` is byte-identical to the pre-variant
//! posture. An unknown value is a usage failure, never coerced.
//!
//! The kernel spawns this binary once per contained environment; it binds
//! the session socket and serves `hh-helper/1` until `shutdown` or the
//! kernel-loss posture ends the session. With `preserve_until` (C1) the
//! process keeps the exec table across a dropped channel — the accept loop
//! re-serves `hello{resume_session_id}` on the same socket path.
//!
//! Exit codes: `0` clean shutdown; `2` usage/bind failure; `3` the backend
//! declared unavailable (fail-closed — the kernel records the verbatim
//! stderr, never invents containment).

use std::os::unix::net::UnixListener;
use std::process::exit;
use std::sync::Arc;

use hharness_helper::podman::PodmanBackend;
use hharness_helper::server::{serve, HelperServer};

fn usage() -> ! {
    eprintln!(
        "usage: hh-helper --socket <path> [--backend seatbelt|podman|direct] \
         [--container <name>] [--image <ref>] [--workspace <dir>] \
         [--network none|bridge]"
    );
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = None::<String>;
    let mut backend = "direct".to_string();
    let mut container = None::<String>;
    let mut image = PodmanBackend::DEFAULT_IMAGE.to_string();
    let mut workspace = None::<String>;
    let mut network = "none".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                socket = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--backend" => {
                backend = args.get(i + 1).cloned().unwrap_or_else(|| usage());
                i += 2;
            }
            "--container" => {
                container = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--image" => {
                image = args.get(i + 1).cloned().unwrap_or_else(|| usage());
                i += 2;
            }
            "--workspace" => {
                workspace = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--network" => {
                network = args.get(i + 1).cloned().unwrap_or_else(|| usage());
                i += 2;
            }
            _ => usage(),
        }
    }
    // Env fallback — hh-env's HelperClient::spawn passes a fixed argv;
    // the helper child inherits the driver process env, so the scoped
    // `declared_deps` admission may also travel as `S4_HELPER_NETWORK`.
    // An explicit flag wins (R5.56; ADR-0421).
    let network = if args.iter().any(|a| a == "--network") {
        network
    } else {
        std::env::var("S4_HELPER_NETWORK").unwrap_or(network)
    };
    let network_on = match network.as_str() {
        "none" => false,
        "bridge" => true,
        _ => usage(),
    };
    let socket = socket.unwrap_or_else(|| usage());
    let srv = Arc::new(HelperServer::new());
    match backend.as_str() {
        "seatbelt" => {
            if !hharness_helper::seatbelt::is_available() {
                eprintln!("hh-helper: backend seatbelt unavailable (no /usr/bin/sandbox-exec)");
                exit(3);
            }
            *srv.backend.lock().unwrap() = "seatbelt";
        }
        "podman" => {
            let (name, ws) = match (container, workspace) {
                (Some(n), Some(w)) => (n, w),
                _ => {
                    eprintln!("hh-helper: --backend podman needs --container and --workspace");
                    exit(2);
                }
            };
            if !hharness_helper::podman::is_available() {
                eprintln!("hh-helper: backend podman unavailable (no reachable podman)");
                exit(3);
            }
            let b = PodmanBackend {
                container: name,
                image,
                workspace: ws,
                network: network_on,
                extra_args: vec![],
            };
            if let Err(e) = b.create() {
                eprintln!("hh-helper: podman create failed: {e}");
                exit(3);
            }
            if let Err(e) = b.start() {
                let _ = b.stop();
                eprintln!("hh-helper: podman start failed: {e}");
                exit(3);
            }
            *srv.container.lock().unwrap() = Some(b);
            *srv.backend.lock().unwrap() = "container";
        }
        "direct" => {}
        _ => usage(),
    }
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("hh-helper: bind {socket}: {e}");
            exit(2);
        }
    };
    // Accept loop — a dropped channel returns here; `preserve_until`
    // sessions resume on the next accept (`terminate` sessions exit).
    // The flag is read *after* `serve` — the session that just ended
    // decides; reading it before would apply the stale (default-true)
    // posture and kill the resume path outright (R5.44's
    // `probe_verb_enoent` repair surfaced this on the reconnect leg).
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let srv2 = Arc::clone(&srv);
                if serve(stream, &srv2).is_err() {
                    break;
                }
                if *srv2.shared.on_kernel_loss_terminate.lock().unwrap() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    // Final teardown — the container goes with the helper.
    if let Some(b) = srv.container.lock().unwrap().take() {
        let _ = b.stop();
    }
    let _ = std::fs::remove_file(&socket);
}
