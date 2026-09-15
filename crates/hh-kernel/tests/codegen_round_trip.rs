//! The codegen round trip (§10.6 M-S2-3; AC-R-2.11.4-1 Stage-0 slice): a client *generated*
//! from the schema export drives `hello → ContractIdentity` against the running kernel over
//! stdio and asserts `(contract_major, schema_hash)`. Green = the generated stub, the schema
//! source and the running kernel all agree. Fails if any of them drifts.

use hh_embed_client_generated as client;
use std::io::BufReader;
use std::process::{Command, Stdio};

#[test]
fn generated_client_negotiates_contract_identity_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-kernel"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hh-kernel serve");

    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    let identity = client::negotiate(&mut reader, &mut stdin, "codegen-round-trip", "0.0.1")
        .expect("negotiation should succeed");

    assert_eq!(identity.contract_major, client::CONTRACT_MAJOR);
    assert_eq!(identity.schema_hash, client::EXPECTED_SCHEMA_HASH);
    assert!(identity.kernel_version_id.starts_with("hh-kernel/"));

    drop(stdin);
    let _ = child.wait();
}
