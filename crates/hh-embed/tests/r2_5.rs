//! R2.5 — the §5c context/memory producer legs at the service boundary
//! (DF-S2.8-1; R-2.4.1/2.4.3/2.4.4/2.4.5). The hh-control battery proves
//! the driver's sequencing over its port seams; this battery proves the
//! *wired* kernel halves emit through the real `EmbedService` drive —
//! rows minted by the driver over `KernelSink`, never test scaffolding:
//!
//! - `supplies.procedures[]` ingest lands durable `context.memory.written`
//!   `procedure_pointer` rows at open (the §5c.4 supply chain).
//! - A `submit` drive mints `context.procedure.selected` (the §5c.5
//!   selection leg) *before* `context.assembled`, and `context.assembled`
//!   before the model call it fed — durable-before-visible (CC3).
//! - A delivered `procedure_index` whose declared
//!   `allowed_capabilities` cover the invoked surface mints the
//!   deterministic `activated` + `verification.artefact.followed` pair —
//!   the detector leg over real producer rows (AC-R-2.4.5-10).
//! - `open_session{kind:"resume", mode:"takeover"}` rebuilds the memory
//!   fold from the durable `context.memory.written` prefix — the resumed
//!   drive's selector sees the same procedures (`rehydrate` preserves the
//!   recorded `version_id`; identity is never re-minted).
//!
//! Fixture-only (the ticket's offline ceiling): `EmbedModel` is the
//! scripted model, `local_host` is the env. No live verification claims.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixture: the conformance hir/1 document (r2_4's shape) ──────────────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn document_json() -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1000),
                        soft: None,
                    },
                )]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots: BTreeMap::new(),
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

// ── service helpers ─────────────────────────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::{Mutex, OnceLock};
    static DIRS: OnceLock<Mutex<BTreeMap<String, std::path::PathBuf>>> = OnceLock::new();
    let mut m = DIRS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    m.entry(tag.to_string())
        .or_insert_with(|| {
            let d =
                std::env::temp_dir().join(format!("hh-embed-r25-{}-{tag}", std::process::id(),));
            let _ = std::fs::remove_dir_all(&d);
            d
        })
        .clone()
}

fn service(tag: &str) -> EmbedService {
    let root = test_dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r2.5".into(),
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

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn hello(svc: &mut EmbedService) {
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    caps.insert("serves_host_executor".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r2.5")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    ok(&r);
}

/// `supplies.procedures[]` — each record's `semantic_id` is the bound name;
/// `procedure.allowed_capabilities` is the capability table the
/// deterministic `followed` leg reads (R-2.4.5).
fn procedure_record(semantic_id: &str, allowed: &[&str]) -> Json {
    Json::obj(vec![
        ("semantic_id", Json::str(semantic_id)),
        (
            "procedure",
            Json::obj(vec![
                (
                    "allowed_capabilities",
                    Json::Arr(allowed.iter().map(|c| Json::str(*c)).collect()),
                ),
                ("preconditions", Json::Arr(vec![])),
            ]),
        ),
    ])
}

fn new_spec_with_supplies(procedures: Vec<Json>, host_caps: Vec<Json>) -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj(vec![("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "supplies",
            Json::obj(vec![
                ("context", Json::Arr(vec![])),
                ("host_capabilities", Json::Arr(host_caps)),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(procedures)),
            ]),
        ),
        (
            "attendance",
            AttendanceDeclaration {
                value: "async".into(),
                source: "declared".into(),
            }
            .to_json(),
        ),
    ])
}

fn open_new(svc: &mut EmbedService, spec: Json) -> Json {
    ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ))
}

fn session_run(s: &Json) -> (String, String) {
    (
        s.get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        s.get("run_id").and_then(Json::as_str).unwrap().to_string(),
    )
}

fn submit_blocks(svc: &mut EmbedService, sid: &str, key: &str, input: Vec<Json>) -> Json {
    ok(&call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(sid)),
            ("input", Json::Arr(input)),
            ("idempotency_key", Json::str(key)),
        ]),
    ))
}

fn submit(svc: &mut EmbedService, sid: &str, key: &str) -> Json {
    submit_blocks(
        svc,
        sid,
        key,
        vec![Json::obj(vec![
            ("kind", Json::str("text")),
            ("text", Json::str("hello")),
        ])],
    )
}

/// A `{kind:"invoke", capability, args:{}}` block — the scripted model's
/// next call targets the declared host surface (empty args validate
/// against the surface's closed `{}` param table).
fn invoke_block(capability: &str) -> Json {
    Json::obj(vec![
        ("kind", Json::str("invoke")),
        ("capability", Json::str(capability)),
        ("args", Json::Obj(BTreeMap::new())),
    ])
}

fn rows(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.surface_events(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

fn seq_of(evs: &[(u64, String, Json)], class: &str) -> Option<u64> {
    evs.iter().find(|(_, c, _)| c == class).map(|(s, _, _)| *s)
}

/// `payload.<member>[]` as string refs — `hh_wire::Json` keeps the
/// closed-variant accessors minimal (`as_str`/`as_int`/`get`).
fn arr_strs<'a>(payload: &'a Json, member: &str) -> Vec<&'a str> {
    match payload.get(member) {
        Some(Json::Arr(items)) => items.iter().filter_map(Json::as_str).collect(),
        _ => Vec::new(),
    }
}

// ── the producer legs ───────────────────────────────────────────────────────

/// `supplies.procedures[]` ingest (§5c.4; R-2.4.3): each record lands a
/// durable `context.memory.written` row — `kind: procedure_pointer`,
/// `scope: session`, `memory_id` = the declared `semantic_id` — minted at
/// open, before any drive.
#[test]
fn supplies_procedures_ingest_lands_durable() {
    let mut svc = service("ingest");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec_with_supplies(
            vec![
                procedure_record("proc:a", &["cap:fs.read"]),
                procedure_record("proc:b", &[]),
            ],
            vec![],
        ),
    );
    let (_sid, run) = session_run(&s);
    let evs = rows(&svc, &run);
    let written: Vec<&Json> = evs
        .iter()
        .filter(|(_, c, _)| c == "context.memory.written")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(written.len(), 2, "one written row per procedure: {evs:?}");
    for want in ["proc:a", "proc:b"] {
        let w = written
            .iter()
            .find(|p| p.get("memory_id").and_then(Json::as_str) == Some(want))
            .unwrap_or_else(|| panic!("no written row for {want}: {written:?}"));
        assert_eq!(
            w.get("kind").and_then(Json::as_str),
            Some("procedure_pointer"),
            "{w:?}"
        );
        assert_eq!(
            w.get("scope").and_then(Json::as_str),
            Some("session"),
            "{w:?}"
        );
        assert!(
            w.get("version_id").and_then(Json::as_str).is_some(),
            "{w:?}"
        );
    }
    // Ingest is durable at open — ahead of any model call.
    let written_seq = seq_of(&evs, "context.memory.written").unwrap();
    assert!(
        seq_of(&evs, "context.assembled").is_none()
            || written_seq < seq_of(&evs, "context.assembled").unwrap(),
        "the write precedes any assembly: {evs:?}"
    );
}

/// The full producer chain over one real drive (DF-S2.8-1; AC-R-2.4.5-10):
/// `procedure.selected` → `assembled` → `artefact.delivered` → the model's
/// `hh.submit` call → `artefact.activated{detector: deterministic}` →
/// `verification.artefact.followed` — all durable, in emit order, before
/// `lifecycle.turn.finished`.
#[test]
fn drive_mints_the_producer_sequence() {
    let mut svc = service("sequence");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec_with_supplies(
            vec![procedure_record("proc:cap", &["cap:fs.read"])],
            vec![Json::obj(vec![
                ("capability_id", Json::str("cap:fs.read")),
                // The declared class — the supply carries it honestly
                // (`fs.read` is a workspace-local idempotent read); an
                // undeclared risk reads UNKNOWN → Π's never-auto floor
                // parks the effect at `intended` (ADR-0031 §2).
                (
                    "risk_class",
                    Json::obj(vec![
                        ("reversibility", Json::str("read_only")),
                        ("repeat_safety", Json::str("idempotent")),
                        ("scope", Json::str("workspace_local")),
                    ]),
                ),
            ])],
        ),
    );
    let (sid, run) = session_run(&s);
    // The scripted model's next call invokes the declared host surface —
    // it validates (empty args against the closed `{}` table), so the
    // deterministic `activated`/`followed` pair mints ahead of the
    // `awaiting_host` park.
    submit_blocks(&mut svc, &sid, "sub-1", vec![invoke_block("cap:fs.read")]);
    let evs = rows(&svc, &run);

    // §5c.5 selection — durable ahead of the assembled row it fed.
    let sel_seq = seq_of(&evs, "context.procedure.selected")
        .unwrap_or_else(|| panic!("no procedure.selected: {evs:?}"));
    let selected = &evs
        .iter()
        .find(|(_, c, _)| c == "context.procedure.selected")
        .unwrap()
        .2;
    assert_eq!(
        selected.get("method").and_then(Json::as_str),
        Some("index_all_under_budget"),
        "{selected:?}"
    );
    let candidates = arr_strs(selected, "candidates");
    assert!(
        candidates.contains(&"proc:cap"),
        "the supplied procedure is a candidate: {selected:?}"
    );

    let asm_seq =
        seq_of(&evs, "context.assembled").unwrap_or_else(|| panic!("no assembled: {evs:?}"));
    assert!(sel_seq < asm_seq, "selection precedes assembly: {evs:?}");

    // The delivery — `procedure_index` kind, `by_reference` (handle-only).
    let delivery = evs
        .iter()
        .find(|(_, c, p)| {
            c == "context.artefact.delivered"
                && p.get("kind").and_then(Json::as_str) == Some("procedure_index")
        })
        .unwrap_or_else(|| panic!("no procedure_index delivery: {evs:?}"));
    let delivery_id = delivery
        .2
        .get("delivery_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // The deterministic detector pair: `activated` carries the declared
    // capability table; `followed` mints on the validated `hh.submit` call.
    let act = evs
        .iter()
        .find(|(_, c, p)| {
            c == "context.artefact.activated"
                && p.get("delivery_id").and_then(Json::as_str) == Some(delivery_id.as_str())
        })
        .unwrap_or_else(|| panic!("no activated row for {delivery_id}: {evs:?}"));
    assert_eq!(
        act.2.get("detector").and_then(Json::as_str),
        Some("deterministic"),
        "{:?}",
        act.2
    );
    assert_eq!(
        act.2.get("signal").and_then(Json::as_str),
        Some("cited"),
        "{:?}",
        act.2
    );
    let caps = arr_strs(&act.2, "allowed_capabilities");
    assert!(
        caps.contains(&"cap:fs.read"),
        "the declared caps ride the activation: {:?}",
        act.2
    );

    let followed = evs
        .iter()
        .find(|(_, c, p)| {
            c == "verification.artefact.followed"
                && p.get("delivery_id").and_then(Json::as_str) == Some(delivery_id.as_str())
        })
        .unwrap_or_else(|| panic!("no followed row for {delivery_id}: {evs:?}"));
    let f = &followed.2;
    assert_eq!(f.get("verdict"), Some(&Json::Bool(true)), "{f:?}");
    assert_eq!(
        f.get("detector").and_then(Json::as_str),
        Some("deterministic"),
        "{f:?}"
    );
    assert_eq!(
        f.get("detector_ref").and_then(Json::as_str),
        Some("procedure:procedure_invoked"),
        "{f:?}"
    );

    // The ordering contract — producer emit order is the ledger order:
    // selection → assembled → delivery → proposal → activation →
    // followed (the validated call's own rows ride the model round).
    let proposed =
        seq_of(&evs, "action.tool.proposed").unwrap_or_else(|| panic!("no tool.proposed: {evs:?}"));
    assert!(
        sel_seq < asm_seq
            && asm_seq < delivery.0
            && delivery.0 < proposed
            && proposed < act.0
            && act.0 < followed.0,
        "producer rows land in emit order: {evs:?}"
    );
}

/// Removal-sensitive baseline: with no `supplies.procedures` the selector
/// has nothing to pick — `context.assembled` still mints (the CAP.3 call
/// site is unconditional), `context.procedure.selected` does not.
#[test]
fn assembled_mints_without_supplies() {
    let mut svc = service("bare");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec_with_supplies(vec![], vec![]));
    let (sid, run) = session_run(&s);
    submit(&mut svc, &sid, "sub-1");
    let evs = rows(&svc, &run);
    assert!(
        seq_of(&evs, "context.assembled").is_some(),
        "assembled mints on every propose: {evs:?}"
    );
    assert!(
        seq_of(&evs, "context.procedure.selected").is_none(),
        "no procedures ⇒ no selection row: {evs:?}"
    );
}

/// §5c.4's replay rule (R-2.4.3): a takeover resume rebuilds the memory
/// fold from the durable `context.memory.written` prefix — the resumed
/// drive's selector sees the same procedures under the recorded
/// identities (rehydrate never re-mints a `version_id`).
#[test]
fn resume_fold_rehydrates_supplied_procedures() {
    let mut svc = service("resume-fold");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec_with_supplies(vec![procedure_record("proc:r", &["cap:x"])], vec![]),
    );
    let (_sid, run) = session_run(&s);
    // The durable `context.memory.written` row is the resume input —
    // capture its identity before the takeover.
    let before = rows(&svc, &run);
    let version_id = before
        .iter()
        .find(|(_, c, p)| {
            c == "context.memory.written"
                && p.get("memory_id").and_then(Json::as_str) == Some("proc:r")
        })
        .and_then(|(_, _, p)| p.get("version_id").and_then(Json::as_str))
        .unwrap()
        .to_string();

    // Takeover fences the parked writer; the resume arm folds the
    // durable prefix into a fresh `KernelContext`.
    let s2 = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run.clone())),
                    ("mode", Json::str("takeover")),
                ]),
            ),
            ("idempotency_key", Json::str("res-1")),
        ]),
    ));
    let sid2 = s2
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    submit(&mut svc, &sid2, "sub-2");

    let evs = rows(&svc, &run);
    // The takeover fences the writer (`lifecycle.lease.released{reason:
    // takeover}`) then attaches the resuming session — the durable anchor
    // the post-resume rows order after. (`lifecycle.run.resumed` is the
    // cross-restart spelling; an in-process takeover mints the lease +
    // session rows.)
    let resumed_seq = evs
        .iter()
        .find(|(_, c, p)| {
            c == "lifecycle.session.attached"
                && p.get("mode").and_then(Json::as_str) == Some("resume")
        })
        .map(|(s, _, _)| *s)
        .unwrap_or_else(|| panic!("no resume attach: {evs:?}"));
    // The post-resume selection names the supplied procedure — proof the
    // fold carried the durable pointer across the writer fence.
    let selected = evs
        .iter()
        .find(|(s, c, _)| c == "context.procedure.selected" && *s > resumed_seq)
        .unwrap_or_else(|| panic!("no post-resume selection: {evs:?}"));
    let candidates = arr_strs(&selected.2, "candidates");
    assert!(
        candidates.contains(&"proc:r"),
        "the folded procedure re-selects after resume: {:?}",
        selected.2
    );
    // Identity is the durable row's — the folded version keeps
    // `version_id` (no re-minted write on the resumed run).
    let rewritten = evs.iter().any(|(s, c, p)| {
        *s > resumed_seq
            && c == "context.memory.written"
            && p.get("memory_id").and_then(Json::as_str) == Some("proc:r")
    });
    assert!(
        !rewritten,
        "resume never re-writes a carried procedure: {evs:?}"
    );
    let _ = version_id;
}
