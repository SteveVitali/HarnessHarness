//! The S3.9 protocol-edge battery (§5d.4; AC-R-2.5.4-{2,4,9,12}) —
//! every row exercised **against the out-of-process `hh-mcp-serve`
//! binary** through the real `StdioTransport` + `McpClient`:
//!
//! - AC-R-2.5.4-2 — two clients, one bundle: byte-identical
//!   `tools/list` with `ttlMs`/`cacheScope`/`resultType`; a changed
//!   bundle fires `notifications/tools/list_changed` (`--watch`).
//! - AC-R-2.5.4-4 — the modern server connects through
//!   `server/discover`; the `--legacy` server is detected by the
//!   compatibility probe; the `ProtocolBinding`s differ in `era`/
//!   `negotiated_version`; an unpinned version is a typed
//!   `ProtocolVersionMismatch` (in-process half).
//! - AC-R-2.5.4-9 — foreign `_meta` keys survive the serve boundary
//!   verbatim.
//! - AC-R-2.5.4-12 — `resultType: "input_required"` maps to `Paused`
//!   with an opaque `requestState`; the retry echoes it unmodified;
//!   `isError` and the paused/observed shapes stay distinct.

use std::path::{Path, PathBuf};
use std::process::Command;

use hh_mcp::client::{ClientError, McpClient, StdioTransport, ToolOutcome};
use hh_mcp::protocol::{Era, PINNED_LEGACY, PINNED_MODERN};
use hh_wire::json::Json;

fn tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hh-mcp-s39-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `hh-mcp-target/1` member — `tools` verbatim + `ttl_ms`.
fn target_doc(tools: Vec<Json>, ttl_ms: i64) -> Json {
    Json::obj([
        ("schema", Json::str(hh_mcp::artifact::MCP_TARGET_SCHEMA)),
        ("target", Json::str("mcp")),
        ("ttl_ms", Json::Int(ttl_ms)),
        ("tools", Json::Arr(tools)),
    ])
}

fn tool(name: &str, desc: &str, meta: Json) -> Json {
    Json::obj([
        ("name", Json::str(name)),
        ("description", Json::str(desc)),
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
        ("_meta", meta),
    ])
}

/// Write a fixture bundle dir for `tools`; return its version_id.
fn write_bundle(dir: &Path, tools: Vec<Json>, ttl_ms: i64) {
    let member = target_doc(tools, ttl_ms).to_canonical_string().into_bytes();
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
}

/// Connect a real client to a spawned server (`--legacy` toggles the
/// served era; `--watch` enables per-iteration reload).
fn connect(dir: &Path, extra: &[&str]) -> McpClient<StdioTransport> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hh-mcp-serve"));
    cmd.args(extra).arg("--bundle").arg(dir);
    let t = StdioTransport::spawn(&mut cmd).expect("spawn hh-mcp-serve");
    McpClient::connect(t).expect("connect")
}

/// AC-R-2.5.4-2 — two independent clients over one bundle: byte-
/// identical `tools/list` carrying `ttlMs`/`cacheScope`/`resultType`,
/// canonical order, and the full HIR block in `_meta`.
#[test]
fn ac_e4_2_byte_identical_listing_with_freshness_meta() {
    let dir = tmpdir("e4-2");
    write_bundle(
        &dir,
        vec![
            tool(
                "beta",
                "second",
                Json::obj([(
                    hh_mcp::artifact::HH_META_KEY,
                    Json::obj([
                        ("semantic_id", Json::str("cap.beta")),
                        ("effects", Json::Arr(vec![Json::str("fs_read")])),
                    ]),
                )]),
            ),
            tool(
                "alpha",
                "first",
                Json::obj([(
                    hh_mcp::artifact::HH_META_KEY,
                    Json::obj([("semantic_id", Json::str("cap.alpha"))]),
                )]),
            ),
        ],
        30_000,
    );
    let mut a = connect(&dir, &[]);
    let mut b = connect(&dir, &[]);
    let la = a.list_tools().expect("list a");
    let lb = b.list_tools().expect("list b");
    assert_eq!(la.tools, lb.tools, "two clients — byte-identical tools[]");
    assert_eq!(la.ttl_ms, Some(30_000));
    assert_eq!(lb.ttl_ms, Some(30_000));
    assert_eq!(la.cache_scope.as_deref(), Some("private"));
    assert_eq!(la.result_type.as_deref(), Some("complete"));
    // Canonical order is (semantic_id, name): cap.alpha before cap.beta.
    assert_eq!(
        la.tools[0].get("name").and_then(Json::as_str),
        Some("alpha")
    );
    // The minimum carried set rides in `_meta` — `effects` included.
    let hir = la.tools[0]
        .get("_meta")
        .and_then(|m| m.get(hh_mcp::artifact::HH_META_KEY))
        .expect("hir meta");
    assert_eq!(
        hir.get("semantic_id").and_then(Json::as_str),
        Some("cap.alpha")
    );
    assert!(hir.get("bundle_id").is_some(), "bundle_id stamped: {hir:?}");
}

/// AC-R-2.5.4-2 second half — `--watch`: a changed bundle fires
/// `notifications/tools/list_changed` and the next `tools/list`
/// answers the new bytes.
#[test]
fn ac_e4_2_list_changed_on_bundle_change() {
    let dir = tmpdir("e4-2-watch");
    write_bundle(&dir, vec![tool("one", "v1", Json::obj([]))], 0);
    let mut c = connect(&dir, &["--watch"]);
    let first = c.list_tools().expect("list 1");
    assert_eq!(first.tools.len(), 1);
    // Mutate the served bundle — the loader re-lowers on the next
    // iteration and the notification precedes the next answer.
    write_bundle(
        &dir,
        vec![
            tool("one", "v1", Json::obj([])),
            tool("two", "v2", Json::obj([])),
        ],
        0,
    );
    // A call forces a loop turn; the server emits the notification at
    // the top of the following iteration — the next request consumes it.
    let _ = c.call_tool("one", &Json::obj([]), None).unwrap();
    let second = c.list_tools().expect("list 2");
    let notes = c.drain_notifications();
    assert!(
        notes
            .iter()
            .any(|n| n.get("method").and_then(Json::as_str)
                == Some("notifications/tools/list_changed")),
        "list_changed notification: {notes:?}"
    );
    assert_eq!(second.tools.len(), 2, "changed catalogue served");
    assert_ne!(
        first
            .tools
            .iter()
            .map(Json::to_canonical_string)
            .collect::<Vec<_>>(),
        second
            .tools
            .iter()
            .map(Json::to_canonical_string)
            .collect::<Vec<_>>(),
        "a second bundle changes the bytes"
    );
}

/// AC-R-2.5.4-4 — modern + legacy connect through the probe-first rule;
/// the bindings differ in `era`/`negotiated_version`; a legacy server
/// answers `server/discover` with `-32601` and is driven by the
/// `initialize` compatibility probe.
#[test]
fn ac_e4_4_dual_era_probe_first() {
    let dir = tmpdir("e4-4");
    write_bundle(&dir, vec![tool("t", "x", Json::obj([]))], 0);

    let modern = connect(&dir, &[]);
    assert_eq!(modern.binding().era, Era::Modern);
    assert_eq!(modern.binding().negotiated_version, PINNED_MODERN);
    assert!(modern.discover_doc().is_some(), "modern answers discover");
    // Probe-first: discover carried supportedVersions.
    let d = modern.discover_doc().unwrap();
    let versions: Vec<&str> = match d.get("supportedVersions") {
        Some(Json::Arr(v)) => v.iter().filter_map(|x| x.as_str()).collect(),
        _ => Vec::new(),
    };
    assert!(versions.contains(&PINNED_MODERN) && versions.contains(&PINNED_LEGACY));

    let legacy = connect(&dir, &["--legacy"]);
    assert_eq!(legacy.binding().era, Era::Legacy);
    assert_eq!(legacy.binding().negotiated_version, PINNED_LEGACY);
    assert!(
        legacy.discover_doc().is_none(),
        "a legacy peer never answers discover"
    );
    assert_ne!(
        modern.binding().negotiated_version,
        legacy.binding().negotiated_version,
        "bindings differ in era/version"
    );

    // The legacy projection drops resultType/ttlMs/cacheScope
    // (narrowed) — and the tools bytes are still the catalogue's.
    let mut legacy = legacy;
    let l = legacy.list_tools().expect("legacy list");
    assert_eq!(l.result_type, None, "legacy carries no resultType");
    assert_eq!(l.ttl_ms, None, "legacy carries no ttlMs");
    assert_eq!(l.tools.len(), 1);
}

/// AC-R-2.5.4-4 third clause — an unpinned `protocolVersion` echo from
/// the compatibility probe is `ProtocolVersionMismatch`, never a silent
/// fallback (in-process half; the scripted transport is a server that
/// reports only `1999-01-01`).
#[test]
fn ac_e4_4_unpinned_version_refuses_typed() {
    use hh_mcp::client::Transport;
    use std::collections::VecDeque;
    struct S(VecDeque<String>);
    impl Transport for S {
        fn send(&mut self, _l: &str) -> Result<(), String> {
            Ok(())
        }
        fn recv(&mut self) -> Result<Option<String>, String> {
            Ok(self.0.pop_front())
        }
    }
    let discover = Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(1)),
        (
            "result",
            Json::obj([(
                "supportedVersions",
                Json::Arr(vec![Json::str("1999-01-01")]),
            )]),
        ),
    ])
    .to_canonical_string();
    let s = S(VecDeque::from([discover]));
    match McpClient::connect(s).err() {
        Some(ClientError::ProtocolVersionMismatch {
            requested,
            supported,
        }) => {
            assert_eq!(requested, PINNED_MODERN);
            assert_eq!(supported, vec!["1999-01-01".to_string()]);
        }
        other => panic!("expected ProtocolVersionMismatch, got {other:?}"),
    }
}

/// AC-R-2.5.4-9 — unknown `_meta` keys and underscore-prefixed values
/// ride through the serve boundary byte-for-byte (N4).
#[test]
fn ac_e4_9_foreign_meta_survives() {
    let dir = tmpdir("e4-9");
    let foreign = Json::obj([("_vendor_field", Json::str("v")), ("see", Json::Int(7))]);
    write_bundle(
        &dir,
        vec![tool(
            "t",
            "x",
            Json::obj([("acme.corp/private", foreign.clone())]),
        )],
        0,
    );
    let mut c = connect(&dir, &[]);
    let l = c.list_tools().expect("list");
    let meta = l.tools[0].get("_meta").expect("meta");
    assert_eq!(meta.get("acme.corp/private"), Some(&foreign));
}

/// AC-R-2.5.4-12 — `input_required` is a paused outcome: the client
/// maps it to `Paused` with the opaque `requestState`; the retry under
/// the same name echoes `requestState` byte-for-byte; `isError` and
/// paused/observed stay distinct outcomes.
#[test]
fn ac_e4_12_input_required_pause_and_resume() {
    let dir = tmpdir("e4-12");
    write_bundle(
        &dir,
        vec![tool(
            "ask",
            "needs input",
            Json::obj([(
                hh_mcp::artifact::INPUT_REQUIRED_META_KEY,
                Json::obj([(
                    "inputRequests",
                    Json::Arr(vec![Json::obj([("question", Json::str("which file?"))])]),
                )]),
            )]),
        )],
        0,
    );
    let mut c = connect(&dir, &[]);
    let paused = c
        .call_tool("ask", &Json::obj([("p", Json::str("x"))]), None)
        .expect("call");
    let state = match &paused {
        ToolOutcome::Paused {
            input_requests,
            request_state,
        } => {
            assert!(input_requests.to_canonical_string().contains("which file?"));
            request_state.clone()
        }
        other => panic!("expected Paused, got {other:?}"),
    };
    // The retry — `requestState` echoed unmodified (the driver keeps
    // `effect_id`/`attempt_no`; the wire half only echoes the blob).
    let resumed = c
        .call_tool("ask", &Json::obj([("p", Json::str("x"))]), Some(&state))
        .expect("resume");
    match resumed {
        ToolOutcome::Observed { result } => {
            assert_eq!(result.get("requestState"), Some(&state));
        }
        other => panic!("expected Observed, got {other:?}"),
    }
    // `isError` stays a distinct outcome (never conflated with paused).
    let failed = c.call_tool("ask", &Json::obj([]), None).expect("call");
    // `ask` pauses again on a stateless retry — use an unknown tool for
    // the isError arm.
    let err = c.call_tool("nope", &Json::obj([]), None).expect("call");
    assert!(matches!(err, ToolOutcome::Failed { .. }), "{err:?}");
    let _ = failed;
}

// ── AC-R-2.5.4-8 — the spec DAG: no edge to a protocol schema ────────────
//
// "No edge from any HIR entity, ledger event class or component-class
// contract to a protocol schema; only `lower_target`, `lift`, `negotiate`,
// `render_session` and transports import them." The compile-time form of
// that rule is the dependency graph: the record/contract crates never name
// the protocol crate (`hh-mcp`) as a dependency — a wire schema stays data,
// read by the boundary (`hh-embed` ops → `registry::import`, `hh-cli`'s
// serve wiring), never by a semantic record.
#[test]
fn ac_e4_8_no_spec_edge_to_protocol_schema() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    const PROTECTED: &[&str] = &[
        // HIR entities, ledger event classes, component-class contracts and
        // their foundations — none may edge to a protocol-schema crate.
        "hh-hir",
        "hh-ledger",
        "hh-ontology",
        "hh-provenance",
        "hh-identity",
        "hh-wire",
        "hh-budget",
        "hh-monitor",
        "hh-containment",
        "hh-secrets",
        "hh-registry",
    ];
    const PROTOCOL: &[&str] = &["hh-mcp"];
    for krate in PROTECTED {
        let manifest = root.join("crates").join(krate).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                section = line;
                continue;
            }
            // Only shipped edges count (`dev-dependencies` are test-time,
            // never part of the artifact graph).
            if section == "[dependencies]" || section == "[build-dependencies]" {
                let dep = line.split('=').next().unwrap_or("").trim();
                assert!(
                    !PROTOCOL.contains(&dep),
                    "{krate} holds a protocol-schema edge ({dep}) — the spec DAG \
                     admits only lower_target/lift/negotiate/render_session/transport importers"
                );
            }
        }
    }
}
