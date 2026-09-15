//! The stdio JSON-RPC 2.0 server loop — binding (b), Stage-0 slice.
//!
//! Reads newline-delimited requests, dispatches the Stage-0 verb set (`hello` only), writes
//! newline-delimited responses. Every negotiation failure lowers to the closed
//! [`embed::EmbedError`] sum as a typed JSON-RPC error — never a silent fallback (ADR-0178 D2).

use std::io::{BufReader, Read, Write};

use hh_embed_schema as embed;
use hh_wire::json::Json;
use hh_wire::jsonrpc;

use crate::kernel_identity;

/// Run the serve loop until end-of-stream. `reader` supplies newline-delimited requests;
/// `writer` receives newline-delimited responses.
pub fn run<R: Read, W: Write>(reader: R, mut writer: W) -> std::io::Result<()> {
    let mut buf = BufReader::new(reader);
    loop {
        match jsonrpc::read_message(&mut buf)? {
            None => return Ok(()),        // clean EOF
            Some(Json::Null) => continue, // blank line — ignore
            Some(msg) => {
                let response = handle(&msg);
                jsonrpc::write_message(&mut writer, &response)?;
            }
        }
    }
}

/// Handle one request message, producing exactly one response message.
fn handle(msg: &Json) -> Json {
    let line = msg.to_canonical_string();
    let req = match jsonrpc::parse_request(&line) {
        Ok(r) => r,
        Err(e) => {
            let err = embed::EmbedError::MalformedRequest {
                detail: e.to_string(),
            };
            return error(Json::Null, &err);
        }
    };

    match req.method.as_str() {
        "hello" => hello(&req.id, &req.params),
        other => {
            let err = embed::EmbedError::UnknownMethod {
                method: other.to_string(),
            };
            error(req.id, &err)
        }
    }
}

/// The `hello` verb: parse params, negotiate against the kernel identity, answer with the
/// kernel's [`embed::ContractIdentity`] or a typed error.
fn hello(id: &Json, params: &Json) -> Json {
    let parsed = match embed::HelloParams::from_json(params) {
        Ok(p) => p,
        Err(detail) => {
            return error(id.clone(), &embed::EmbedError::MalformedRequest { detail });
        }
    };
    let kernel = kernel_identity();
    match embed::negotiate(&parsed, &kernel) {
        Ok(()) => {
            let result = embed::HelloResult {
                contract_identity: kernel,
            };
            jsonrpc::ok_response(id.clone(), result.to_json())
        }
        Err(e) => error(id.clone(), &e),
    }
}

fn error(id: Json, e: &embed::EmbedError) -> Json {
    jsonrpc::err_response(id, e.code(), &e.message(), e.to_data_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(input: &str) -> Json {
        let mut out = Vec::new();
        run(std::io::Cursor::new(input.as_bytes().to_vec()), &mut out).unwrap();
        hh_wire::parse(String::from_utf8(out).unwrap().trim()).unwrap()
    }

    #[test]
    fn hello_returns_contract_identity() {
        let params = embed::HelloParams {
            client_name: "t".into(),
            client_version: "1".into(),
            asserted_contract_major: 1,
            asserted_schema_hash: None,
        };
        let req = jsonrpc::request(Json::Int(7), "hello", params.to_json());
        let resp = drive(&format!("{}\n", req.to_canonical_string()));
        let ci = resp
            .get("result")
            .and_then(|r| r.get("contract_identity"))
            .unwrap();
        let got = embed::ContractIdentity::from_json(ci).unwrap();
        assert_eq!(got, kernel_identity());
        assert_eq!(resp.get("id"), Some(&Json::Int(7)));
    }

    #[test]
    fn unknown_method_is_typed_error() {
        let req = jsonrpc::request(Json::Int(1), "nope", Json::Null);
        let resp = drive(&format!("{}\n", req.to_canonical_string()));
        let data = resp.get("error").and_then(|e| e.get("data")).unwrap();
        assert_eq!(
            data.get("kind").and_then(Json::as_str),
            Some("UnknownMethod")
        );
    }

    #[test]
    fn wrong_major_is_typed_error() {
        let params = embed::HelloParams {
            client_name: "t".into(),
            client_version: "1".into(),
            asserted_contract_major: 42,
            asserted_schema_hash: None,
        };
        let req = jsonrpc::request(Json::Int(1), "hello", params.to_json());
        let resp = drive(&format!("{}\n", req.to_canonical_string()));
        let err = resp.get("error").unwrap();
        assert_eq!(err.get("code"), Some(&Json::Int(1001)));
        assert_eq!(
            err.get("data")
                .and_then(|d| d.get("kind"))
                .and_then(Json::as_str),
            Some("ContractMajorUnsupported")
        );
    }
}
