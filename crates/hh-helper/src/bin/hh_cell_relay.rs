//! `hh-cell-relay` — the in-cell byte relay (R6.15; AR-05; REQ-XB-129;
//! experiments-spec §3.5). One statically linked binary, the cell's main
//! process under `--init`: it binds each declared loopback port and
//! bridges bytes to the bind-mounted unix socket of the closed pair
//! `{model, egress}`:
//!
//! ```text
//! hh-cell-relay --pair <model|egress>=<port>=<sock_path> [--pair …]
//! ```
//!
//! - `name ∈ {model, egress}` — the closed pair set; anything else is a
//!   usage failure (exit 2), never coerced;
//! - exactly one pair per name;
//! - each pair binds `127.0.0.1:<port>` (a literal address — no NSS, the
//!   cell has no resolver) and pipes each accepted connection to a
//!   `UnixStream` at `<sock_path>` until close;
//! - the binary is std-only so a `x86_64-unknown-linux-musl` build is
//!   statically linked and runs in any task image as PID 1.
//!
//! Death semantics (REQ-XB-129): the relay is the cell's main process —
//! its death stops the container. A death after an agent-issued signal
//! to PID 1 classifies `executed{…, end_cause: crashed}` flagged
//! `relay_killed_by_agent`; any other relay death is `infra:relay_died`.

use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::process::exit;
use std::thread;

fn usage() -> ! {
    eprintln!("usage: hh-cell-relay --pair <model|egress>=<port>=<sock_path> [--pair …]");
    exit(2);
}

struct Pair {
    name: String,
    port: u16,
    sock: String,
}

fn parse_args() -> Vec<Pair> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] != "--pair" {
            usage();
        }
        let v = args.get(i + 1).unwrap_or_else(|| usage());
        let (name, rest) = v.split_once('=').unwrap_or_else(|| usage());
        if !matches!(name, "model" | "egress") {
            usage();
        }
        let (port_s, sock) = rest.split_once('=').unwrap_or_else(|| usage());
        let port: u16 = port_s.parse().unwrap_or_else(|_| usage());
        if port == 0 || sock.is_empty() {
            usage();
        }
        if pairs.iter().any(|p: &Pair| p.name == name) {
            usage();
        }
        pairs.push(Pair {
            name: name.to_string(),
            port,
            sock: sock.to_string(),
        });
        i += 2;
    }
    if pairs.is_empty() {
        usage();
    }
    pairs
}

fn pipe(mut a: TcpStream, mut b: UnixStream) {
    let mut a2 = a.try_clone().expect("relay: clone downstream");
    let mut b2 = b.try_clone().expect("relay: clone upstream");
    let t = thread::spawn(move || {
        let _ = std::io::copy(&mut a2, &mut b2);
        let _ = b2.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut b, &mut a);
    let _ = a.shutdown(Shutdown::Write);
    let _ = t.join();
}

fn serve(pair: Pair) {
    let listener = match TcpListener::bind(("127.0.0.1", pair.port)) {
        Ok(l) => l,
        Err(e) => {
            // A bind failure is fatal — the cell without its bridge is
            // an infrastructure failure, never a degraded relay.
            eprintln!("hh-cell-relay: bind 127.0.0.1:{}: {e}", pair.port);
            exit(1);
        }
    };
    eprintln!(
        "hh-cell-relay: {} 127.0.0.1:{} <-> {}",
        pair.name, pair.port, pair.sock
    );
    for conn in listener.incoming() {
        match conn {
            Ok(downstream) => {
                let sock = pair.sock.clone();
                thread::spawn(move || match UnixStream::connect(&sock) {
                    Ok(upstream) => pipe(downstream, upstream),
                    Err(e) => {
                        eprintln!("hh-cell-relay: connect {sock}: {e}");
                        let _ = downstream.shutdown(Shutdown::Both);
                    }
                });
            }
            Err(e) => eprintln!("hh-cell-relay: accept: {e}"),
        }
    }
}

fn main() {
    let pairs = parse_args();
    let mut joins = Vec::new();
    for p in pairs {
        joins.push(thread::spawn(move || serve(p)));
    }
    for j in joins {
        let _ = j.join();
    }
}
