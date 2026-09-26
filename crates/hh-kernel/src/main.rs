//! `hh-kernel` — the E1 reference kernel binary, Stage-0 skeleton.
//!
//! Subcommands:
//!   export-schema   print the canonical schema export (codegen pipeline input) to stdout
//!   serve           run the newline-delimited JSON-RPC 2.0 server over stdio (binding (b))
//!   hello           print this kernel's ContractIdentity (in-process; for humans/doctor)
//!   version         print kernel_version_id, contract_major and schema_hash
//!   doctor          self-check: export → hash → in-process hello round trip
//!
//! Everything the kernel exposes goes through `hh-embed/1`; `serve` is the Stage-0 slice of
//! that contract (§7.4). `stderr` is for logs only (ADR-0179 D1(b)).

use std::io::{self, Write};
use std::process::ExitCode;

use hh_embed_schema as embed;
use hh_wire::json::Json;
use hh_wire::jsonrpc;

mod serve;

/// The kernel version id recorded in `ContractIdentity`. A SemVer-class label (ADR-0037 name
/// history); sourced from the build so it cannot drift from the package.
pub const KERNEL_VERSION_ID: &str = concat!("hh-kernel/", env!("CARGO_PKG_VERSION"));

/// This kernel's contract identity: the negotiated `(contract_major, schema_hash)` plus the
/// kernel version id.
pub fn kernel_identity() -> embed::ContractIdentity {
    embed::ContractIdentity {
        contract_major: embed::CONTRACT_MAJOR,
        schema_hash: embed::schema_hash(),
        kernel_version_id: KERNEL_VERSION_ID.to_string(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("doctor");
    match cmd {
        "export-schema" => {
            println!("{}", embed::canonical_schema_bytes());
            ExitCode::SUCCESS
        }
        "serve" => match serve::run(io::stdin().lock(), io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hh-kernel serve: {e}");
                ExitCode::FAILURE
            }
        },
        "hello" => {
            println!("{}", kernel_identity().to_json().to_canonical_string());
            ExitCode::SUCCESS
        }
        "version" => {
            let id = kernel_identity();
            println!(
                "{}",
                Json::obj([
                    ("kernel_version_id", Json::str(id.kernel_version_id)),
                    ("contract_major", Json::Int(id.contract_major)),
                    ("schema_hash", Json::str(id.schema_hash)),
                ])
                .to_canonical_string()
            );
            ExitCode::SUCCESS
        }
        "doctor" => doctor(),
        "--help" | "-h" | "help" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("hh-kernel: unknown subcommand {other:?}");
            print_help();
            ExitCode::FAILURE
        }
    }
}

/// In-process self-check: export the schema, content-address it, and drive one `hello`
/// negotiation loop-back to prove the boundary skeleton is wired end to end.
fn doctor() -> ExitCode {
    let id = kernel_identity();

    // 1. Schema export is non-empty and content-addressed.
    let bytes = embed::canonical_schema_bytes();
    assert!(!bytes.is_empty());
    assert!(id.schema_hash.starts_with("sha256:"));

    // 2. In-process hello round trip through the serve handler over an in-memory pipe.
    let req = jsonrpc::request(
        Json::Int(1),
        "hello",
        embed::HelloParams {
            contract_major: embed::CONTRACT_MAJOR,
            client: embed::ClientDescriptor {
                name: "hh-kernel doctor".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                kind: "cli".into(),
            },
            capabilities: embed::HostCapabilities::default(),
            schema_hash: Some(id.schema_hash.clone()),
            kernel_floor: None,
        }
        .to_json(),
    );
    let mut input = req.to_canonical_string().into_bytes();
    input.push(b'\n');
    let mut out: Vec<u8> = Vec::new();
    if let Err(e) = serve::run(io::Cursor::new(input), &mut out) {
        eprintln!("doctor: serve failed: {e}");
        return ExitCode::FAILURE;
    }
    let resp_line = String::from_utf8_lossy(&out);
    let resp = match hh_wire::parse(resp_line.trim()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("doctor: bad response: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The hello result's kernel descriptor must report this kernel's
    // identity — version id, contract major, schema hash (the
    // negotiated identity the client pins, I4).
    let kernel = resp.get("result").and_then(|r| r.get("kernel"));
    let ok = kernel.and_then(|k| k.get("version")).and_then(Json::as_str)
        == Some(id.kernel_version_id.as_str())
        && kernel
            .and_then(|k| k.get("schema_hash"))
            .and_then(Json::as_str)
            == Some(id.schema_hash.as_str())
        && kernel
            .and_then(|k| k.get("contract_major"))
            .and_then(Json::as_int)
            == Some(id.contract_major);
    if ok {
        let _ = writeln!(
            io::stderr(),
            "doctor: ok — {} schema_hash={}",
            id.kernel_version_id,
            id.schema_hash
        );
        println!("ok");
        ExitCode::SUCCESS
    } else {
        eprintln!("doctor: identity mismatch: {resp_line}");
        ExitCode::FAILURE
    }
}

fn print_help() {
    let _ = writeln!(
        io::stderr(),
        "hh-kernel <export-schema|serve|hello|version|doctor>"
    );
}
