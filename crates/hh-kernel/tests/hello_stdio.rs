//! The stdio binding skeleton: a raw newline-delimited JSON-RPC 2.0 `hello` against the
//! spawned kernel returns a `ContractIdentity` (§7.4 Stage-0; binding (b), ADR-0179 D1(b)).
//! This test fails if the `hello`/`ContractIdentity`/stdio negotiation is removed.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn hello_over_stdio_returns_contract_identity() {
    // Bind the ledger to a unique temp root — `serve` defaults to
    // `<cwd>/.hh/store`, which would dirty the crate dir.
    let store = std::env::temp_dir().join(format!("hh-kernel-hello-stdio-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-kernel"))
        .arg("serve")
        .env("HH_STORE_ROOT", &store)
        .env("HH_WORKSPACE_ROOT", &store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hh-kernel serve");

    let mut stdin = child.stdin.take().unwrap();
    let req = r#"{"jsonrpc":"2.0","id":1,"method":"hello","params":{"contract_major":1,"client":{"name":"raw","version":"1","kind":"test"},"capabilities":{"experimental":false,"opt_out_notifications":[],"serves_permission_channel":false,"serves_host_executor":false,"serves_hook_observer":false,"serves_elicitation":false,"serves_measurement":false,"serves_principal_channel":false,"accepts_ephemeral_frames":true}}}"#;
    writeln!(stdin, "{req}").unwrap();
    drop(stdin); // EOF ends the serve loop

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "serve exited non-zero");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let line = stdout.lines().next().expect("a response line");

    // The `hello` result carries the kernel's `ContractIdentity` under
    // `kernel{version, schema_hash, contract_major, idp}` (§7.4 §2.2).
    assert!(line.contains("\"kernel\""), "no kernel identity in {line}");
    assert!(
        line.contains("\"contract_major\":1"),
        "wrong contract_major in {line}"
    );
    assert!(
        line.contains("sha256:"),
        "schema_hash is not a content address in {line}"
    );

    // stderr is for logs only; the machine response is on stdout (ADR-0179 D1(b)).
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        !stderr.contains("\"result\""),
        "response leaked onto stderr"
    );
}
