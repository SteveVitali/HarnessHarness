//! `hh-codegen` — the schema-export → codegen pipeline (ADR-0050 (e); CC7).
//!
//! Reads the single schema source ([`hh_embed_schema`]) and emits two checked-in artifacts:
//!   1. `schema/hh-embed-1.schema.json` — the canonical schema export.
//!   2. `crates/hh-embed-client-generated/src/generated.rs` — the generated client bindings
//!      (typed structs + the `hello` negotiation glue), asserting `(contract_major,
//!      schema_hash)` so a mismatch is typed, never a silent fallback (ADR-0178 D2).
//!
//! The generator is schema-driven: it reads field names/types/`required` from the exported
//! schema and fails loudly if a Stage-0 type is missing — so the generated bindings cannot
//! drift from the source. `scripts/check-drift.sh` re-runs this and diffs the working tree;
//! any diff is a build failure (CC7: "schema drift is a build failure").
//!
//! Usage: `hh-codegen [--root <dir>]` (default root: the current directory).

use std::path::{Path, PathBuf};

use hh_embed_schema as embed;
use hh_wire::json::Json;

mod emit;

fn main() -> std::process::ExitCode {
    let mut root = PathBuf::from(".");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => {
                root = PathBuf::from(args.next().expect("--root needs a value"));
            }
            "--help" | "-h" => {
                eprintln!("hh-codegen [--root <dir>]");
                return std::process::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("hh-codegen: unexpected argument {other:?}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    match generate(&root) {
        Ok(paths) => {
            for p in paths {
                eprintln!("hh-codegen: wrote {}", p.display());
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hh-codegen: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Generate every artifact under `root`. Returns the paths written.
pub fn generate(root: &Path) -> Result<Vec<PathBuf>, String> {
    let schema = embed::export_schema();
    let schema_hash = embed::schema_hash();

    // 1. The canonical schema export.
    let schema_path = root.join("schema").join("hh-embed-1.schema.json");
    let schema_body = format!("{}\n", embed::canonical_schema_bytes());
    write(&schema_path, &schema_body)?;

    // 2. The generated client bindings.
    let client_path = root
        .join("crates")
        .join("hh-embed-client-generated")
        .join("src")
        .join("generated.rs");
    let client_body = emit::generated_client(&schema, &schema_hash)?;
    write(&client_path, &client_body)?;
    // Format the emitted Rust with the pinned rustfmt so the generated file is byte-identical
    // to what `cargo fmt` produces — otherwise `cargo fmt --check` and the drift check would
    // disagree. rustfmt is a pinned toolchain component (rust-toolchain.toml).
    rustfmt(&client_path)?;

    Ok(vec![schema_path, client_path])
}

fn rustfmt(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .arg(path)
        .status()
        .map_err(|e| format!("rustfmt not available ({e}); needed to format generated code"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("rustfmt failed on {}", path.display()))
    }
}

fn write(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Helper re-used by tests: look up a type's field list in the exported schema.
pub fn type_fields<'a>(schema: &'a Json, type_name: &str) -> Result<&'a Vec<Json>, String> {
    let fields = schema
        .get("types")
        .and_then(|t| t.get(type_name))
        .and_then(|t| t.get("fields"))
        .ok_or_else(|| format!("schema is missing type {type_name:?}"))?;
    match fields {
        Json::Arr(v) => Ok(v),
        _ => Err(format!("type {type_name:?} fields is not an array")),
    }
}
