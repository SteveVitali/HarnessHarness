//! S4.9 boundary tests — the `fleet.*` surface over `hharness-embed` (records
//! in, records out; one Store). Under the `tier-c4` feature: open →
//! reconcile → dispatch-note → settle → accountability over the fixture
//! adapter, plus the experimental gate + typed refusals. Without it:
//! the ops stay in the schema and answer `Unsupported{by: "tier-c4"}`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-fleet-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn err_kind(r: &Json) -> String {
    // The JSON-RPC error nests the closed-sum member under `data.kind`.
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .or_else(|| {
            r.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Json::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn hello_experimental(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([("experimental", Json::Bool(true))]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

#[cfg(feature = "tier-c4")]
fn spec_json() -> Json {
    let mut triggers = BTreeMap::new();
    triggers.insert(
        "observe".to_string(),
        Json::obj([
            ("name", Json::str("observe")),
            (
                "trigger",
                Json::obj([
                    ("type", Json::str("external")),
                    ("kind", Json::str("ticket.updated")),
                ]),
            ),
            (
                "policy",
                Json::obj([
                    ("delivery_mode", Json::str("follow_up")),
                    ("coalesce", Json::str("none")),
                    ("max_pending", Json::Int(16)),
                ]),
            ),
        ]),
    );
    Json::obj([
        ("schema", Json::str("hh.fleet.spec/1")),
        ("version", Json::str("1")),
        ("name", Json::str("triage")),
        ("purpose", Json::str("fleet_activation")),
        ("fixture_ref", Json::str("fixture:triage")),
        (
            "agents",
            Json::Arr(
                ["alice", "bob", "ops"]
                    .iter()
                    .map(|a| Json::str(*a))
                    .collect(),
            ),
        ),
        (
            "capacity",
            Json::obj([("activate_run", Json::Int(8)), ("items", Json::Int(64))]),
        ),
        ("ownership", Json::obj([("alice", Json::str("ops"))])),
        ("policy_ref", Json::str("policy:test")),
        ("narrowing", Json::Arr(vec![])),
        ("budget", Json::Null),
        ("budget_ref", Json::Null),
        ("out_of_scope", Json::Bool(true)),
        ("triggers", Json::Obj(triggers)),
        (
            "defaults",
            Json::obj([
                ("max_retries", Json::Int(3)),
                ("retry_backoff_ms", Json::Int(60_000)),
                ("dispatch_lease_ms", Json::Int(300_000)),
                ("stall_timeout_ms", Json::Int(300_000)),
            ]),
        ),
        (
            "human_gate_states",
            Json::Arr(vec![Json::str("needs_human")]),
        ),
    ])
}

#[cfg(feature = "tier-c4")]
fn fixture_json(item: Json) -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("occ-1")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("ticket.updated")),
                ("item", item),
                ("observed_at_ms", Json::Int(1_100)),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
        ("activate_run", Json::Arr(vec![Json::str("i1")])),
    ])
}

#[cfg(feature = "tier-c4")]
fn item_json(id: &str, owner: &str) -> Json {
    Json::obj([
        ("item_id", Json::str(id)),
        ("title", Json::str(format!("ticket {id}"))),
        (
            "source",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("kind", Json::str("ticket")),
                ("source_ref", Json::str(format!("src:{id}"))),
            ]),
        ),
        ("idempotency_key", Json::str(format!("idem:{id}"))),
        ("owner", Json::str(owner)),
    ])
}

/// The boundary is experimental-gated like every Group L op.
#[test]
fn fleet_ops_require_experimental_optin() {
    let mut svc = service();
    // hello without the opt-in.
    let r = call(
        &mut svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(BTreeMap::new())),
        ]),
    );
    assert!(r.get("result").is_some());
    let r = call(
        &mut svc,
        "fleet.list",
        Json::obj([("run", Json::str("fleet-x"))]),
    );
    assert_eq!(err_kind(&r), "ExperimentalRequired", "{r:?}");
}

/// E2E over the fixture: open → reconcile → settle → accountability →
/// fleet_view — records in, records out (AC-R-2.12.6's boundary leg).
#[cfg(feature = "tier-c4")]
#[test]
fn fleet_surface_end_to_end() {
    let mut svc = service();
    hello_experimental(&mut svc);

    // open — the activation run is returned; the spec ref is durable.
    let r = call(&mut svc, "fleet.open", Json::obj([("spec", spec_json())]));
    let run = r
        .get("result")
        .and_then(|v| v.get("run_id"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("fleet.open: {r:?}"))
        .to_string();
    assert!(run.starts_with("fleet-"));

    // reconcile over the pinned fixture — the occurrence admits + the
    // fixture's `activate_run` set dispatches (RC-2 mark lands).
    let r = call(
        &mut svc,
        "fleet.reconcile",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", fixture_json(item_json("i1", "alice"))),
            ("now_ms", Json::Int(1_100)),
        ]),
    );
    let dispatched: Vec<Json> = match r.get("result").and_then(|v| v.get("dispatched")) {
        Some(Json::Arr(a)) => a.clone(),
        _ => Vec::new(),
    };
    assert_eq!(
        dispatched,
        vec![Json::str("i1")],
        "reconcile must dispatch the runnable item: {r:?}"
    );

    // work_item read — folded `dispatching` state.
    let r = call(
        &mut svc,
        "fleet.work_item",
        Json::obj([("run", Json::str(&run)), ("item", Json::str("i1"))]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|v| v.get("state"))
            .and_then(Json::as_str),
        Some("dispatching"),
        "{r:?}"
    );

    // transfer ownership — the ack gate (dispatch post-handoff requires
    // the new owner's acknowledgement).
    let r = call(
        &mut svc,
        "fleet.transfer_owner",
        Json::obj([
            ("run", Json::str(&run)),
            ("item", Json::str("i1")),
            ("to", Json::str("bob")),
            ("basis", Json::str("policy_rule")),
        ]),
    );
    assert!(r.get("result").is_some(), "transfer_owner: {r:?}");
    let r = call(
        &mut svc,
        "fleet.acknowledge_owner",
        Json::obj([
            ("run", Json::str(&run)),
            ("item", Json::str("i1")),
            ("agent", Json::str("bob")),
        ]),
    );
    assert!(r.get("result").is_some(), "acknowledge_owner: {r:?}");

    // settle — the terminal row.
    let r = call(
        &mut svc,
        "fleet.settle",
        Json::obj([
            ("run", Json::str(&run)),
            ("item", Json::str("i1")),
            ("outcome", Json::str("completed")),
            ("evidence_refs", Json::Arr(vec![Json::str("ev:1")])),
        ]),
    );
    assert!(r.get("result").is_some(), "settle: {r:?}");

    // The projection + accountability records render (records out).
    for op in [
        "fleet.fleet_view",
        "fleet.accountability_record",
        "fleet.audit_link",
        "fleet.state_map",
        "fleet.list",
    ] {
        let r = call(&mut svc, op, Json::obj([("run", Json::str(&run))]));
        assert!(r.get("result").is_some(), "{op}: {r:?}");
    }

    // Typed refusal — an unknown run names no activation.
    let r = call(
        &mut svc,
        "fleet.work_item",
        Json::obj([("run", Json::str("fleet-nope")), ("item", Json::str("i1"))]),
    );
    let kind = err_kind(&r);
    assert!(
        kind == "UnknownRun" || kind == "Refused",
        "unknown run must be a typed refusal: {r:?}"
    );
}

/// The `restore` boundary — service restart re-folds (AC's restore leg
/// through the boundary).
#[cfg(feature = "tier-c4")]
#[test]
fn fleet_restore_through_boundary() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let r = call(&mut svc, "fleet.open", Json::obj([("spec", spec_json())]));
    let run = r
        .get("result")
        .and_then(|v| v.get("run_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let r = call(
        &mut svc,
        "fleet.reconcile",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", fixture_json(item_json("i1", "alice"))),
            ("now_ms", Json::Int(1_100)),
        ]),
    );
    assert!(r.get("result").is_some());
    // Restore — the lease's live holder is this service's engine map;
    // `fleet.restore` in the same service is a re-ensure under the held
    // lease (WouldBlock is honest — a second PROCESS can't fence it).
    let r = call(
        &mut svc,
        "fleet.restore",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", fixture_json(item_json("i1", "alice"))),
            ("now_ms", Json::Int(1_200)),
        ]),
    );
    assert!(
        r.get("result").is_some() || err_kind(&r) == "WouldBlock",
        "restore through the boundary: {r:?}"
    );
}

/// Tier absent — the ops stay in the schema and answer the typed
/// `tier_unavailable` refusal (never silent degrade; CC6).
#[cfg(not(feature = "tier-c4"))]
#[test]
fn fleet_ops_refuse_tier_unavailable() {
    let mut svc = service();
    hello_experimental(&mut svc);
    for op in [
        "fleet.open",
        "fleet.reconcile",
        "fleet.list",
        "fleet.fleet_view",
        "fleet.webhook_ingress",
        "fleet.source_capabilities",
        "fleet.source_records",
    ] {
        let r = call(&mut svc, op, Json::obj([("run", Json::str("x"))]));
        assert_eq!(
            err_kind(&r),
            "Unsupported",
            "{op} must refuse Unsupported{{by:tier-c4}}: {r:?}"
        );
        assert_eq!(
            r.get("error")
                .and_then(|e| e.get("data"))
                .and_then(|d| d.get("by"))
                .and_then(Json::as_str),
            Some("tier-c4"),
            "{op}: {r:?}"
        );
    }
}

/// S5.6 — the adapter surface through the boundary (AC-R-2.12.6-9's
/// embed leg): signed-webhook ingress → reconcile admits; the capability
/// probe + source read minimum; FleetView's `metrics` member.
#[cfg(feature = "tier-c4")]
#[test]
fn fleet_adapter_surface_through_the_boundary() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let r = call(&mut svc, "fleet.open", Json::obj([("spec", spec_json())]));
    let run = r
        .get("result")
        .and_then(|r| r.get("run_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // The `hh.fleet.webhook_source/1` doc — `webhook` is the IngressPolicy
    // verbatim; `key_ref` names the broker coordinate (never the bytes).
    let source = Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook_source/1")),
        (
            "webhook",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("key_ref", Json::str("vault:hh/test/webhook")),
                ("replay_window_ms", Json::Int(3_600_000)),
                ("volatile_fields", Json::Arr(vec![Json::str("etag")])),
                ("external_kind", Json::str("ticket.updated")),
            ]),
        ),
        (
            "records",
            Json::Arr(vec![Json::obj([
                ("native_id", Json::str("T-1")),
                ("state", Json::str("open")),
            ])]),
        ),
    ]);
    // A signed delivery — `hmac-sha256:<hex>` over the canonical form;
    // the boundary hands the broker-resolved key (verification only).
    let key = b"boundary-webhook-key";
    let delivery = Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook/1")),
        ("delivery_id", Json::str("d-1")),
        ("item", item_json("i1", "alice")),
    ]);
    let sig = format!(
        "hmac-sha256:{}",
        hharness_wire::sha256::hmac_sha256_hex(key, delivery.to_canonical_string().as_bytes())
    );
    let r = call(
        &mut svc,
        "fleet.webhook_ingress",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", source.clone()),
            ("delivery", delivery.clone()),
            ("signature", Json::str(&sig)),
            ("key", Json::str(std::str::from_utf8(key).unwrap())),
        ]),
    );
    let status = r
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    assert_eq!(status, "received", "webhook_ingress: {r:?}");
    // A bad signature refuses — closed fail set, never a silent drop.
    let r = call(
        &mut svc,
        "fleet.webhook_ingress",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", source.clone()),
            ("delivery", delivery.clone()),
            ("signature", Json::str("hmac-sha256:deadbeef")),
            ("key", Json::str(std::str::from_utf8(key).unwrap())),
        ]),
    );
    assert_eq!(err_kind(&r), "Refused", "bad signature must refuse: {r:?}");
    // The capability probe + source records through the boundary —
    // `push_delivery_id` declared, the records member verbatim.
    let r = call(
        &mut svc,
        "fleet.source_capabilities",
        Json::obj([("run", Json::str(&run)), ("source", source.clone())]),
    );
    let caps = r.get("result").and_then(|r| r.get("capabilities"));
    assert_eq!(
        caps.and_then(|c| c.get("push_delivery_id"))
            .and_then(Json::as_str),
        Some("declared"),
        "capabilities: {r:?}"
    );
    let r = call(
        &mut svc,
        "fleet.source_records",
        Json::obj([("run", Json::str(&run)), ("source", source.clone())]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("records"))
            .and_then(|rs| rs.as_str().map(|_| ()))
            .or_else(|| { r.get("result").and_then(|r| r.get("records")).map(|_| ()) })
            .map(|_| ()),
        Some(()),
        "source_records: {r:?}"
    );
    // The signed delivery lands in the activation on the next reconcile —
    // the WebhookAdapter the boundary built holds the queued occurrence;
    // a second `webhook_ingress` carrying the same delivery is a
    // `duplicate` label (process-side; the durable skip audits at fold).
    let r = call(
        &mut svc,
        "fleet.webhook_ingress",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", source.clone()),
            ("delivery", delivery),
            ("signature", Json::str(&sig)),
            ("key", Json::str(std::str::from_utf8(key).unwrap())),
        ]),
    );
    // Note: each `fleet.*` call rebuilds the adapter from the pinned doc —
    // a repeated delivery is fresh at the process boundary; the durable
    // dedup is the ledger's (`occurrence_id` namespacing keeps it so).
    let status2 = r
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(status2 == "received" || status2 == "duplicate", "{r:?}");

    // `fleet.fleet_view` carries the S5.6 `metrics` member — the
    // catalogue's fleet rows in `MetricDeclaration` form.
    let r = call(
        &mut svc,
        "fleet.fleet_view",
        Json::obj([("run", Json::str(&run))]),
    );
    let metrics = r
        .get("result")
        .and_then(|v| v.get("metrics"))
        .cloned()
        .unwrap_or(Json::Null);
    let names: Vec<String> = match &metrics {
        Json::Arr(a) => a
            .iter()
            .filter_map(|d| d.get("name").and_then(Json::as_str).map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    assert!(
        names.contains(&"unowned_dispatch_count".to_string())
            && names.contains(&"reconcile_latency_ms".to_string()),
        "fleet_view.metrics missing fleet declarations: {names:?}"
    );
}

/// R4.7 — the GitHub `X-Hub-Signature-256` lane through the boundary
/// (`delivery_raw` + `event`/`delivery_id` + the verbatim `sha256=<hex>`
/// header). A verified delivery is *admitted* durably inside the op
/// (`observe` folds it); the next `fleet.reconcile` turns the cue into
/// the WorkItem. Replay and stale-window legs ride the durable fold.
#[cfg(feature = "tier-c4")]
#[test]
fn fleet_github_webhook_lane_through_the_boundary() {
    let mut svc = service();
    hello_experimental(&mut svc);
    // The activation's external trigger covers `github.issue`.
    let mut spec = spec_json();
    if let Json::Obj(ref mut s) = spec {
        if let Json::Obj(ref mut t) = s.get_mut("triggers").unwrap() {
            if let Json::Obj(ref mut tr) = t.get_mut("observe").unwrap() {
                if let Json::Obj(ref mut tg) = tr.get_mut("trigger").unwrap() {
                    tg.insert("kind".into(), Json::str("github.issue"));
                }
            }
        }
    }
    let r = call(&mut svc, "fleet.open", Json::obj([("spec", spec)]));
    let run = r
        .get("result")
        .and_then(|r| r.get("run_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // `webhook` is the shared IngressPolicy; `github` arms the raw lane.
    let source = Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook_source/1")),
        (
            "webhook",
            Json::obj([
                ("source_id", Json::str("github")),
                ("key_ref", Json::str("broker:github/webhook")),
                ("external_kind", Json::str("github.issue")),
            ]),
        ),
        (
            "github",
            Json::obj([
                ("repo", Json::str("acme/sandbox")),
                ("source_id", Json::str("github")),
                ("owner_map", Json::obj([("octocat", Json::str("alice"))])),
                ("owner_default", Json::str("ops")),
            ]),
        ),
    ]);
    let key = b"boundary-github-key";
    let raw = Json::obj([
        ("action", Json::str("opened")),
        (
            "issue",
            Json::obj([
                ("number", Json::Int(9)),
                ("title", Json::str("live issue")),
                ("state", Json::str("open")),
                (
                    "html_url",
                    Json::str("https://github.com/acme/sandbox/issues/9"),
                ),
                ("assignee", Json::obj([("login", Json::str("octocat"))])),
            ]),
        ),
        (
            "repository",
            Json::obj([("full_name", Json::str("acme/sandbox"))]),
        ),
    ])
    .to_canonical_string();
    let sig = format!(
        "sha256={}",
        hharness_wire::sha256::hmac_sha256_hex(key, raw.as_bytes())
    );
    let ingress = |svc: &mut EmbedService, raw: &str, sig: &str, did: &str| {
        call(
            svc,
            "fleet.webhook_ingress",
            Json::obj([
                ("run", Json::str(&run)),
                ("source", source.clone()),
                ("delivery_raw", Json::str(raw)),
                ("signature", Json::str(sig)),
                ("delivery_id", Json::str(did)),
                ("event", Json::str("issues")),
                ("key", Json::str(std::str::from_utf8(key).unwrap())),
            ]),
        )
    };
    let r = ingress(&mut svc, &raw, &sig, "gh-d-9");
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("status"))
            .and_then(Json::as_str),
        Some("received"),
        "github ingress: {r:?}"
    );
    // The verified occurrence is admitted durably inside the op.
    let observed = r
        .get("result")
        .and_then(|r| r.get("observed"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    assert_eq!(
        match &observed {
            Json::Arr(a) => a.len(),
            _ => 0,
        },
        1,
        "the delivery must fold durable at ingress: {r:?}"
    );

    // `fleet.reconcile` folds the cue → the WorkItem exists.
    let r = call(
        &mut svc,
        "fleet.reconcile",
        Json::obj([("run", Json::str(&run)), ("source", source.clone())]),
    );
    assert!(r.get("result").is_some(), "reconcile: {r:?}");
    let r = call(
        &mut svc,
        "fleet.work_item",
        Json::obj([
            ("run", Json::str(&run)),
            ("item", Json::str("gh-acme-sandbox-9")),
        ]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("title"))
            .and_then(Json::as_str),
        Some("live issue"),
        "the delivery's item must admit: {r:?}"
    );
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("owner"))
            .and_then(Json::as_str),
        Some("alice")
    );

    // Replay — same delivery id: the durable fold audits the skip (the
    // op still answers `received` — the per-call window table is fresh;
    // `observed` is empty because the fold names it a duplicate).
    let r = ingress(&mut svc, &raw, &sig, "gh-d-9");
    let observed = r
        .get("result")
        .and_then(|r| r.get("observed"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    assert_eq!(
        match &observed {
            Json::Arr(a) => a.len(),
            _ => 99,
        },
        0,
        "a replayed delivery id observes nothing new: {r:?}"
    );

    // Stale window — a fresh delivery id with identical content: the
    // occurrence is durable-new at ingress (`observed` carries it —
    // recorded, never dropped) but the item's content-hash idem key
    // admits `known` — `fleet.list` still one item.
    let r = ingress(&mut svc, &raw, &sig, "gh-d-10");
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("status"))
            .and_then(Json::as_str),
        Some("received"),
        "stale ingress: {r:?}"
    );
    let observed = r
        .get("result")
        .and_then(|r| r.get("observed"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    assert_eq!(
        match &observed {
            Json::Arr(a) => a.len(),
            _ => 99,
        },
        1,
        "the stale delivery's occurrence is recorded: {r:?}"
    );
    let r = call(
        &mut svc,
        "fleet.reconcile",
        Json::obj([("run", Json::str(&run)), ("source", source.clone())]),
    );
    let items = r
        .get("result")
        .and_then(|r| r.get("observed"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    assert_eq!(
        match &items {
            Json::Arr(a) => a.len(),
            _ => 99,
        },
        0,
        "ingress already folded the occurrence: {r:?}"
    );
    let r = call(
        &mut svc,
        "fleet.list",
        Json::obj([("run", Json::str(&run))]),
    );
    let items = r
        .get("result")
        .and_then(|r| r.get("items"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    assert_eq!(
        match &items {
            Json::Arr(a) => a.len(),
            _ => 99,
        },
        1,
        "stale content admits `known` — no second item: {r:?}"
    );

    // A bad `sha256=` refuses typed; a `sha1=` header refuses the scheme.
    let bad = format!(
        "sha256={}",
        hharness_wire::sha256::hmac_sha256_hex(b"wrong", raw.as_bytes())
    );
    assert_eq!(
        err_kind(&ingress(&mut svc, &raw, &bad, "gh-d-11")),
        "Refused"
    );
    let sha1 = format!(
        "sha1={}",
        hharness_wire::sha256::hmac_sha256_hex(key, raw.as_bytes())
    );
    assert_eq!(
        err_kind(&ingress(&mut svc, &raw, &sha1, "gh-d-12")),
        "Refused"
    );
    // The canonical lane still works on the same doc (`delivery` member).
    let delivery = Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook/1")),
        ("delivery_id", Json::str("d-canon-1")),
        (
            "item",
            Json::obj([
                ("item_id", Json::str("canon-1")),
                ("title", Json::str("canonical delivery")),
                (
                    "source",
                    Json::obj([
                        ("source_id", Json::str("github")),
                        ("kind", Json::str("ticket")),
                    ]),
                ),
                ("idempotency_key", Json::str("idem:canon-1")),
                ("owner", Json::str("alice")),
            ]),
        ),
    ]);
    let csig = format!(
        "hmac-sha256:{}",
        hharness_wire::sha256::hmac_sha256_hex(key, delivery.to_canonical_string().as_bytes())
    );
    let r = call(
        &mut svc,
        "fleet.webhook_ingress",
        Json::obj([
            ("run", Json::str(&run)),
            ("source", source.clone()),
            ("delivery", delivery),
            ("signature", Json::str(&csig)),
            ("key", Json::str(std::str::from_utf8(key).unwrap())),
        ]),
    );
    assert_eq!(
        r.get("result")
            .and_then(|r| r.get("status"))
            .and_then(Json::as_str),
        Some("received"),
        "canonical lane on the github doc: {r:?}"
    );
}
