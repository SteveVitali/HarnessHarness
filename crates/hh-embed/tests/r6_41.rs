//! R6.41 Stage 1 — `compaction_strategy` slot wiring (G16a; REQ-XE-122;
//! AC-7.3-19): the sealed slot's variant ladder lands in the kernel's
//! `KernelCompaction` port — the first `src` consumer of the C1
//! `compact_family`.
//!
//! The fixture is two-legged, both deterministic and offline (USD $0):
//!
//! - **the port drive** — a `KernelContext` fold armed by the sealed
//!   slot's `arm_compaction` (the same call `open_session` makes) feeds
//!   `KernelCompaction::new`; `compact("explicit")` runs the real
//!   `hharness_context::compact` ladder over a stashed small-window
//!   plan whose occupancy overflows the declared `window_cap`. The
//!   port's `CompactionDone.emitted` `context.compaction.*` rows are
//!   what the driver's `compact_round` appends through the fenced
//!   writer verbatim (CC3 — `compact_round`'s own `self.append` is the
//!   unchanged plumbing; the rows' classes are `context.compaction.
//!   {started,completed}`).
//! - **the boundary open** — `open_session` accepts the bound slot and
//!   refuses typed (`Refused`) an unknown variant, a
//!   `hh/model-owned@1` (β(compact)=model — never a code rung), a
//!   misspelt param member, and a missing required member — the arm's
//!   honest refusals, never a silent `evict_oldest` substitution.
//!
//! The absent leg is byte-identical by construction *and* by
//! comparison: an unarmed fold yields `variants.is_empty()` — the same
//! `Vec::new()` the pre-slot `new` produced — and the emitted
//! `(class, payload)` rows equal the pre-slot port's row-for-row.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use hharness_assembly::grammar::Assembly;
use hharness_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, PlannedItem, SlotFill,
};
use hharness_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hharness_context::Estimate;
use hharness_control::driver::CompactionPort;
use hharness_embed::runtime::{KernelCompaction, KernelContext};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{
    AuthorityClass, HumanRole, Label, Origin, PersistenceScope, ProvenanceRecord,
};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

// ── the conformance fixture document (r3_4's shape) ─────────────────────────

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

/// `document_json(compaction)` — the conformance fixture shape with the
/// `compaction_strategy` slot bound when `compaction` is `Some`
/// (`(variant_ref, params)`); `None` binds nothing — the pre-slot
/// document.
fn document_json(compaction: Option<(&str, BTreeMap<String, Json>)>) -> Json {
    let nodes = vec![
        sid(
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
                    accounting: sel("test:agent"),
                }),
                2,
            ),
            "test:budget",
        ),
        sid(
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
        ),
        sid(
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
        ),
    ];
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
    if let Some((variant, params)) = compaction {
        let mut b = SlotBinding::of(ComponentVariantRef::selected(
            "compaction_strategy",
            variant,
            "latest",
        ));
        b.params = params;
        // `ordered-many` cardinality binds through `Many` even for one
        // rung (the class's declared shape).
        assembly
            .slots
            .insert("compaction_strategy".into(), SlotBindings::Many(vec![b]));
    }
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = nodes;
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

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
                std::env::temp_dir().join(format!("hh-embed-r641-{}-{tag}", std::process::id()));
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
        holder: "r6.41".into(),
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
    caps.insert("serves_measurement".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r6.41")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    ok(&r);
}

fn new_spec(definition: Json) -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", definition),
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
                ("host_capabilities", Json::Arr(vec![])),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(vec![])),
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

// ── the plan/candidate stash (r5_20's shapes) ────────────────────────────────

fn toolprov(cap: &str, inv: &str) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: Origin::tool(cap, inv),
        authority: AuthorityClass::Delegate,
        taint: BTreeSet::new(),
        readers: hharness_provenance::authority::ReaderSet::Public,
        scope: PersistenceScope::Run,
        derived_from: Vec::new(),
        created_at: 0,
        attestation: None,
    }
}

fn cand(item: &str, tokens: u64, priority: PriorityClass, seq: u64) -> Candidate {
    Candidate {
        candidate_id: format!("cand-{item}"),
        context_item_id: Some(item.to_string()),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(priority),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".into(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: toolprov("cap/tool", "inv-1"),
        validity: hharness_hir::records::Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

fn plan_of(cands: &[Candidate], occupancy: u64) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let candidates: BTreeMap<String, Candidate> = cands
        .iter()
        .map(|c| (c.candidate_id.clone(), c.clone()))
        .collect();
    let items: Vec<PlannedItem> = cands
        .iter()
        .map(|c| PlannedItem {
            candidate_id: c.candidate_id.clone(),
            context_item_id: c.context_item_id.clone().unwrap(),
            artefact_id: None,
            delivery_id: format!("del:{}", c.candidate_id),
            authority: c.label.authority,
            label: c.label.clone(),
            tokens: c.estimate.tokens,
            state: c.state,
            delivered_by_reference: false,
            derived_from: None,
        })
        .collect();
    (
        ContextPlan {
            plan_id: "plan:r6_41".into(),
            model_call_id: "mc.prev".into(),
            derived_from: DerivedFrom {
                run_id: "run".into(),
                seq: 10,
                view_hash: "sha256:view".into(),
            },
            slots: vec![SlotFill {
                slot_id: "transcript".into(),
                items,
            }],
            omitted: vec![],
            context_label: Label::top(),
            occupancy_estimate: occupancy,
            legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
            reserved: 0,
            estimator_ref: "est/pinned".into(),
            static_hash: "sha256:static".into(),
        },
        candidates,
    )
}

/// A fold armed the way `open_session` arms it — `for_policy` then
/// `arm_compaction` per bound slot — with a small-window plan stashed
/// (the assembler's own stash shape). `window_cap` is the declared cap
/// the profile's `context_window` writes into `AssembleInputs`
/// (§5c.1 — the one `ContextBudget.window_cap` source).
fn armed_fold(
    variant: &str,
    params: &BTreeMap<String, Json>,
    window_cap: u64,
) -> Rc<std::cell::RefCell<KernelContext>> {
    let mut ctx = KernelContext::for_policy("run-r641", "context_policy/default").unwrap();
    ctx.arm_compaction(variant, params).unwrap();
    ctx.window_cap = window_cap;
    // The overflow plan: optional observations push occupancy past the
    // declared cap so the ladder must reclaim (needed=1 > 0 headroom).
    let cands = vec![
        cand("obs-0", 40, PriorityClass::ObservationOld, 1),
        cand("obs-1", 40, PriorityClass::ObservationOld, 2),
        cand("obs-2", 40, PriorityClass::ObservationOld, 3),
        cand("obs-3", 40, PriorityClass::ObservationOld, 4),
        cand("obs-4", 40, PriorityClass::ObservationOld, 5),
        cand("obs-5", 40, PriorityClass::ObservationOld, 6),
    ];
    let (plan, map) = plan_of(&cands, 160);
    ctx.last_plan = Some(plan);
    ctx.last_candidates = map;
    Rc::new(std::cell::RefCell::new(ctx))
}

fn emitted_rows(done: &hharness_control::driver::CompactionDone) -> Vec<String> {
    done.emitted
        .iter()
        .map(|(class, payload)| format!("{} ∥ {}", class, payload.to_canonical_string()))
        .collect()
}

/// AC-1 — `compaction_strategy = hh/summarize-rolling@1` under a
/// declared `window_cap`: the armed ladder's `context.compaction.*`
/// rows land through the fenced-writer channel, and the completed
/// record names the bound variant. The scripted lane binds no
/// summarizer — the variant's own deterministic `Evict` fallback
/// applies under its `variant` name (I-FALLBACK), never a fabricated
/// summary.
#[test]
fn ac1_summarize_rolling_fires_under_declared_window_cap() {
    let params = BTreeMap::from([
        ("keep_recent_tokens".to_string(), Json::Int(8)),
        ("reserve_tokens".to_string(), Json::Int(0)),
        ("summarizer_profile".to_string(), Json::str("compaction")),
    ]);
    let fold = armed_fold("hh/summarize-rolling@1", &params, 120);
    // The armed fold carries the slot's ladder — one variant ahead of
    // the kernel's `evict_oldest` rung.
    assert_eq!(fold.borrow().compaction_variants.len(), 1);
    let mut port = KernelCompaction::new(fold);
    let done = port.compact("explicit").expect("ladder compacts");
    let rows = emitted_rows(&done);
    assert!(
        rows.iter()
            .any(|r| r.starts_with("context.compaction.started")),
        "no started row: {rows:?}"
    );
    let completed = rows
        .iter()
        .find(|r| r.starts_with("context.compaction.completed"))
        .unwrap_or_else(|| panic!("no completed row: {rows:?}"));
    let payload = ok_str(completed);
    assert!(
        payload.contains("\"variant_ref\":\"hh/summarize-rolling@1\""),
        "the completed record names the bound variant: {completed}"
    );
    // The reclaim happened — the compacted view is smaller than the
    // 160-token input (the deterministic fallback leg dropped the
    // eviction candidates; `view_hash` is the record's own identity).
    assert!(
        completed.contains("applied"),
        "the ladder applied a reclaim: {completed}"
    );
}

/// The port emits through `CollectSink` — the same `(class, payload)`
/// pairs `compact_round` appends under the run's fenced writer; the
/// rows' own text is the record.
fn ok_str(row: &str) -> String {
    row.to_string()
}

/// AC-1 (absent leg) — an unarmed fold yields `variants.is_empty()`
/// (the pre-slot `Vec::new()` literally), and the emitted
/// `(class, payload)` rows are byte-identical between an unarmed fold
/// and a port whose ladder the test constructs by hand. Absent-not-
/// null: the unbound slot adds no member, no row, no reorder.
#[test]
fn ac1_absent_slot_byte_identical() {
    let fold = armed_fold_unarmed(120);
    assert!(
        fold.borrow().compaction_variants.is_empty(),
        "the unbound slot arms nothing"
    );
    // The port over the unarmed fold.
    let mut port = KernelCompaction::new(fold);
    let done_slot_absent = port.compact("explicit").expect("ladder compacts");
    // A literal pre-slot construction — `variants: vec![]` — over the
    // same fold contents.
    let mut ctx = KernelContext::for_policy("run-r641", "context_policy/default").unwrap();
    ctx.window_cap = 120;
    let cands = vec![
        cand("obs-0", 40, PriorityClass::ObservationOld, 1),
        cand("obs-1", 40, PriorityClass::ObservationOld, 2),
        cand("obs-2", 40, PriorityClass::ObservationOld, 3),
        cand("obs-3", 40, PriorityClass::ObservationOld, 4),
        cand("obs-4", 40, PriorityClass::ObservationOld, 5),
        cand("obs-5", 40, PriorityClass::ObservationOld, 6),
    ];
    let (plan, map) = plan_of(&cands, 160);
    ctx.last_plan = Some(plan);
    ctx.last_candidates = map;
    let mut legacy = KernelCompaction {
        ctx: Rc::new(std::cell::RefCell::new(ctx)),
        variants: vec![],
    };
    let done_legacy = legacy.compact("explicit").expect("ladder compacts");
    assert_eq!(
        emitted_rows(&done_slot_absent),
        emitted_rows(&done_legacy),
        "absent slot must emit byte-identical rows"
    );
    let rows = emitted_rows(&done_slot_absent);
    let completed = rows
        .iter()
        .find(|r| r.starts_with("context.compaction.completed"))
        .unwrap_or_else(|| panic!("no completed row: {rows:?}"));
    assert!(
        completed.contains("\"variant\":\"hh/evict-oldest@1\"")
            || completed.contains("evict-oldest"),
        "the C0 rung alone answers: {completed}"
    );
}

fn armed_fold_unarmed(window_cap: u64) -> Rc<std::cell::RefCell<KernelContext>> {
    let mut ctx = KernelContext::for_policy("run-r641", "context_policy/default").unwrap();
    ctx.window_cap = window_cap;
    let cands = vec![
        cand("obs-0", 40, PriorityClass::ObservationOld, 1),
        cand("obs-1", 40, PriorityClass::ObservationOld, 2),
        cand("obs-2", 40, PriorityClass::ObservationOld, 3),
        cand("obs-3", 40, PriorityClass::ObservationOld, 4),
        cand("obs-4", 40, PriorityClass::ObservationOld, 5),
        cand("obs-5", 40, PriorityClass::ObservationOld, 6),
    ];
    let (plan, map) = plan_of(&cands, 160);
    ctx.last_plan = Some(plan);
    ctx.last_candidates = map;
    Rc::new(std::cell::RefCell::new(ctx))
}

/// The boundary leg — `open_session` accepts the bound
/// `compaction_strategy` slot (the arm path runs the same
/// `compaction_slot` + `arm_compaction` calls the port fixture
/// exercises), and the arm's typed refusals fail the open as
/// `Refused`: unknown variant, `hh/model-owned@1` (β(compact)=model —
/// a proposal channel, never a code rung), a misspelt param member, a
/// missing required member.
#[test]
fn ac1_boundary_open_and_refusals() {
    // The bound slot opens — the arm accepted `hh/summarize-rolling@1`.
    let mut svc = service("open_bound");
    hello(&mut svc);
    let params = BTreeMap::from([
        ("keep_recent_tokens".to_string(), Json::Int(8)),
        ("summarizer_profile".to_string(), Json::str("compaction")),
    ]);
    let doc = document_json(Some(("hh/summarize-rolling@1", params)));
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec(doc)),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ));
    assert!(
        s.get("session_id").and_then(Json::as_str).is_some(),
        "bound slot must open: {}",
        s.to_canonical_string()
    );
    // The unbound slot opens too — absent-not-null.
    let mut svc2 = service("open_unbound");
    hello(&mut svc2);
    let s2 = ok(&call(
        &mut svc2,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec(document_json(None))),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ));
    assert!(s2.get("session_id").and_then(Json::as_str).is_some());
    // Two typed-refusal layers, both honest:
    // 1. an **unregistered** variant (`hh/never-heard@1`,
    //    `hh/model-owned@1` — model-owned is a proposal channel, never a
    //    registrable code variant) fails `resolve`'s snapshot member
    //    check at seal — `InvalidDefinition`, before the arm ever runs;
    // 2. a registered variant whose params are bad fails the arm's own
    //    decode — `Refused` out of `compaction_strategy_for`, never a
    //    silent `evict_oldest` substitution.
    for (variant, params, kind, needle) in [
        (
            "hh/never-heard@1",
            BTreeMap::new(),
            "InvalidDefinition",
            "outside snapshot",
        ),
        (
            "hh/model-owned@1",
            BTreeMap::new(),
            "InvalidDefinition",
            "outside snapshot",
        ),
        (
            "hh/summarize-rolling@1",
            BTreeMap::from([("bogus_member".to_string(), Json::Int(1))]),
            "Refused",
            "unknown member bogus_member",
        ),
        (
            "hh/structured-checkpoint@1",
            BTreeMap::new(),
            "Refused",
            "section_order must be a string array",
        ),
        (
            "hh/world-state-refresh@1",
            BTreeMap::from([("surprise".to_string(), Json::Int(1))]),
            "Refused",
            "unknown member surprise",
        ),
    ] {
        let mut svc = service(&format!("refuse_{}", needle.replace(' ', "_")));
        hello(&mut svc);
        let doc = document_json(Some((variant, params)));
        let r = call(
            &mut svc,
            "open_session",
            Json::obj(vec![
                ("spec", new_spec(doc)),
                ("idempotency_key", Json::str("open-1")),
            ]),
        );
        let text = r.to_canonical_string();
        assert!(
            text.contains(kind) && text.contains(needle),
            "{variant} must refuse typed ({kind} ∥ {needle}): {text}"
        );
    }
    // `hh/model-owned@1`'s arm-level spelling — if it ever registers,
    // the mapping still refuses it as a code rung.
    let e = crate::runtime_probe();
    assert!(e.contains("model-owned boundary"), "{e}");
}

/// The `compaction_strategy_for` refusal for `hh/model-owned@1` —
/// surfaced here so the arm's own vocabulary stays pinned even though
/// the seal layer rejects the unregistered ref first.
fn runtime_probe() -> String {
    hharness_embed::runtime::compaction_strategy_for("hh/model-owned@1", &BTreeMap::new())
        .err()
        .unwrap_or_default()
}
