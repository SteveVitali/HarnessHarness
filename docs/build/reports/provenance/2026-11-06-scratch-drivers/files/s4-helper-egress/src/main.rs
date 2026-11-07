//! `s4-helper-egress` — the S4 egress-supplement helper variant.
//!
//! SCOPED ADMISSION (recorded verbatim in every egress-enabled slot
//! record, `egress_profile: declared_deps`): this binary is
//! `crates/hh-helper/src/bin/hh_helper.rs` verbatim — the same
//! `hharness_helper::server::serve` session loop, the same
//! `PodmanBackend` lifecycle, the same `--socket/--backend/--container/
//! --image/--workspace` argv — with ONE added flag:
//!
//!   --network <none|bridge>   (default: none — the stock posture)
//!   S4_HELPER_NETWORK env     (same vocabulary; the flag wins when both
//!                              are present — hh-env's spawn can't pass
//!                              extra argv, but the child inherits env)
//!
//! When the driver admits `egress_profile: declared_deps` for a slot,
//! it spawns this helper with the bridge mode selected, so the cell's
//! `podman create` carries `--network bridge` (outbound NAT through the
//! VM's slirp4netns) instead of `--network none`. Everything else —
//! pids limit, no-new-privileges, /work mount, --init, the journal,
//! dedup, deadlines — is the identical code path. The stock
//! `hh-helper` binary and crate source are untouched; this is a named
//! supplement variant, not a silent confinement bypass.

use std::os::unix::net::UnixListener;
use std::process::exit;
use std::sync::Arc;

use hharness_helper::podman::PodmanBackend;
use hharness_helper::server::{serve, HelperServer};

fn usage() -> ! {
    eprintln!(
        "usage: s4-helper-egress --socket <path> [--backend seatbelt|podman|direct] \
         [--container <name>] [--image <ref>] [--workspace <dir>] [--network none|bridge]"
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
    let socket = socket.unwrap_or_else(|| usage());
    // Env fallback — hh-env's HelperClient::spawn passes a fixed argv;
    // the helper child inherits the driver process env, so the scoped
    // admission travels as S4_HELPER_NETWORK. An explicit flag wins.
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
    let srv = Arc::new(HelperServer::new());
    match backend.as_str() {
        "seatbelt" => {
            if !hharness_helper::seatbelt::is_available() {
                eprintln!("s4-helper-egress: backend seatbelt unavailable (no /usr/bin/sandbox-exec)");
                exit(3);
            }
            *srv.backend.lock().unwrap() = "seatbelt";
        }
        "podman" => {
            let (name, ws) = match (container, workspace) {
                (Some(n), Some(w)) => (n, w),
                _ => {
                    eprintln!("s4-helper-egress: --backend podman needs --container and --workspace");
                    exit(2);
                }
            };
            if !hharness_helper::podman::is_available() {
                eprintln!("s4-helper-egress: backend podman unavailable (no reachable podman)");
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
                eprintln!("s4-helper-egress: podman create failed: {e}");
                exit(3);
            }
            if let Err(e) = b.start() {
                let _ = b.stop();
                eprintln!("s4-helper-egress: podman start failed: {e}");
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
            eprintln!("s4-helper-egress: bind {socket}: {e}");
            exit(2);
        }
    };
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
    if let Some(b) = srv.container.lock().unwrap().take() {
        let _ = b.stop();
    }
    let _ = std::fs::remove_file(&socket);
}
