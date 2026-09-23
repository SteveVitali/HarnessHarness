//! `hh-helper` — the helper binary's entrypoint (S2.1; §5d.5 §4).
//!
//! ```text
//! hh-helper --socket <path> [--backend seatbelt|podman|direct]
//!           [--container <name>] [--image <ref>] [--workspace <dir>]
//! ```
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

use hh_helper::podman::PodmanBackend;
use hh_helper::server::{serve, HelperServer};

fn usage() -> ! {
    eprintln!(
        "usage: hh-helper --socket <path> [--backend seatbelt|podman|direct] \
         [--container <name>] [--image <ref>] [--workspace <dir>]"
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
            _ => usage(),
        }
    }
    let socket = socket.unwrap_or_else(|| usage());
    let srv = Arc::new(HelperServer::new());
    match backend.as_str() {
        "seatbelt" => {
            if !hh_helper::seatbelt::is_available() {
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
            if !hh_helper::podman::is_available() {
                eprintln!("hh-helper: backend podman unavailable (no reachable podman)");
                exit(3);
            }
            let b = PodmanBackend {
                container: name,
                image,
                workspace: ws,
                network: false,
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
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let srv2 = Arc::clone(&srv);
                let was_terminated = *srv2.shared.on_kernel_loss_terminate.lock().unwrap();
                if serve(stream, &srv2).is_err() {
                    break;
                }
                if was_terminated {
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
