//! R6.15 (AR-05; REQ-XB-129/113) — the `hh-cell-relay` argv surface, the
//! socket-mount flags, a real byte-pipe leg, and the static-link check
//! (recorded skipped where a musl toolchain is unavailable).

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use hharness_helper::podman::{CellSpec, PodmanBackend, RelaySpec, SocketMount};

fn backend() -> PodmanBackend {
    PodmanBackend {
        container: "hh-env-t".into(),
        image: "img".into(),
        workspace: "/tmp/w".into(),
        network: false,
        extra_args: vec![],
        cell: CellSpec::default(),
        mounts: vec![],
        relay: None,
    }
}

#[test]
fn create_args_mounts_and_relay_tail() {
    let mut b = backend();
    b.mounts = vec![
        SocketMount {
            name: "model".into(),
            host_path: "/h/m.sock".into(),
        },
        SocketMount {
            name: "egress".into(),
            host_path: "/h/e.sock".into(),
        },
    ];
    b.relay = Some(RelaySpec {
        host_bin: "/h/hh-cell-relay".into(),
        pairs: vec![("egress".into(), 3128), ("model".into(), 8080)],
    });
    let a = b.create_args();
    let j = a.join(" ");
    assert!(j.contains("--network none"), "{j}");
    assert!(j.contains("-v /h/m.sock:/run/hh/model.sock"), "{j}");
    assert!(j.contains("-v /h/e.sock:/run/hh/egress.sock"), "{j}");
    assert!(
        j.contains("-v /h/hh-cell-relay:/hh/bin/hh-cell-relay:ro"),
        "{j}"
    );
    assert!(j.contains("--init"), "{j}");
    assert!(j.contains(
        "/hh/bin/hh-cell-relay --pair egress=3128=/run/hh/egress.sock --pair model=8080=/run/hh/model.sock"
    ), "{j}");
    assert!(!j.contains("sleep"), "{j}");
    // Absent mounts/relay: byte-identical to the pre-admission argv.
    let a2 = backend().create_args().join(" ");
    assert!(a2.ends_with("img sleep infinity"), "{a2}");
}

#[test]
fn relay_bridges_loopback_to_socket() {
    // A real byte pipe: unix listener upstream, the relay binary
    // bridging an ephemeral loopback port.
    let d = std::env::temp_dir().join(format!("hh-relay-t-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let sock = d.join("egress.sock");
    let lis = UnixListener::bind(&sock).unwrap();
    let up = std::thread::spawn(move || {
        let (mut s, _) = lis.accept().unwrap();
        let mut b = [0u8; 16];
        let n = s.read(&mut b).unwrap();
        assert_eq!(&b[..n], b"through");
        s.write_all(b"back").unwrap();
    });
    let bin = env!("CARGO_BIN_EXE_hh-cell-relay");
    let port = {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    let mut child = Command::new(bin)
        .args(["--pair", &format!("egress={port}={}", sock.display())])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.write_all(b"through").unwrap();
    let mut b = [0u8; 8];
    let n = c.read(&mut b).unwrap();
    assert_eq!(&b[..n], b"back");
    drop(c);
    up.join().unwrap();
    let _ = child.kill();
    // Closed-name refusal: `model|egress` only.
    let out = Command::new(bin)
        .args(["--pair", "evil=1=/x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cell_relay_is_statically_linked() {
    // AR-05: no dynamic interpreter/needed libs on the musl artifact.
    // Build it when the target is usable; record a real skip elsewhere
    // (a macOS host without a musl linker cannot produce it — an honest
    // skipped(<reason>), never a fabricated pass).
    let artifact = std::env::var("HH_CELL_RELAY_MUSL_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/x86_64-unknown-linux-musl/debug/hh-cell-relay")
        });
    if !artifact.exists() {
        let ok = Command::new("cargo")
            .args([
                "build",
                "--target",
                "x86_64-unknown-linux-musl",
                "-p",
                "hharness-helper",
                "--bin",
                "hh-cell-relay",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!(
                "skipped(host): musl build unavailable on {} — run under the Linux CI toolchain",
                std::env::consts::OS
            );
            return;
        }
    }
    if !artifact.exists() {
        eprintln!(
            "skipped(artifact): {} absent after build",
            artifact.display()
        );
        return;
    }
    let f = Command::new("file").arg(&artifact).output().unwrap();
    let out = String::from_utf8_lossy(&f.stdout);
    assert!(
        out.contains("statically linked") || out.contains("static-pie linked"),
        "not static: {out}"
    );
    let r = Command::new("readelf")
        .args(["-d", artifact.to_str().unwrap()])
        .output()
        .unwrap();
    let so = String::from_utf8_lossy(&r.stdout);
    assert!(!so.contains("NEEDED"), "dynamic deps: {so}");
}
