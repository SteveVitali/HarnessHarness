//! L4.6 — `capstone-mcp` fallback arm (ADR-0363): the `hh-mcp` surface
//! (`hh-mcp-serve`) driven by a *real MCP client*. The leg prefers the
//! official `@modelcontextprotocol/sdk` client when it installs without
//! new repo deps (npx, runtime-only); when it cannot, this gated cell is
//! the stated in-repo client — `hharness_mcp::client::McpClient` over
//! `StdioTransport`, a real spawned `hh-mcp-serve` process, real
//! handshake → `tools/list` → `tools/call`. The leg's record names which
//! client ran.
//!
//! The served artifact is a fixture `hh-bundle/1` dir whose `target:mcp`
//! member carries one echo tool — the same lowering `s3_9` exercises;
//! `HH_MCP_BUNDLE_OUT` (set by the leg when the official-SDK arm runs)
//! materializes the bundle for an external client instead of driving it
//! in-process — the cell then writes the dir and returns (the bundle
//! write is *not* the leg; the client drive is).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use hharness_mcp::client::{McpClient, StdioTransport};
use hharness_wire::json::Json;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-mcp-l46-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A `hh-mcp-target/1` member — `tools` verbatim + `ttl_ms` (s3_9's
/// fixture shape).
fn target_doc(tools: Vec<Json>, ttl_ms: i64) -> Json {
    Json::obj([
        (
            "schema",
            Json::str(hharness_mcp::artifact::MCP_TARGET_SCHEMA),
        ),
        ("target", Json::str("mcp")),
        ("ttl_ms", Json::Int(ttl_ms)),
        ("tools", Json::Arr(tools)),
    ])
}

fn echo_tool() -> Json {
    Json::obj([
        ("name", Json::str("l46.echo")),
        ("description", Json::str("capstone echo tool")),
        (
            "inputSchema",
            Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("p", Json::obj([("type", Json::str("string"))]))]),
                ),
            ]),
        ),
        ("_meta", Json::obj([("hir", Json::str("capstone"))])),
    ])
}

/// Write the fixture bundle dir the server lowers (s3_9's writer).
fn write_bundle(dir: &Path) {
    let tools = vec![echo_tool()];
    let member = target_doc(tools, 0).to_canonical_string().into_bytes();
    let addr = hharness_identity::idp_id("blob", &member);
    let doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("created_at", Json::str("2026-10-23T00:00:00.000Z")),
        (
            "producer",
            Json::obj([
                (
                    "origin",
                    Json::obj([
                        ("kind", Json::str("kernel")),
                        ("component_ref", Json::str("l4.6-capstone")),
                    ]),
                ),
                ("authority", Json::str("kernel")),
                ("scope", Json::str("run")),
                ("created_at", Json::Int(0)),
            ]),
        ),
        ("participant_class", Json::str("native")),
        ("observability_levels", Json::Arr(vec![])),
        ("claims", Json::Arr(vec![])),
        ("name_bindings", Json::Arr(vec![])),
        ("fetch_policy", Json::str("self_contained")),
        ("fetch", Json::Arr(vec![])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![Json::str("run-l46")])),
                ("heads", Json::obj([])),
                ("lineage", Json::Arr(vec![])),
                ("watermarks", Json::obj([("run-l46", Json::Int(0))])),
                ("status", Json::str("finished")),
            ]),
        ),
        ("definition", Json::obj([])),
        ("configuration", Json::obj([])),
        ("resolved_dependencies", Json::obj([])),
        ("model", Json::obj([("snapshots", Json::Arr(vec![]))])),
        ("instrument", Json::obj([])),
        ("traces", Json::obj([])),
        ("results", Json::obj([("rows", Json::Arr(vec![]))])),
        ("reproducibility", Json::obj([])),
        (
            "members",
            Json::Arr(vec![Json::obj([
                ("role", Json::str("target:mcp")),
                ("ref", Json::str(addr.clone())),
                ("media_type", Json::str("application/json")),
                ("size", Json::Int(member.len() as i64)),
                ("status", Json::str("present")),
            ])]),
        ),
        ("unpinned", Json::Arr(vec![])),
        ("ext", Json::obj([])),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = hharness_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    manifest.version_id = manifest.compute_id();
    let mut members = hharness_bundle::export::MemberBytes::new();
    members.insert(addr, member);
    hharness_bundle::codec::encode_dir(dir, &manifest, &members).unwrap();
}

/// `capstone-mcp` — the in-repo-client arm: a real spawned
/// `hh-mcp-serve`, negotiated handshake, `tools/list`, `tools/call`.
/// With `HH_MCP_BUNDLE_OUT` set the cell only materializes the bundle
/// for the official-SDK driver and returns.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh capstone-mcp"]
fn l46_live_mcp_client() {
    let dir = match std::env::var("HH_MCP_BUNDLE_OUT") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => tmpdir("bundle"),
    };
    let _ = std::fs::remove_dir_all(&dir);
    write_bundle(&dir);
    if std::env::var("HH_MCP_BUNDLE_OUT").is_ok() {
        println!("MCP_BUNDLE: materialized at {dir:?}");
        return;
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hh-mcp-serve"));
    cmd.arg("--bundle").arg(&dir);
    let t = StdioTransport::spawn(&mut cmd).expect("spawn hh-mcp-serve");
    let mut client = McpClient::connect(t).expect("connect");
    println!(
        "MCP_BINDING: era={:?} version={}",
        client.binding().era,
        client.binding().negotiated_version
    );
    let listing = client.list_tools().expect("tools/list");
    assert!(
        listing
            .tools
            .iter()
            .any(|t| t.get("name").and_then(Json::as_str) == Some("l46.echo")),
        "tools/list carries the served tool: {:?}",
        listing
            .tools
            .iter()
            .map(|t| t.get("name").cloned())
            .collect::<Vec<_>>()
    );
    println!(
        "MCP_TOOLS: {}",
        listing
            .tools
            .iter()
            .filter_map(|t| t.get("name").and_then(Json::as_str))
            .collect::<Vec<_>>()
            .join(",")
    );
    let answer = client
        .call_tool("l46.echo", &Json::obj([("p", Json::str("capstone"))]), None)
        .expect("tools/call");
    // The fixture surface answers honestly: either a real result or a
    // *typed* refusal — never a protocol error or a fabricated pass.
    match &answer {
        hharness_mcp::client::ToolOutcome::Observed { .. } => {
            println!("MCP_CALL: outcome=observed")
        }
        hharness_mcp::client::ToolOutcome::Failed { result } => {
            let refusal = result
                .get("structuredContent")
                .and_then(|s| s.get("refusal"))
                .and_then(Json::as_str)
                .unwrap_or("");
            assert!(!refusal.is_empty(), "typed refusal carries its reason");
            println!("MCP_CALL: outcome=typed-refusal refusal={refusal}");
        }
        other => println!("MCP_CALL: outcome={other:?}"),
    }
    println!("MCP_RESULT: in-repo-client pass");
    let _ = std::fs::remove_dir_all(&dir);
}
