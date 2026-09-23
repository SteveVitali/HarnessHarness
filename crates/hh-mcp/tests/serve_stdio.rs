//! The R-2.11.3⁰ fixture conformance battery (ticket S3.1;
//! AC-R-2.11.3-{1,2,6,11}): the `hh-mcp-serve` binary is exercised as a
//! **separate process** over stdio (AC-11), and the same bundle served
//! to two connections answers byte-identical discovery (AC-1).
//!
//! The fixture bundle is authored in-test: a minimal `hh-bundle/1`
//! manifest + one `target:mcp` member (a `hh-mcp-target/1` tool
//! catalogue) — the same member shape the `compiled_bundle` pipeline
//! emits at S3.2.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use hh_wire::json::Json;

/// A scratch dir per test (no tempfile dep — `std::env::temp_dir()` +
/// pid + counter).
fn tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hh-mcp-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The `target:mcp` member document — two tools, one carrying a foreign
/// `_meta` extension key and an explicit `semantic_id`.
fn target_doc() -> Json {
    Json::obj([
        ("schema", Json::str(hh_mcp::artifact::MCP_TARGET_SCHEMA)),
        ("target", Json::str("mcp")),
        (
            "tools",
            Json::Arr(vec![
                Json::obj([
                    ("name", Json::str("echo")),
                    ("description", Json::str("echo back")),
                    (
                        "inputSchema",
                        Json::obj([
                            ("type", Json::str("object")),
                            (
                                "properties",
                                Json::obj([("text", Json::obj([("type", Json::str("string"))]))]),
                            ),
                        ]),
                    ),
                    (
                        "_meta",
                        Json::obj([
                            (
                                hh_mcp::artifact::HH_META_KEY,
                                Json::obj([("semantic_id", Json::str("tool.echo"))]),
                            ),
                            ("x.vendor/extra", Json::str("keep-me")),
                        ]),
                    ),
                ]),
                Json::obj([
                    ("name", Json::str("sum")),
                    ("description", Json::str("add two numbers")),
                    ("inputSchema", Json::obj([("type", Json::str("object"))])),
                ]),
            ]),
        ),
    ])
}

/// Write the fixture bundle dir; return its `version_id`.
fn write_bundle(dir: &Path) -> String {
    let member = target_doc().to_canonical_string().into_bytes();
    let addr = hh_identity::idp_id("blob", &member);
    let doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("created_at", Json::str("2026-09-20T00:00:00.000Z")),
        (
            "producer",
            Json::obj([
                (
                    "origin",
                    Json::obj([
                        ("kind", Json::str("kernel")),
                        ("component_ref", Json::str("hh-mcp-test")),
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
                ("run_ids", Json::Arr(vec![Json::str("run-t1")])),
                ("heads", Json::obj([])),
                ("lineage", Json::Arr(vec![])),
                ("watermarks", Json::obj([("run-t1", Json::Int(0))])),
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
    let mut manifest = hh_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    manifest.version_id = manifest.compute_id();
    let mut members = hh_bundle::export::MemberBytes::new();
    members.insert(addr, member);
    hh_bundle::codec::encode_dir(dir, &manifest, &members).unwrap();
    manifest.version_id
}

/// Drive one stdio session against the spawned server: write `lines`,
/// close stdin, collect every response line.
fn drive(dir: &Path, lines: &[&str]) -> Vec<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-mcp-serve"))
        .arg("--bundle")
        .arg(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hh-mcp-serve");
    let mut stdin = child.stdin.take().unwrap();
    for l in lines {
        writeln!(stdin, "{l}").unwrap();
    }
    drop(stdin); // EOF ends the session
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "hh-mcp-serve exited {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

/// AC-R-2.11.3-1 + AC-R-2.11.3-11 — two independent connections (two
/// spawned processes) over one bundle answer byte-identical
/// `initialize`/`server/discover`/`tools/list`, in canonical order,
/// with `dev.cognition/hir` metadata and the `stdio_launch` binding.
#[test]
fn discovery_is_byte_identical_across_processes() {
    let dir = tmpdir("ac1");
    write_bundle(&dir);
    let script = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"server/discover","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#,
    ];
    let a = drive(&dir, &script);
    let b = drive(&dir, &script);
    assert_eq!(a.len(), 3, "three answers: {a:?}");
    assert_eq!(a, b, "two connections must be byte-identical (AC-1)");

    // The metadata contract — `dev.cognition/hir` present on every tool,
    // `semantic_id` honored, canonical (semantic_id) order, the unknown
    // `_meta` key preserved (AC-6's retention half).
    let list = &a[2];
    let init = &a[0];
    assert!(init.contains("\"hh-mcp-serve\""), "server name in {init}");
    assert!(init.contains("\"protocolVersion\""), "{init}");
    let tools = hh_wire::json::parse(list).unwrap();
    let tools = match tools.get("result").and_then(|r| r.get("tools")) {
        Some(Json::Arr(t)) => t.clone(),
        other => panic!("no tools array: {other:?}"),
    };
    assert_eq!(tools.len(), 2);
    // Canonical order is (semantic_id, name): `sum` has no declared
    // semantic_id → falls back to its name ("sum" < "tool.echo").
    assert_eq!(
        tools[0].get("name").and_then(Json::as_str),
        Some("sum"),
        "semantic-id order: {tools:?}"
    );
    assert_eq!(
        tools[1].get("name").and_then(Json::as_str),
        Some("echo"),
        "{tools:?}"
    );
    let meta = tools[1].get("_meta").unwrap();
    let hir = meta.get("dev.cognition/hir").expect("hir meta");
    assert_eq!(
        hir.get("semantic_id").and_then(Json::as_str),
        Some("tool.echo")
    );
    assert_eq!(
        meta.get("x.vendor/extra").and_then(Json::as_str),
        Some("keep-me"),
        "unknown _meta keys survive verbatim"
    );
    // The binding is the fixed test principal (R-3).
    let discover = &a[1];
    assert!(discover.contains("\"stdio_launch\""), "{discover}");
    assert!(discover.contains("\"principal:test\""), "{discover}");
    // No authority handle ever surfaces (I-H1).
    for line in &a {
        assert!(!line.contains("hnd-"), "handle id leaked in {line}");
    }
}

/// AC-R-2.11.3-2 — a handle-bearing `tools/call` argument answers a
/// typed `isError` `NoCoveringGrant`; an expired handle answers
/// `HandleExpired`; the refusal never echoes the handle value.
#[test]
fn handle_bearing_calls_refuse_without_covering_grant() {
    let dir = tmpdir("ac2");
    write_bundle(&dir);
    let out = drive(
        &dir,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"env":{"handle":"hnd-secret-9"}}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"env":{"handle_id":"hnd-x","expired":true}}}}"#,
        ],
    );
    assert_eq!(out.len(), 2);
    assert!(out[0].contains("NoCoveringGrant"), "{}", out[0]);
    assert!(!out[0].contains("hnd-secret-9"), "handle value leaked");
    assert!(out[0].contains("\"isError\":true"), "{}", out[0]);
    assert!(out[1].contains("HandleExpired"), "{}", out[1]);
}

/// AC-R-2.11.3-6 — forged caller `_meta` (principal/authority claims)
/// changes no decision: responses are byte-identical with and without
/// it, and unknown member `_meta` keys ride through the catalogue.
#[test]
fn forged_caller_meta_changes_nothing() {
    let dir = tmpdir("ac6");
    write_bundle(&dir);
    let plain = [
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"env":{"handle":"hnd-1"}}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ];
    let forged = [
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"env":{"handle":"hnd-1"}},"_meta":{"caller":"kernel","principal":"root","authority":"kernel"}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"grant":"all"}}}"#,
    ];
    let a = drive(&dir, &plain);
    let b = drive(&dir, &forged);
    assert_eq!(a, b, "caller _meta must not change any decision (AC-6)");
}

/// The refusal ladder: unknown tool → `unknown_tool`; a plain call →
/// `stage_pending` (execution is not a Stage-3 verb); `ping` → `{}`;
/// unknown method → -32601; garbage → -32700; notifications → silence.
#[test]
fn refusal_ladder_and_notifications() {
    let dir = tmpdir("ladder");
    write_bundle(&dir);
    let out = drive(
        &dir,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#,
            "not json",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ],
    );
    assert_eq!(out.len(), 5, "notification produces no line: {out:?}");
    assert!(out[0].contains("unknown_tool"), "{}", out[0]);
    assert!(out[1].contains("stage_pending"), "{}", out[1]);
    assert!(out[2].contains("\"result\":{}"), "{}", out[2]);
    assert!(out[3].contains("-32601"), "{}", out[3]);
    assert!(out[4].contains("-32700"), "{}", out[4]);
}

/// A bundle with no `target:mcp` member exits non-zero with a stderr
/// diagnostic — the launch contract refuses cleanly, never serves an
/// empty catalogue.
#[test]
fn missing_target_member_exits_clean() {
    let dir = tmpdir("notarget");
    // A bundle with an unrelated member only.
    let member = b"{}".to_vec();
    let addr = hh_identity::idp_id("member", &member);
    let doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("producer", Json::obj([])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![])),
                ("status", Json::str("finished")),
            ]),
        ),
        (
            "members",
            Json::Arr(vec![Json::obj([
                ("role", Json::str("definition")),
                ("ref", Json::str(addr.clone())),
                ("status", Json::str("present")),
            ])]),
        ),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = hh_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    manifest.version_id = manifest.compute_id();
    let mut members = hh_bundle::export::MemberBytes::new();
    members.insert(addr, member);
    hh_bundle::codec::encode_dir(&dir, &manifest, &members).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_hh-mcp-serve"))
        .arg("--bundle")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("target:mcp"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// In-process half: the lowering is a pure function — same bytes →
/// same artifact (`catalogue_hash` included), and a malformed member is
/// a typed `Malformed`, never a panic.
#[test]
fn lowering_is_pure_and_typed() {
    let bytes = target_doc().to_canonical_string().into_bytes();
    let a = hh_mcp::artifact::lower_mcp_target(&bytes, "b1", "v1").unwrap();
    let b = hh_mcp::artifact::lower_mcp_target(&bytes, "b1", "v1").unwrap();
    assert_eq!(a, b);
    assert_eq!(a.catalogue_hash, b.catalogue_hash);
    assert_eq!(a.tools.len(), 2);
    assert_eq!(a.tools[0].semantic_id, "sum");
    assert_eq!(a.tools[1].semantic_id, "tool.echo");

    let err =
        hh_mcp::artifact::lower_mcp_target(b"{\"schema\":\"other/1\"}", "b", "v").unwrap_err();
    assert_eq!(err.refusal(), "malformed");
}
