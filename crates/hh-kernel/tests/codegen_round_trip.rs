//! The codegen round trip (§10.6 M-S2-3; AC-R-2.11.4-1 Stage-0 slice): a client *generated*
//! from the schema export drives `hello → ContractIdentity` against the running kernel over
//! stdio and asserts `(contract_major, schema_hash)`. Green = the generated stub, the schema
//! source and the running kernel all agree. Fails if any of them drifts.

use hh_embed_client_generated as client;
use std::collections::BTreeMap;
use std::io::BufReader;
use std::process::{Command, Stdio};

#[test]
fn generated_client_negotiates_contract_identity_over_stdio() {
    // Bind the ledger to a unique temp root — `serve` defaults to
    // `<cwd>/.hh/store`, which would dirty the crate dir.
    let store = std::env::temp_dir().join(format!("hh-kernel-codegen-rt-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-kernel"))
        .arg("serve")
        .env("HH_STORE_ROOT", &store)
        .env("HH_WORKSPACE_ROOT", &store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hh-kernel serve");

    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    let mut c = client::Client::new(&mut reader, &mut stdin);
    let hello = c
        .hello(&client::HelloParams {
            contract_major: client::CONTRACT_MAJOR,
            client: client::ClientDescriptor {
                name: "codegen-round-trip".into(),
                version: "0.0.1".into(),
                kind: client::ClientKind::Test,
            },
            capabilities: client::HostCapabilities {
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
                extensions: BTreeMap::new(),
            },
            schema_hash: Some(client::EXPECTED_SCHEMA_HASH.to_string()),
            kernel_floor: None,
        })
        .expect("negotiation should succeed");

    assert_eq!(hello.kernel.contract_major, client::CONTRACT_MAJOR);
    assert_eq!(hello.kernel.schema_hash, client::EXPECTED_SCHEMA_HASH);
    assert_eq!(hello.kernel.version, env!("CARGO_PKG_VERSION"));

    drop(stdin);
    let _ = child.wait();
}
