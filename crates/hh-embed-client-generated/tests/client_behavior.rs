//! Generated-client behaviour, driven with in-memory streams (no kernel process):
//!   - the generated `EXPECTED_SCHEMA_HASH`/`CONTRACT_MAJOR` match the live schema source
//!     (an in-test drift guard, complementary to `scripts/check-drift.sh`);
//!   - an RPC error response lowers to a typed `ClientError::Rpc`;
//!   - a mismatched returned identity lowers to `ClientError::IdentityMismatch` — never a
//!     silent fallback (ADR-0178 D2).

use hh_embed_client_generated::{
    negotiate, ClientError, ContractIdentity, CONTRACT_MAJOR, EXPECTED_SCHEMA_HASH,
};
use std::io::Cursor;

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
fn rpc_error_lowers_to_typed_client_error() {
    let response = r#"{"jsonrpc":"2.0","id":1,"error":{"code":1001,"message":"x","data":{"kind":"ContractMajorUnsupported","asked":1,"kernel_major":2}}}"#;
    let mut reader = Cursor::new(format!("{response}\n").into_bytes());
    let mut writer: Vec<u8> = Vec::new();
    match negotiate(&mut reader, &mut writer, "c", "1") {
        Err(ClientError::Rpc { code, kind, .. }) => {
            assert_eq!(code, 1001);
            assert_eq!(kind, "ContractMajorUnsupported");
        }
        other => panic!("expected typed Rpc error, got {other:?}"),
    }
    // The client actually wrote a framed hello request.
    let sent = String::from_utf8(writer).unwrap();
    assert!(sent.contains("\"method\":\"hello\""));
    assert!(sent.ends_with('\n'));
}

#[test]
fn mismatched_identity_lowers_to_typed_mismatch() {
    let response = r#"{"jsonrpc":"2.0","id":1,"result":{"contract_identity":{"contract_major":1,"schema_hash":"sha256:wrong","kernel_version_id":"k"}}}"#;
    let mut reader = Cursor::new(format!("{response}\n").into_bytes());
    let mut writer: Vec<u8> = Vec::new();
    match negotiate(&mut reader, &mut writer, "c", "1") {
        Err(ClientError::IdentityMismatch { field }) => assert_eq!(field, "schema_hash"),
        other => panic!("expected IdentityMismatch, got {other:?}"),
    }
}
