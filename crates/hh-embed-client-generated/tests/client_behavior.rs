//! Generated-client behaviour, driven with in-memory streams (no kernel process):
//!   - the generated `EXPECTED_SCHEMA_HASH`/`CONTRACT_MAJOR` match the live schema source
//!     (an in-test drift guard, complementary to `scripts/check-drift.sh`);
//!   - an RPC error response lowers to a typed `ClientError::Rpc` carrying the
//!     decoded `EmbedError` (kind/code/retryable);
//!   - a mismatched returned identity lowers to `ClientError::IdentityMismatch` — never a
//!     silent fallback (ADR-0178 D2).

use hh_embed_client_generated::{
    Client, ClientDescriptor, ClientError, ClientKind, ContractIdentity, HelloParams,
    HostCapabilities, CONTRACT_MAJOR, EXPECTED_SCHEMA_HASH,
};
use std::collections::BTreeMap;
use std::io::Cursor;

fn caps() -> HostCapabilities {
    HostCapabilities {
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
    }
}

fn hello_params() -> HelloParams {
    HelloParams {
        contract_major: CONTRACT_MAJOR,
        client: ClientDescriptor {
            name: "client-behavior-test".into(),
            version: "0".into(),
            kind: ClientKind::Test,
        },
        capabilities: caps(),
        schema_hash: Some(EXPECTED_SCHEMA_HASH.to_string()),
        kernel_floor: None,
    }
}

/// A well-formed `hello` result line with a caller-chosen schema hash /
/// contract major (the fields the client asserts).
fn hello_result_line(schema_hash: &str, contract_major: i64) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"kernel\":{{\"version\":\"0.0.1\",\"schema_hash\":\"{schema_hash}\",\"contract_major\":{contract_major},\"idp\":\"test-idp\"}},\"negotiated\":{{}},\"stability\":{{}},\"experimental_enabled\":false}}}}\n"
    )
}

#[test]
fn generated_constants_match_the_live_schema_source() {
    assert_eq!(EXPECTED_SCHEMA_HASH, hh_embed_schema::schema_hash());
    assert_eq!(CONTRACT_MAJOR, hh_embed_schema::CONTRACT_MAJOR);
}

#[test]
fn generated_contract_identity_round_trips() {
    let ci = ContractIdentity {
        contract_major: 1,
        schema_hash: EXPECTED_SCHEMA_HASH.to_string(),
        kernel_version_id: "hh-kernel/0.0.1".to_string(),
    };
    let back = ContractIdentity::from_json(&ci.to_json()).unwrap();
    assert_eq!(back, ci);
}

#[test]
fn hello_round_trips_and_stores_negotiated() {
    let response = hello_result_line(EXPECTED_SCHEMA_HASH, CONTRACT_MAJOR);
    let reader = Cursor::new(response.into_bytes());
    let mut writer: Vec<u8> = Vec::new();
    let mut client = Client::new(reader, &mut writer);
    let result = client.hello(&hello_params()).unwrap();
    assert_eq!(result.kernel.schema_hash, EXPECTED_SCHEMA_HASH);
    assert!(client.negotiated().is_some());
    // The client actually wrote a framed hello request.
    let sent = String::from_utf8(writer).unwrap();
    assert!(sent.contains("\"method\":\"hello\""));
    assert!(sent.ends_with('\n'));
}

#[test]
fn rpc_error_lowers_to_typed_client_error() {
    let response = "{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":1001,\"message\":\"x\",\"data\":{\"kind\":\"ContractMajorUnsupported\",\"asked\":1,\"kernel_major\":2}}}\n";
    let reader = Cursor::new(response.as_bytes().to_vec());
    let mut writer: Vec<u8> = Vec::new();
    let mut client = Client::new(reader, &mut writer);
    match client.hello(&hello_params()) {
        Err(ClientError::Rpc(e)) => {
            assert_eq!(e.code, 1001);
            assert_eq!(e.kind, "ContractMajorUnsupported");
        }
        other => panic!("expected typed Rpc error, got {other:?}"),
    }
    let sent = String::from_utf8(writer).unwrap();
    assert!(sent.contains("\"method\":\"hello\""));
    assert!(sent.ends_with('\n'));
}

#[test]
fn mismatched_schema_hash_lowers_to_typed_mismatch() {
    let response = hello_result_line("sha256:wrong", CONTRACT_MAJOR);
    let reader = Cursor::new(response.into_bytes());
    let mut writer: Vec<u8> = Vec::new();
    let mut client = Client::new(reader, &mut writer);
    match client.hello(&hello_params()) {
        Err(ClientError::IdentityMismatch { field }) => assert_eq!(field, "schema_hash"),
        other => panic!("expected IdentityMismatch, got {other:?}"),
    }
}

#[test]
fn mismatched_contract_major_lowers_to_typed_mismatch() {
    let response = hello_result_line(EXPECTED_SCHEMA_HASH, 2);
    let reader = Cursor::new(response.into_bytes());
    let mut writer: Vec<u8> = Vec::new();
    let mut client = Client::new(reader, &mut writer);
    match client.hello(&hello_params()) {
        Err(ClientError::IdentityMismatch { field }) => assert_eq!(field, "contract_major"),
        other => panic!("expected IdentityMismatch, got {other:?}"),
    }
}
