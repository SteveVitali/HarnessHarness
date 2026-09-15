//! The stdio binding skeleton: a raw newline-delimited JSON-RPC 2.0 `hello` against the
//! spawned kernel returns a `ContractIdentity` (§7.4 Stage-0; binding (b), ADR-0179 D1(b)).
//! This test fails if the `hello`/`ContractIdentity`/stdio negotiation is removed.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn hello_over_stdio_returns_contract_identity() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-kernel"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hh-kernel serve");

    let mut stdin = child.stdin.take().unwrap();
    let req = r#"{"jsonrpc":"2.0","id":1,"method":"hello","params":{"asserted_contract_major":1,"client_name":"raw","client_version":"1"}}"#;
    writeln!(stdin, "{req}").unwrap();
    drop(stdin); // EOF ends the serve loop

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "serve exited non-zero");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let line = stdout.lines().next().expect("a response line");

    assert!(
        line.contains("\"contract_identity\""),
        "no ContractIdentity in {line}"
    );
    assert!(
        line.contains("\"contract_major\":1"),
        "wrong contract_major in {line}"
    );
    assert!(
        line.contains("sha256:"),
        "schema_hash is not a content address in {line}"
    );
    assert!(
        line.contains("hh-kernel/"),
        "no kernel_version_id in {line}"
    );

    // stderr is for logs only; the machine response is on stdout (ADR-0179 D1(b)).
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        !stderr.contains("\"contract_identity\""),
        "response leaked onto stderr"
    );
}
