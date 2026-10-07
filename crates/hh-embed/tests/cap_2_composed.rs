//! CAP.2 — the capstone composed-verification battery
//! (docs/tickets/099_CAP.2__capstone-composed-verification.md; the CAP.1
//! seam hunt `docs/build/CAPSTONE_GAP_ANALYSIS.md` §S1–S8).
//!
//! The additive tickets each proved their own slice. This file drives the
//! *composed* paths the slices imply — through `EmbedService::handle` (the
//! one in-process production dispatch binding) or the real crate seams
//! where the boundary is records-in — and pins the result honestly:
//!
//! 1. **Turn loop → context.assembled (R-2.4.1).** A real `submit` turn
//!    appends `context.assembled` — the `AssemblerPort` is invoked on the
//!    live path and (CAP.3, DF-S2.8-1) it IS the real `hh_context::assemble`
//!    builder over the run's durable prefix: the payload carries the
//!    builder record (`plan_id`/`layout_ref`/`policy_ref`/`derived_from`/
//!    `occupancy_estimate`/`assembly_ms`/`compaction_state`), and the
//!    retired pass-through stamp (`assembler: hh-embed/kernel`) is asserted
//!    *absent*. Both legs are un-ignored green; the residual DF-S2.8-1
//!    halves (retrieval/compaction producers, resume_set consumers,
//!    judged detectors, the OOP corpus leg) stay on the open row.
//! 2. **env.snapshot → fork{env: snapshot} → child run (R-2.2.4/2.2.5).**
//!    `env.snapshot{kind: fs_tree}` (instrument) then `fork{env:
//!    "snapshot"}` at the run head composes green: child run bound
//!    `forked_from`, `lifecycle.run.forked`, a snapshot-derived child env
//!    handle. The cadence leg — fork succeeding at a boundary with NO
//!    explicit snapshot — is `#[ignore]`d (DF-S2.9-3); the same call
//!    without a producer returns the typed `snapshot_unavailable`
//!    `EnvironmentUnavailable` refusal (pinned green — the honest answer).
//!    `env.suspend` on `local_host` refuses `UnknownCapability` →
//!    `EnvironmentUnavailable` (the class never declared it; provider
//!    classes that declare `fs_only` are not served at the boundary —
//!    `environment class <x> is not served at Stage 2`).
//! 3. **Experiment lifecycle through the `lab.experiment.*` boundary
//!    ops (R-2.10.3).** register → expand → open_experiment →
//!    next/claim/launch → subject rows under the subject writer lease
//!    (`surface_append`, the real append gate) → settle → `NextVerdict::
//!    Done` → close. At one-matched-dim scale the E-4 close row lands
//!    `status: completed` on `measurement.experiment.closed`. At exemplar
//!    scale (compaction family, 7 matched dims honestly measured) the
//!    same path completes too — CAP.3 (DF-S3.12b-1, ADR-0327) replaced
//!    the 512 B `OPEN_AUDIT` member bound with the enumerated
//!    `EXPERIMENT_CLOSED_FIELDS` partition, so the honest recheck
//!    members (`utilization`, `budget_match`) fit their declared
//!    record/list bound. Both legs are un-ignored green.
//! 4. **Hosted participant spine (R-2.10.6 ± R-2.10.3/R-2.10.4).** A REAL
//!    `hh_hosting` `HostingService` (dev-dep — the removable tier is
//!    test-only here; no production edge) over `AdapterA` +
//!    `FixtureParticipant`: attach → open → submit → close →
//!    `stream_events` → `proj::lift`. The lifted `{class, payload}` rows
//!    enter the subject run through `lab.hosting.attach` (the real
//!    registry-pin/quarantine/vouch path), the experiment lifecycle runs
//!    through `lab.experiment.*`, settled subjects project through
//!    `ResultsStore::project_and_record`, and `lab.analysis.analyze`
//!    produces a real `ComparisonReport` over `arm:native` vs
//!    `arm:hosted`.
//!
//! The egress mediation seam (DF-S2.4-1 — closed at CAP.3: dispatch
//! routes `net_egress` through `EgressMediator::gate`/`forward`) is
//! pinned in `hh-env/tests/acceptance.rs` (`cap2_*`) where the
//! dispatcher fixture lives; the surface-approval seam (DF-S4.11-3 —
//! closed at CAP.3: `respond_approval` is the supply protocol's builtin
//! answer verb, ADR-0329) in `hh-mcp-lab/tests/cap_2.rs`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::store::Lease;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixtures (the conformance.rs shapes, minimal) ───────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-cap2-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service(tag: &str) -> EmbedService {
    let root = test_dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "cap2".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("cap2-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

fn err_text(resp: &Json) -> String {
    resp.get("error")
        .map(|e| e.to_canonical_string())
        .unwrap_or_default()
}

/// `hello` with the experimental + measurement capabilities the lab and
/// env verb groups gate on.
fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("cap2")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                    ("serves_host_executor", Json::Bool(true)),
                    ("serves_permission_channel", Json::Bool(true)),
                    ("accepts_ephemeral_frames", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello refused: {r:?}");
}

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

/// The conformance fixture document (agent + rule + budget + permission +
/// the Stage-1 assembly over `hh/round_robin` + `hh/full_window`).
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

fn new_spec() -> Json {
    Json::obj([
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj([
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
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

/// `open_session{kind:"new"}` → `{session_id, run_id}`.
fn open_new(svc: &mut EmbedService, key: &str) -> (String, String) {
    let r = ok(&call(
        svc,
        "open_session",
        Json::obj([("spec", new_spec()), ("idempotency_key", Json::str(key))]),
    ));
    (
        r.get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        r.get("run_id").and_then(Json::as_str).unwrap().to_string(),
    )
}

fn submit(svc: &mut EmbedService, session: &str, key: &str) -> Json {
    call(
        svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(session)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("cap2 turn")),
                ])]),
            ),
            ("idempotency_key", Json::str(key)),
        ]),
    )
}

/// The run's committed `(class, payload)` rows — `surface_events` is the
/// boundary's own read seam (the surface-session projection source).
fn rows(svc: &EmbedService, run_id: &str) -> Vec<(String, Json)> {
    svc.surface_events(run_id)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

// ── 1. context assembly — the pass-through pin + the builder xfail ──────────

/// S2 leg — a real turn appends `context.assembled` produced by the
/// real `hh_context::assemble` builder (DF-S2.8-1 closed): every row
/// carries the builder record (`plan_id`, `model_call_id`, `derived_from`,
/// `layout_ref`, `policy_ref`, `occupancy_estimate`, `assembly_ms`,
/// `compaction_state`) and none carries the retired pass-through stamp.
/// The pin is removal-sensitive: an assembler that stops running the
/// builder drops `plan_id` and this test fails.
#[test]
fn cap2_turn_loop_assembler_is_invoked_as_passthrough() {
    let mut svc = service("assemble");
    hello(&mut svc);
    let (sid, run_id) = open_new(&mut svc, "open-assemble");
    let r = submit(&mut svc, &sid, "submit-assemble");
    assert!(r.get("result").is_some(), "submit refused: {r:?}");

    let evs = rows(&svc, &run_id);
    let assembled: Vec<&Json> = evs
        .iter()
        .filter(|(c, _)| c == "context.assembled")
        .map(|(_, p)| p)
        .collect();
    assert!(
        !assembled.is_empty(),
        "context.assembled appended on the scripted turn: {:?}",
        evs.iter().map(|(c, _)| c).collect::<Vec<_>>()
    );
    // Every assemble row (one per Propose round of the scripted turn) is
    // the builder's record — none carries the pass-through stamp.
    for p in &assembled {
        assert!(
            p.get("assembler").and_then(Json::as_str) != Some("hh-embed/kernel"),
            "the pass-through is retired — no `assembler: hh-embed/kernel` stamp: {p:?}"
        );
        assert!(p.get("assembly_error").is_none(), "the builder ran: {p:?}");
        for member in [
            "plan_id",
            "model_call_id",
            "derived_from",
            "layout_ref",
            "policy_ref",
            "occupancy_estimate",
            "assembly_ms",
            "compaction_state",
        ] {
            assert!(
                p.get(member).is_some(),
                "the builder record carries {member}: {p:?}"
            );
        }
    }
    // The turn still completed — the real assembler never blocks the loop.
    assert!(
        evs.iter().any(|(c, _)| c == "lifecycle.run.finished"),
        "run finished"
    );
}

/// DF-S2.8-1 (+ DF-S1.19-1): the composed leg — a live turn driven
/// through the real `hh_context::assemble` builder. CAP.3 landed the
/// producer wiring (`KernelAssembler` runs `hh_context::assemble` over
/// the durable prefix); the assertion set is the deferral's
/// done-condition verbatim.
#[test]
fn cap2_turn_loop_runs_hh_context_assembler() {
    let mut svc = service("assemble-real");
    hello(&mut svc);
    let (sid, run_id) = open_new(&mut svc, "open-assemble-real");
    let r = submit(&mut svc, &sid, "submit-assemble-real");
    assert!(r.get("result").is_some(), "submit refused: {r:?}");

    let evs = rows(&svc, &run_id);
    let p = evs
        .iter()
        .find(|(c, _)| c == "context.assembled")
        .map(|(_, p)| p)
        .expect("context.assembled appended on a Propose turn");
    // The §5c.1 builder record (crates/hh-context/src/assemble.rs —
    // `assembled_payload`) carries the plan identity + layout/policy refs
    // + measured timing. None of these exist on the pass-through stamp.
    assert!(
        p.get("plan_id").and_then(Json::as_str).is_some(),
        "DF-S2.8-1 residual: context.assembled is the kernel pass-through, \
         not the builder's plan record: {p:?}"
    );
    assert_eq!(
        p.get("layout_ref").and_then(Json::as_str),
        Some("layout/default")
    );
    assert_eq!(
        p.get("policy_ref").and_then(Json::as_str),
        Some("context_policy/default")
    );
    assert!(
        p.get("assembly_ms")
            .and_then(|m| m.get("value"))
            .and_then(Json::as_int)
            .is_some(),
        "assembly_ms{{value, measured_at}} missing: {p:?}"
    );
    assert!(p.get("occupancy_estimate").is_some());
    // The record names the call it feeds and the view it derived from.
    assert!(p.get("model_call_id").and_then(Json::as_str).is_some());
    let derived = p.get("derived_from").expect("derived_from");
    assert!(derived.get("run_id").and_then(Json::as_str).is_some());
    assert!(derived.get("view_hash").and_then(Json::as_str).is_some());
}

// ── 2. snapshot → fork{env: snapshot} → child run ───────────────────────────

/// S4 leg — the composed half that IS green: an explicit `env.snapshot`
/// (the instrument leg of R-2.2.5) followed by `fork{env: "snapshot"}`
/// produces a child run bound `forked_from` with a snapshot-derived env
/// handle, and `lifecycle.run.forked` is durable on it.
#[test]
fn cap2_snapshot_fork_composes_child_run() {
    let mut svc = service("snap-fork");
    hello(&mut svc);
    let (sid, run_id) = open_new(&mut svc, "open-snapfork");

    // Instrument snapshot — `env.snapshot{kind: fs_tree}` is the Group-M
    // op; the record returns the ref + the head seq it covers. It runs on
    // the live run: the store honestly refuses appends once the run has
    // finished (`EnvironmentUnavailable{Ledger(RunFinished)}`), so the
    // snapshot precedes the turn that ends the parent.
    let snap = ok(&call(
        &mut svc,
        "env.snapshot",
        Json::obj([
            ("session_id", Json::str(&sid)),
            ("kind", Json::str("fs_tree")),
        ]),
    ));
    let snapshot_ref = snap
        .get("snapshot_ref")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let at_seq = snap.get("at_seq").and_then(Json::as_int).expect("at_seq");
    assert_eq!(
        snap.get("taken_by").and_then(Json::as_str),
        Some("instrument")
    );

    // fork{env: snapshot} at the snapshotted head — the chooser binds the
    // snapshot the caller recorded.
    let f = ok(&call(
        &mut svc,
        "fork",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "at",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(at_seq))]),
            ),
            ("env", Json::str("snapshot")),
        ]),
    ));
    let child_run = f.get("run_id").and_then(Json::as_str).unwrap().to_string();
    assert_ne!(child_run, run_id);
    // The branch record pins the snapshot the child materialised from.
    let br = f.get("branch_record").expect("branch_record");
    assert_eq!(
        br.get("snapshot_ref").and_then(Json::as_str),
        Some(snapshot_ref.as_str()),
        "fork bound the instrument snapshot: {br:?}"
    );

    // The child run carries the fork lineage + the derived env attach.
    let evs = rows(&svc, &child_run);
    let classes: Vec<&str> = evs.iter().map(|(c, _)| c.as_str()).collect();
    assert!(
        classes.contains(&"lifecycle.run.forked"),
        "forked row on the child: {classes:?}"
    );
    assert!(
        evs.iter()
            .any(|(c, p)| { c == "lifecycle.run.forked" || p.get("snapshot_ref").is_some() }),
        "the fork record names the snapshot"
    );
    assert!(
        classes
            .iter()
            .any(|c| c.starts_with("action.environment.") || c.starts_with("lifecycle.env")),
        "a snapshot-derived env attached to the child: {classes:?}"
    );

    // The parent keeps driving — the fork neither finished nor detached
    // it; the turn that completes the run lands after the fork.
    let r = submit(&mut svc, &sid, "submit-snapfork");
    assert!(r.get("result").is_some(), "submit refused: {r:?}");

    // The parent stream carries the instrument snapshot row.
    let pevs = rows(&svc, &run_id);
    assert!(pevs.iter().any(|(c, _)| c == "action.environment.snapshot"));
    assert!(pevs.iter().any(|(c, _)| c == "lifecycle.run.finished"));
}

/// S4's unwired leg — pinned GREEN as the honest refusal: `fork{env:
/// "snapshot"}` on a run that never snapshotted refuses
/// `snapshot_unavailable` (EnvironmentUnavailable), never fabricates a
/// state.
#[test]
fn cap2_fork_without_snapshot_producer_refuses_typed() {
    let mut svc = service("fork-nosnap");
    hello(&mut svc);
    let (sid, _run_id) = open_new(&mut svc, "open-nosnap");
    let f = call(
        &mut svc,
        "fork",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "at",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("env", Json::str("snapshot")),
        ]),
    );
    assert_eq!(err_kind(&f), "EnvironmentUnavailable", "{f:?}");
    assert!(
        err_text(&f).contains("snapshot_unavailable"),
        "the typed refusal names the absent snapshot: {f:?}"
    );
}

/// DF-S2.9-3: the producer cadence — nothing snapshots a live run at
/// turn/checkpoint boundaries, so this assertion fails today. `#[ignore]`d
/// pending the env-driver ticket the deferral names.
#[test]
#[ignore = "DF-S2.9-3: no snapshot producer cadence — fork{env: snapshot} at a turn boundary only composes when a test/op called env.snapshot first; routed to the env-driver ticket (CAP.3 may wire a minimal cadence)"]
fn cap2_fork_at_turn_boundary_without_explicit_snapshot() {
    let mut svc = service("fork-cadence");
    hello(&mut svc);
    let (sid, _run_id) = open_new(&mut svc, "open-cadence");
    let r = submit(&mut svc, &sid, "submit-cadence");
    assert!(r.get("result").is_some(), "submit refused: {r:?}");
    // The cadence that does not exist: a snapshot at/below the boundary.
    let f = call(
        &mut svc,
        "fork",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "at",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(1))]),
            ),
            ("env", Json::str("snapshot")),
        ]),
    );
    assert!(
        f.get("result").is_some(),
        "DF-S2.9-3 residual: no cadence-produced snapshot exists: {f:?}"
    );
}

/// The suspend leg is environment-bound: `local_host` never declared a
/// `suspend` capability (`SuspendKind::Unknown`), so `env.suspend`
/// honestly refuses `EnvironmentUnavailable{unknown_capability}` — and
/// the provider classes that declare `fs_only` are not served at the
/// boundary at all (`environment class <x> is not served at Stage 2`).
/// Both halves pinned green — this is the specified behavior, not a
/// fake of the composed suspend→snapshot→fork leg.
#[test]
fn cap2_suspend_is_environment_bound_honest_refusal() {
    let mut svc = service("suspend");
    hello(&mut svc);
    let (sid, _run_id) = open_new(&mut svc, "open-suspend");
    let s = call(
        &mut svc,
        "env.suspend",
        Json::obj([("session_id", Json::str(&sid))]),
    );
    assert_eq!(err_kind(&s), "EnvironmentUnavailable", "{s:?}");
    // A provider-class binding never reaches a driver — the boundary
    // refuses the class at open (no provider adapters are served).
    let r = call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("new")),
                    (
                        "definition",
                        Json::obj([
                            ("kind", Json::str("document")),
                            ("document", document_json()),
                        ]),
                    ),
                    ("overrides", Json::Arr(vec![])),
                    (
                        "environment",
                        Json::obj([
                            ("kind", Json::str("connection_info")),
                            (
                                "connection_info",
                                Json::obj([("class", Json::str("provider_hosted"))]),
                            ),
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
                ]),
            ),
            ("idempotency_key", Json::str("open-provider")),
        ]),
    );
    assert_eq!(
        err_kind(&r),
        "EnvironmentUnavailable",
        "provider classes are not served at Stage 2: {r:?}"
    );
}

// ── 3. the `lab.experiment.*` boundary lifecycle ────────────────────────────
//
// The lifecycle is driven through `EmbedService::handle` — the production
// dispatch binding — never the engine directly. Subject-run accounting rows
// append through `surface_append` under the `subject_writer` lease the
// boundary's `launch` returns (the same gate the surface path uses).

use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};
use hh_ontology::eval::{
    Design, DesignKind, MetricValue, MetricValueKind, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::{Granularity, ParticipantClass};
use hh_ontology::FactorKind;

fn pinned(tag: &str) -> String {
    hh_identity::idp::idp_id(&format!("cap2.{tag}"), tag.as_bytes())
}

/// The resolver budget bodies `register{budgets{}}` deposits (hard caps,
/// `model_calls` — the pool/experiment envelopes see them too).
fn exp_budgets() -> Json {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
        .to_json()
    };
    Json::obj([
        ("eval:a", caps(100)),
        ("eval:b", caps(100)),
        ("search:a", caps(50)),
        ("search:b", caps(50)),
        ("budget:exp", caps(1_000)),
        ("budget:inst", caps(1_000)),
        ("pool", caps(1_000)),
        ("instr", caps(1_000)),
    ])
}

fn prereg() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "arm B is not worse than arm A".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: pinned("analysis.plan"),
        task_split_hash: pinned("split.hash"),
        interactions: vec![],
    }
}

fn seed_policy() -> SeedPolicy {
    SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: false,
        seed_honoured_required: false,
    }
}

fn design() -> Design {
    Design {
        id: "design:cap2".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 2,
        pairing: Pairing::ByTask,
        seed_policy: seed_policy(),
        held_out_split_ref: None,
        pre_registration: prereg(),
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn exp_arm(id: &str, level: &str, eval: &str, limits_enforced: &str) -> ArmSpec {
    ArmSpec {
        inference_budget: None,
        arm_id: id.into(),
        hypothesis: format!("{id} does better"),
        level_assignment: BTreeMap::from([("participant".to_string(), level.to_string())]),
        eval_budget: eval.into(),
        search_budget: Some("search:a".into()),
        match_spec: Some(hh_budget::MatchSpec::matched_cap(&[
            DimensionId::ModelCalls,
        ])),
        artifact_ref: hh_ontology::config::Ref::new("artifact:x", pinned("artifact.x")),
        limits_enforced: limits_enforced.into(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    }
}

fn level(level_id: &str, ref_: &str, class: ParticipantClass) -> LevelSpec {
    LevelSpec {
        level_id: level_id.into(),
        ref_: ref_.into(),
        overrides: None,
        label: level_id.into(),
        class,
        non_portable: false,
    }
}

/// The minimal comparative spec — one `participant` factor, two levels,
/// `matched_cap([model_calls])`, `replicates_per_cell` replicates on
/// `task:cap2.0` (held_out).
fn exp_spec(native_class: ParticipantClass, hosted: Option<&str>) -> ExperimentSpec {
    let mut levels = vec![level("native", &pinned("level.native"), native_class)];
    if let Some(vid) = hosted {
        levels.push(level("hosted", vid, ParticipantClass::Hosted));
    } else {
        levels.push(level(
            "other",
            &pinned("level.other"),
            ParticipantClass::Native,
        ));
    }
    let mut arms = vec![exp_arm("arm:native", "native", "eval:a", "full")];
    arms.push(if hosted.is_some() {
        exp_arm("arm:hosted", "hosted", "eval:a", "partial")
    } else {
        exp_arm("arm:other", "other", "eval:a", "full")
    });
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: design(),
        pre_registration: Some(prereg()),
        factors: vec![FactorSpec {
            name: "participant".into(),
            kind: FactorKind::Harness,
            granularity: Some(Granularity::ProductLevel),
            role: None,
            levels,
        }],
        arms,
        suite: SuiteBinding {
            suite_ref: pinned("suite.cap2"),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pinned("split.assign")),
        },
        replicates_per_cell: 2,
        seed_policy: seed_policy(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: "perm:cap2".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 1_000_000,
            backoff: Backoff {
                min_ms: 0,
                multiplier_ppm: 1_000_000,
                max_ms: 0,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:cap2".into(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

/// `register → expand` through the boundary — returns `(experiment_id,
/// run_plan_id → arm_id)` decoded off the deposited `CellPlan` preview.
fn exp_register_expand(
    svc: &mut EmbedService,
    spec: &ExperimentSpec,
    tasks: &[&str],
    extra: Vec<(&'static str, Json)>,
) -> (String, BTreeMap<String, String>) {
    let mut reg = vec![
        ("spec", spec.to_json()),
        ("budgets", exp_budgets()),
        ("min_replicates", Json::Int(1)),
    ];
    for (k, v) in extra {
        reg.push((k, v));
    }
    let r = ok(&call(svc, "lab.experiment.register", Json::obj(reg)));
    let eid = r
        .get("experiment_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut expand = vec![
        ("experiment_id", Json::str(&eid)),
        // `expand` re-validates the spec's matched-budget projection
        // through the param-supplied resolver (SpecContext::resolve_budget
        // has no LabDocs fallback — the deposit only serves engine ops).
        ("budgets", exp_budgets()),
        (
            "suite_tasks",
            Json::Arr(
                tasks
                    .iter()
                    .map(|t| {
                        Json::obj([
                            ("task_id", Json::str(*t)),
                            ("split_label", Json::str("held_out")),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "arm_configs",
            Json::Obj(
                spec.arms
                    .iter()
                    .map(|a| {
                        (
                            a.arm_id.clone(),
                            Json::obj([
                                (
                                    "configuration_id",
                                    Json::str(pinned(&format!("cfg.{}", a.arm_id))),
                                ),
                                (
                                    "configuration_version_id",
                                    Json::str(pinned(&format!("cfgv.{}", a.arm_id))),
                                ),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
    ];
    let plan = ok(&call(
        svc,
        "lab.experiment.expand",
        Json::obj(expand.clone()),
    ));
    let _ = &mut expand;
    let plan = plan.get("plan").cloned().unwrap_or(Json::Null);
    // `run_plan_id → arm_id` — cell_id joins the two plan members.
    let mut cell_arm: BTreeMap<String, String> = BTreeMap::new();
    if let Json::Arr(cells) = plan.get("cells").cloned().unwrap_or(Json::Arr(vec![])) {
        for c in &cells {
            cell_arm.insert(
                c.get("cell_id").and_then(Json::as_str).unwrap().to_string(),
                c.get("arm_id").and_then(Json::as_str).unwrap().to_string(),
            );
        }
    }
    let mut rp_arm = BTreeMap::new();
    if let Json::Arr(rps) = plan.get("run_plans").cloned().unwrap_or(Json::Arr(vec![])) {
        for rp in &rps {
            let rpid = rp.get("run_plan_id").and_then(Json::as_str).unwrap();
            let cid = rp.get("cell_id").and_then(Json::as_str).unwrap();
            rp_arm.insert(
                rpid.to_string(),
                cell_arm.get(cid).cloned().unwrap_or_default(),
            );
        }
    }
    (eid, rp_arm)
}

/// `open_experiment` → the experiment run id.
fn exp_open(svc: &mut EmbedService, eid: &str) -> String {
    ok(&call(
        svc,
        "lab.experiment.open_experiment",
        Json::obj([("experiment_id", Json::str(eid))]),
    ))
    .get("experiment_run_id")
    .and_then(Json::as_str)
    .unwrap()
    .to_string()
}

/// Rebuild the typed `Lease` the `launch` result reports (the boundary's
/// own spelling — `subject_writer{lease_id, holder, generation,
/// expires_at_ms}`).
fn lease_of(w: &Json, run_id: &str) -> Lease {
    Lease {
        run_id: run_id.to_string(),
        lease_id: w
            .get("lease_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        holder: w.get("holder").and_then(Json::as_str).unwrap().to_string(),
        generation: w.get("generation").and_then(Json::as_int).unwrap() as u64,
        expires_at_ms: w.get("expires_at_ms").and_then(Json::as_int).unwrap() as u64,
    }
}

/// Mint a kernel-provenance row for `run_id` — the shape the driver/oracle
/// halves honestly record (the same `mint` s3_12b uses against the store).
fn mint(svc: &EmbedService, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: svc.store().alloc_id("evt"),
        class: class.to_string(),
        ts: svc.store().ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-embed-cap2/1"),
        scope: Scope::default(),
        parent_event_id: svc.store().head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "hh-embed-cap2/1",
            svc.store().now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

/// The subject run's honest accounting — `control.budget.consumed` on every
/// matched dim (the compare's `budget_match` reads these), the
/// `task_success` metric cell, `lifecycle.run.finished` — appended under
/// the launch's `subject_writer` lease through the real append gate.
fn finish_subject(
    svc: &mut EmbedService,
    run_id: &str,
    writer: &Json,
    consumed: &[(DimensionId, i64)],
    task_success: bool,
) {
    let lease = lease_of(writer, run_id);
    let stop = hh_ontology::control::StopReason::Completed;
    let mut batch = Vec::new();
    for (d, a) in consumed {
        batch.push(mint(
            svc,
            run_id,
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str(d.as_str())),
                ("amount", Json::Int(*a)),
            ]),
        ));
    }
    batch.push(mint(
        svc,
        run_id,
        "measurement.metric.emitted",
        MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(task_success),
            applies_to: run_id.into(),
            oracle_ref: "oracle/stub.cap2".into(),
            detector: hh_ontology::compliance::Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
        }
        .to_json(),
    ));
    batch.push(mint(
        svc,
        run_id,
        "lifecycle.run.finished",
        Json::obj([
            ("stop_reason", stop.to_json()),
            ("outcome_class", Json::str(stop.outcome_class().as_str())),
        ]),
    ));
    let mut parent = svc.store().head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    svc.surface_append(run_id, &lease, batch).unwrap();
}

/// `next → claim → launch → finish → settle` until `done`, through the
/// boundary ops. `hosted_session` (when `Some`) rides every launch — the
/// engine routes only hosted-level arms through it. Returns the settled
/// `(run_plan_id, subject_run_id, arm_id)` triples in schedule order.
/// What a hosted subject run needs around its launch — the hosting
/// plane's open/submit/close already ran (the outcome is the *reported*
/// half `launch` stamps); `rows` are the `proj::lift` lifted rows and
/// `end_state` the participant's closed snapshot — both ingest through
/// `lab.hosting.attach` under the `subject_writer` lease.
struct HostedLeg {
    /// The `hosted_session` member for `launch` (the adapter's report).
    launch_outcome: Json,
    /// The hosted session ref (attach's `session.session_ref`).
    session_ref: String,
    /// The negotiated ABI version (`hh-hosting/1`).
    abi_version: String,
    /// `{class, payload}` lifted rows for `attach`'s `rows[]`.
    rows: Vec<Json>,
    /// The participant's end-state record (verbatim onto `detached`).
    end_state: Json,
}

/// The hosted drain leg — the participant leg closure + the participant's
/// version_id pin (the closure is `dyn` because callers wrap distinct
/// `HostingService` borrows).
type HostedDrain<'a> = (
    &'a mut dyn FnMut(&str, &str) -> HostedLeg,
    &'a str, // participant version_id pin
);

fn exp_drain(
    svc: &mut EmbedService,
    eid: &str,
    rp_arm: &BTreeMap<String, String>,
    consumed: &[(DimensionId, i64)],
    task_success: &dyn Fn(&str) -> bool,
    mut hosted: Option<HostedDrain<'_>>,
) -> Vec<(String, String, String)> {
    let mut settled = Vec::new();
    loop {
        let n = ok(&call(
            svc,
            "lab.experiment.next",
            Json::obj([("experiment_id", Json::str(eid))]),
        ));
        match n.get("verdict").and_then(Json::as_str) {
            Some("done") => break,
            Some("plan") => {
                let rpid = n
                    .get("run_plan_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string();
                let t = ok(&call(
                    svc,
                    "lab.experiment.claim",
                    Json::obj([
                        ("experiment_id", Json::str(eid)),
                        ("run_plan_id", Json::str(&rpid)),
                    ]),
                ));
                let arm = rp_arm.get(&rpid).cloned().unwrap_or_default();
                // The hosted arm's session runs on the real HostingService
                // before launch — the outcome is what `launch` stamps.
                let leg = hosted
                    .as_mut()
                    .and_then(|(f, _)| arm.contains("hosted").then(|| f(&rpid, &arm)));
                let mut lp = vec![
                    ("experiment_id", Json::str(eid)),
                    ("run_plan_id", Json::str(&rpid)),
                    ("lease_id", t.get("lease_id").cloned().unwrap_or(Json::Null)),
                ];
                if let Some(l) = &leg {
                    lp.push(("hosted_session", l.launch_outcome.clone()));
                }
                let l = ok(&call(svc, "lab.experiment.launch", Json::obj(lp)));
                let subject = l.get("run_id").and_then(Json::as_str).unwrap().to_string();
                let writer = l.get("subject_writer").cloned().unwrap();
                // The hosted session's lifted rows ingest through
                // `lab.hosting.attach` under the subject_writer lease (D4).
                if let (Some(leg), Some((_, vid))) = (&leg, &hosted) {
                    let a = ok(&call(
                        svc,
                        "lab.hosting.attach",
                        Json::obj([
                            ("participant_ref", Json::str(*vid)),
                            ("run_id", Json::str(&subject)),
                            ("lease", writer.clone()),
                            (
                                "session",
                                Json::obj([
                                    ("session_ref", Json::str(&leg.session_ref)),
                                    ("abi_version", Json::str(&leg.abi_version)),
                                    ("mediation", Json::str("none")),
                                    ("adapter_version_id", Json::str("a/1")),
                                ]),
                            ),
                            ("rows", Json::Arr(leg.rows.clone())),
                            ("end_state", leg.end_state.clone()),
                        ]),
                    ));
                    // `attached` may be `false` — when `launch` stamped
                    // the `hosted_session` outcome the engine already
                    // minted the `attached`/`bound` pair (D4's converge);
                    // the lifted `rows[]` still append — pin that.
                    assert!(
                        a.get("appended").and_then(Json::as_int).unwrap_or(0) > 0
                            && a.get("lifted_rows").and_then(Json::as_int).unwrap_or(0) > 0,
                        "hosted attach lands lifted rows: {a:?}"
                    );
                }
                settled.push((rpid.clone(), subject.clone(), arm.clone()));
                finish_subject(svc, &subject, &writer, consumed, task_success(&arm));
                let s = ok(&call(
                    svc,
                    "lab.experiment.settle",
                    Json::obj([
                        ("experiment_id", Json::str(eid)),
                        ("run_plan_id", Json::str(&rpid)),
                    ]),
                ));
                assert_eq!(
                    s.get("accepted"),
                    Some(&Json::Bool(true)),
                    "subject run settles accepted: {s:?}"
                );
            }
            other => panic!("unexpected next verdict: {other:?}"),
        }
    }
    settled
}

/// 3a — the composed lifecycle at one-matched-dim scale: register → expand
/// → open → drain → settle → `close` — and `status: completed` lands
/// `measurement.experiment.closed` on the experiment run (the E-4 row's
/// audit members fit `AUDIT_FIELD_MAX_BYTES` at this scale — the honest
/// green half DF-S3.12b-1 names).
#[test]
fn cap2_experiment_boundary_lifecycle_closes_green() {
    let mut svc = service("exp-lifecycle");
    hello(&mut svc);
    let spec = exp_spec(ParticipantClass::Native, None);
    let (eid, rp_arm) = exp_register_expand(&mut svc, &spec, &["task:cap2.0"], vec![]);
    let exp_run = exp_open(&mut svc, &eid);
    // 2 arms × 1 task × 2 replicates = 4 plans — every subject honestly
    // recording `model_calls` consumption (the matched dim).
    let settled = exp_drain(
        &mut svc,
        &eid,
        &rp_arm,
        &[(DimensionId::ModelCalls, 5)],
        &|_| true,
        None,
    );
    assert_eq!(settled.len(), 4, "2 arms × 1 task × 2 replicates");

    let report = ok(&call(
        &mut svc,
        "lab.experiment.close",
        Json::obj([("experiment_id", Json::str(eid))]),
    ));
    assert_eq!(
        report.get("status").and_then(Json::as_str),
        Some("completed"),
        "the honest small-scale close completes: {report:?}"
    );
    // The E-4 row is durable on the experiment run.
    let evs = rows(&svc, &exp_run);
    assert!(
        evs.iter()
            .any(|(c, _)| c == "measurement.experiment.closed"),
        "measurement.experiment.closed landed: {:?}",
        evs.iter().map(|(c, _)| c).collect::<Vec<_>>()
    );
}

/// 3b — the same boundary lifecycle at exemplar scale: the compaction
/// family's seven matched dims honestly measured on every subject — the
/// E-4 `closed` row's `utilization`/`budget_match`/`na_cells` members are
/// record-shaped and sit at `AUDIT_FIELD_LIST_BYTES` under the enumerated
/// `EXPERIMENT_CLOSED_FIELDS` partition (DF-S3.12b-1, closed at CAP.3 —
/// ADR-0327). Pinned as the landed close — the refusal this test
/// pre-CAP.3 pinned is gone.
#[test]
fn cap2_experiment_exemplar_close_lands() {
    let mut svc = service("exp-exemplar");
    hello(&mut svc);
    let pins = hh_lab::exemplars::ExemplarPins {
        suite_ref: pinned("suite.tb2"),
        held_out_split_ref: pinned("split.held_out"),
        split_assignment_ref: pinned("split.assign"),
        registry_snapshot_id: pinned("registry.snap"),
        eval_budget: "eval:a".into(),
        search_budget: "search:a".into(),
        experiment_budget: "pool".into(),
        instrument_budget: "instr".into(),
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
    };
    let spec = hh_lab::exemplars::compaction_family_v1(
        &pins,
        &hh_lab::exemplars::CompactionFamilyPins {
            evict_oldest_ref: pinned("variant.evict_oldest"),
            clear_tool_results_ref: pinned("variant.clear_tool_results"),
            model_level_ref: pinned("model.fixed"),
            environment_level_ref: pinned("env.tb2"),
            artifacts: (
                hh_ontology::config::Ref::new("definition:compaction", pinned("artifact.evict")),
                hh_ontology::config::Ref::new("definition:compaction", pinned("artifact.clear")),
            ),
        },
        1,
    );
    let tasks = ["task:cap2ex.0", "task:cap2ex.1", "task:cap2ex.2"];
    let (eid, rp_arm) = exp_register_expand(&mut svc, &spec, &tasks, vec![]);
    let exp_run = exp_open(&mut svc, &eid);
    // 2 arms × 3 tasks × 5 replicates = 30 plans — honestly recording
    // every matched dimension (the overrun condition).
    let dims: Vec<(DimensionId, i64)> = spec.arms[0]
        .match_spec
        .as_ref()
        .map(|m| m.dimensions.iter().map(|d| (*d, 5)).collect())
        .unwrap_or_default();
    let settled = exp_drain(&mut svc, &eid, &rp_arm, &dims, &|_| true, None);
    assert_eq!(settled.len(), 30);
    let report = ok(&call(
        &mut svc,
        "lab.experiment.close",
        Json::obj([("experiment_id", Json::str(&eid))]),
    ));
    assert_eq!(
        report.get("status").and_then(Json::as_str),
        Some("completed"),
        "the exemplar close completes under the enumerated partition: {report:?}"
    );
    // The E-4 row is durable on the experiment run and carries the
    // measured recheck members the open partition refused.
    let evs = rows(&svc, &exp_run);
    let closed = evs
        .iter()
        .find(|(c, _)| c == "measurement.experiment.closed")
        .map(|(_, p)| p.clone())
        .expect("measurement.experiment.closed landed at exemplar scale");
    assert!(
        closed.get("utilization").is_some() && closed.get("budget_match").is_some(),
        "the closed row carries the honest measured detail: {closed:?}"
    );
}

/// DF-S3.12b-1 — closed at CAP.3: `status: completed` lands at exemplar
/// scale under the enumerated `EXPERIMENT_CLOSED_FIELDS` partition
/// (ADR-0327's class-declaration ruling). This is the deferral's done
/// check verbatim, un-ignored.
#[test]
fn cap2_experiment_exemplar_close_completes() {
    let mut svc = service("exp-exemplar-done");
    hello(&mut svc);
    let pins = hh_lab::exemplars::ExemplarPins {
        suite_ref: pinned("suite.tb2"),
        held_out_split_ref: pinned("split.held_out"),
        split_assignment_ref: pinned("split.assign"),
        registry_snapshot_id: pinned("registry.snap"),
        eval_budget: "eval:a".into(),
        search_budget: "search:a".into(),
        experiment_budget: "pool".into(),
        instrument_budget: "instr".into(),
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
    };
    let spec = hh_lab::exemplars::compaction_family_v1(
        &pins,
        &hh_lab::exemplars::CompactionFamilyPins {
            evict_oldest_ref: pinned("variant.evict_oldest"),
            clear_tool_results_ref: pinned("variant.clear_tool_results"),
            model_level_ref: pinned("model.fixed"),
            environment_level_ref: pinned("env.tb2"),
            artifacts: (
                hh_ontology::config::Ref::new("definition:compaction", pinned("artifact.evict")),
                hh_ontology::config::Ref::new("definition:compaction", pinned("artifact.clear")),
            ),
        },
        1,
    );
    let tasks = ["task:cap2exd.0", "task:cap2exd.1", "task:cap2exd.2"];
    let (eid, rp_arm) = exp_register_expand(&mut svc, &spec, &tasks, vec![]);
    let _exp_run = exp_open(&mut svc, &eid);
    let dims: Vec<(DimensionId, i64)> = spec.arms[0]
        .match_spec
        .as_ref()
        .map(|m| m.dimensions.iter().map(|d| (*d, 5)).collect())
        .unwrap_or_default();
    exp_drain(&mut svc, &eid, &rp_arm, &dims, &|_| true, None);
    let report = ok(&call(
        &mut svc,
        "lab.experiment.close",
        Json::obj([("experiment_id", Json::str(&eid))]),
    ));
    assert_eq!(
        report.get("status").and_then(Json::as_str),
        Some("completed"),
        "DF-S3.12b-1 residual: the honest exemplar close cannot complete"
    );
}

// ── 4. the hosted participant spine — a REAL HostingService ────────────────
//
// `hh-embed` never depends on `hh-hosting` in production code (the seam is
// records-in/records-out); this test adds it as a *dev-dependency* so the
// composed leg drives a genuine `HostingService` (Adapter A over the
// deterministic `FixtureParticipant`) next to the boundary — open → submit
// → close → `stream_events` → `proj::lift` — and hands the lifted rows to
// `lab.hosting.attach` on the subject run the experiment's `launch` minted
// (the `subject_writer` lease hand-off, D4). Settled subjects project
// through `ResultsStore::project_and_record` and `lab.analysis.analyze`
// produces the comparison over `arm:native` vs `arm:hosted`.

use hh_hir::leaves::Text;
use hh_hir::records::{EvidenceRef, ExpiryCondition, OwnerRef};
use hh_hosting::abi::HostedRunSpec;
use hh_hosting::adapter_a::{adapter_a_record, allow_all_decider, AdapterA};
use hh_hosting::fixture::FixtureParticipant;
use hh_hosting::records::{HostingExt, ModelIoIntercept, ParticipantRecord, ProcessPlacement};
use hh_hosting::service::HostingService;
use hh_ontology::debt::{DebtStatus, ExpiryKind};
use hh_ontology::eval::{EstimatorSelection, IntervalMethod};
use hh_ontology::participant::{
    CapabilityVerdict, HostingMechanism, Observability, ParticipantDescriptor,
};

/// The claim-complete adapter debt record (AC-R-2.10.6-8 — attach refuses
/// a record without a hash-addressed hypothesis, evidence, and a removal
/// test).
fn adapter_debt() -> hh_hir::records::AssumptionDebtRecord {
    let prov = ProvenanceRecord::kernel("hh-embed/cap2", 0);
    hh_hir::records::AssumptionDebtRecord {
        rule_id: "hh.hosting.adapter_a".into(),
        hypothesis: Text::new(
            "participants matching participant_selector behave per declaration_defaults",
            "hh.adapter.a",
            prov.clone(),
        ),
        evidence_refs: vec![EvidenceRef::legacy("cap2-adapterA")],
        owner: OwnerRef::principal("hh.adapter.a"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ProbeFailure,
            value: Some("P0 dimension drift".into()),
        },
        removal_test_ref: "cap_2_composed.rs".into(),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: None,
        created_by: Some(prov),
        created_at: Some(0),
        supersedes: None,
    }
}

/// A real `HostingService` over Adapter A + a `FixtureParticipant`, plus
/// the `ParticipantRecord` the registry body mirrors.
fn hosted_service(fixture: FixtureParticipant) -> (HostingService, ParticipantRecord) {
    let decl = BTreeMap::from([("usage_reporting".to_string(), Json::str("supported"))]);
    let vector = BTreeMap::from([("usage_reporting".to_string(), CapabilityVerdict::Supported)]);
    let participant = ParticipantRecord::new(
        "p:cap2",
        "1.0.0",
        ParticipantDescriptor {
            class: ParticipantClass::Hosted,
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: [Observability::Events, Observability::EndState]
                .into_iter()
                .collect(),
            capability_vector: vector,
        },
        decl,
        HostingExt {
            abi_versions: Some(vec!["hh-hosting/1".into()]),
            credential_supply: Some(vec!["env".into()]),
            process_placement: Some(ProcessPlacement::InEnvironment),
            ..Default::default()
        },
        BTreeMap::new(),
    )
    .expect("participant record");
    let adapter = AdapterA::new(
        Box::new(fixture),
        adapter_a_record("a/1", ModelIoIntercept::None, adapter_debt()),
        Some(allow_all_decider()),
    );
    let capability_vector =
        BTreeMap::from([("usage_reporting".to_string(), Json::str("supported"))]);
    let svc = HostingService::attach(
        participant.clone(),
        adapter,
        capability_vector,
        true,
        BTreeMap::new(),
    )
    .expect("hosting attach");
    (svc, participant)
}

/// The fixture half the green spine drives — the default participant
/// minus the permission-gated Lab-supplied tool (whose lifted
/// `security.permission.*` rows hit the audit-partition seam
/// `cap2_hosted_attach_permission_rows_refuse` pins — DF-CAP.2-1).
fn ungated_fixture() -> FixtureParticipant {
    let mut f = FixtureParticipant::default();
    f.request_permissions = false;
    f.scripted_tools = vec![hh_hosting::fixture::ScriptedTool {
        kind_hint: "read".into(),
        permission_gated: false,
        lab_supplied: false,
    }];
    f
}

/// The `HostedRunSpec` a launch hands the participant (placement-bound
/// `connection_info` — never a handle).
fn hosted_run_spec() -> HostedRunSpec {
    HostedRunSpec {
        definition_ref: pinned("def.cap2"),
        params: Json::obj([]),
        placement: ProcessPlacement::InEnvironment,
        connection_info: Json::obj([("socket", Json::str("/tmp/cap2"))]),
        context_items: Vec::new(),
        budget_view: None,
        credential_channels: vec!["env".into()],
        resume_cursor: None,
    }
}

/// The registry `participant` body — the opaque-records shape the
/// boundary reads (`version_identity`, `descriptor{hosting_mechanism,
/// observability_level}`, top-level `hosting_ext{abi_versions}`, and
/// `capability_declaration`).
fn participant_body(p: &ParticipantRecord) -> Json {
    Json::obj([
        ("kind", Json::str("participant")),
        ("participant_id", Json::str(&p.participant_id)),
        ("version_identity", Json::str(&p.version_identity)),
        (
            "descriptor",
            Json::obj([
                ("class", Json::str("hosted")),
                ("hosting_mechanism", Json::str("session_abi")),
                (
                    "observability_level",
                    Json::Arr(vec![Json::str("events"), Json::str("end_state")]),
                ),
            ]),
        ),
        (
            "hosting_ext",
            Json::obj([
                ("abi_versions", Json::Arr(vec![Json::str("hh-hosting/1")])),
                ("credential_supply", Json::Arr(vec![Json::str("env")])),
            ]),
        ),
        (
            "capability_declaration",
            Json::obj([("usage_reporting", Json::str("supported"))]),
        ),
    ])
}

/// One hosted subject's worth of HostingService work — open → submit →
/// close → stream → lift — reported for `launch` and ingested by
/// `lab.hosting.attach`.
fn drive_hosted(
    hsvc: &mut HostingService,
    participant_identity: &str,
    run_plan_id: &str,
    arm_id: &str,
) -> HostedLeg {
    drive_hosted_stamped(hsvc, participant_identity, run_plan_id, arm_id, None)
}

/// `drive_hosted` + the adapter's stamped `budget_enforcement` report —
/// `{dimension → enforced|advisory|unenforceable}` rides `launch`'s
/// `hosted_session` outcome onto the subject manifest (ADR-0165 D3;
/// DF-CAP.2-2).
fn drive_hosted_stamped(
    hsvc: &mut HostingService,
    participant_identity: &str,
    _run_plan_id: &str,
    _arm_id: &str,
    budget_enforcement: Option<Json>,
) -> HostedLeg {
    let opened = hsvc.open(hosted_run_spec()).expect("hosted open");
    let _turn = hsvc
        .submit(
            &opened.session_ref,
            &Json::obj([("prompt", Json::str("cap2 hosted turn"))]),
        )
        .expect("hosted submit");
    let end = hsvc.close(&opened.session_ref).expect("hosted close");
    let events = hsvc
        .stream_events(&opened.session_ref)
        .expect("stream_events")
        .to_vec();
    let lifted = hh_hosting::proj::lift(&events);
    HostedLeg {
        launch_outcome: Json::obj([
            ("session_ref", Json::str(&opened.session_ref)),
            ("hosting_mechanism", Json::str("session_abi")),
            ("limits_enforced", Json::str("partial")),
            ("adapter_version_id", Json::str("a/1")),
            (
                "participant_version_identity",
                Json::str(participant_identity),
            ),
            ("abi_version", Json::str(&opened.abi_version)),
            (
                "budget_enforcement",
                budget_enforcement.unwrap_or(Json::obj([])),
            ),
        ]),
        session_ref: opened.session_ref,
        abi_version: opened.abi_version,
        rows: lifted
            .iter()
            .map(|r| {
                Json::obj([
                    ("class", Json::str(&r.class)),
                    ("payload", r.payload.clone()),
                ])
            })
            .collect(),
        end_state: Json::obj([
            ("kind", Json::str(&end.reason)),
            ("turns", Json::Int(end.turns as i64)),
        ]),
    }
}

/// The composed spine end-to-end: a REAL `HostingService` session per
/// hosted subject run (open → submit → close on the fixture participant),
/// the lifted rows ingested through `lab.hosting.attach` under the
/// launch's `subject_writer` lease, the experiment closing green at
/// small scale, the settled subjects projecting through
/// `project_and_record`, and `lab.analysis.analyze` returning the
/// `{record, report, body}` triple — one `ComparisonReport` over the
/// native/hosted arm pair (native all-pass vs hosted all-fail by the
/// Lab oracle's own `measurement.metric.emitted` rows — the oracle is
/// Lab-side on both arms).
#[test]
fn cap2_hosted_spine_attach_project_analyze() {
    let mut svc = service("hosted-spine");
    hello(&mut svc);
    let (mut hsvc, participant) = hosted_service(ungated_fixture());

    // The participant registers through the real registry boundary (the
    // body's `version_identity` is the record's derived coordinate).
    let vid = ok(&call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", participant_body(&participant)),
            (
                "registrar",
                ProvenanceRecord::kernel("hh-embed/cap2", 0).to_json(),
            ),
        ]),
    ))
    .get("version_id")
    .and_then(Json::as_str)
    .unwrap()
    .to_string();

    // Two arms — `arm:native` + `arm:hosted` (the hosted level's `ref_`
    // is the registered participant's version coordinate).
    let spec = exp_spec(ParticipantClass::Native, Some(&vid));
    let tasks = ["task:cap2h.0"];
    let (eid, rp_arm) = exp_register_expand(&mut svc, &spec, &tasks, vec![]);
    let exp_run = exp_open(&mut svc, &eid);

    let identity = participant.version_identity.clone();
    let mut host = |rpid: &str, arm: &str| drive_hosted(&mut hsvc, &identity, rpid, arm);
    // 2 arms × 1 task × 2 replicates = 4 plans — the two hosted plans
    // each open their own HostingService session; the Lab oracle records
    // native subjects passing and hosted subjects failing.
    let settled = exp_drain(
        &mut svc,
        &eid,
        &rp_arm,
        &[(DimensionId::ModelCalls, 5)],
        &|arm| !arm.contains("hosted"),
        Some((&mut host, vid.as_str())),
    );
    assert_eq!(settled.len(), 4, "2 arms × 1 task × 2 replicates");

    // The experiment run's launch rows stamp the hosted half honestly —
    // `run_launched{participant_class: hosted, hosted_session_ref}` for
    // the two hosted plans.
    let evs = rows(&svc, &exp_run);
    let hosted_launches = evs
        .iter()
        .filter(|(c, p)| {
            c == "measurement.experiment.run_launched"
                && p.get("participant_class").and_then(Json::as_str) == Some("hosted")
        })
        .count();
    assert_eq!(
        hosted_launches,
        2,
        "two hosted launches stamped on the experiment run: {:?}",
        evs.iter().map(|(c, _)| c).collect::<Vec<_>>()
    );

    // The hosted subject runs carry the attached/detached audit pair and
    // lifted turn rows under the attached-session record.
    let hosted_subjects: Vec<&String> = settled
        .iter()
        .filter(|(_, _, arm)| arm.contains("hosted"))
        .map(|(_, s, _)| s)
        .collect();
    assert_eq!(hosted_subjects.len(), 2);
    for subject in &hosted_subjects {
        let evs = rows(&svc, subject.as_str());
        let classes: Vec<&str> = evs.iter().map(|(c, _)| c.as_str()).collect();
        assert!(
            classes.contains(&"lifecycle.hosted.attached"),
            "hosted subject {subject} carries the attached row: {classes:?}"
        );
        assert!(
            classes.contains(&"lifecycle.hosted.detached"),
            "hosted subject {subject} carries the detached row: {classes:?}"
        );
        assert!(
            classes.contains(&"lifecycle.component.bound"),
            "the adapter binds on the subject run: {classes:?}"
        );
        // The fixture's turn lifts real observational rows — turn
        // lifecycle on the allowlist lands verbatim.
        assert!(
            classes.contains(&"lifecycle.turn.started")
                && classes.contains(&"lifecycle.turn.finished"),
            "lifted turn rows on {subject}: {classes:?}"
        );
    }

    let report = ok(&call(
        &mut svc,
        "lab.experiment.close",
        Json::obj([("experiment_id", Json::str(&eid))]),
    ));
    assert_eq!(
        report.get("status").and_then(Json::as_str),
        Some("completed"),
        "the hosted-spine close completes at this scale: {report:?}"
    );

    // Project every settled subject into the derived-state store — the
    // same `project_and_record` the S3.4b seam serves — then compare the
    // arms through `lab.analysis.analyze`.
    let results =
        hh_results::store::ResultsStore::open(svc.store().root().join("results")).unwrap();
    let docs = hh_experiment::docs::LabDocs::open(svc.store().root()).unwrap();
    let mut keys = Vec::new();
    for (_, subject, _) in &settled {
        let (row, _v) = results
            .project_and_record(
                svc.store(),
                Some(&docs),
                subject,
                None,
                None,
                hh_results::version::DerivedReason::Initial,
            )
            .unwrap_or_else(|e| panic!("project_and_record {subject}: {e:?}"));
        keys.push(row.key.key_id());
    }

    // The analysis boundary over the projected rows — `summarize` (A1)
    // first: the composed spine's durable report. The task/suite contexts
    // name the driven cell.
    let analysis_params = |aspec: &hh_lab::analysis::AnalysisSpec| {
        Json::obj([
            ("spec", aspec.to_json()),
            (
                "rows",
                Json::Arr(keys.iter().map(|k| Json::str(k.as_str())).collect()),
            ),
            (
                "tasks",
                Json::Arr(vec![Json::obj([
                    ("task_id", Json::str("task:cap2h.0")),
                    ("suite_id", Json::str(&spec.suite.suite_ref)),
                    ("split_label", Json::str("held_out")),
                    ("split_hash", Json::str(pinned("split.hash"))),
                    ("stratum", Json::str("private_held_out")),
                ])]),
            ),
            (
                "suites",
                Json::Arr(vec![Json::obj([
                    ("suite_id", Json::str(&spec.suite.suite_ref)),
                    ("retired_for_headline", Json::Bool(false)),
                    ("family", Json::str("coding_terminal")),
                ])]),
            ),
            ("seed", Json::Int(11)),
        ])
    };
    let mut sspec = hh_lab::analysis::AnalysisSpec {
        spec_id: String::new(),
        kind: "summarize".into(),
        query: hh_lab::analysis::QuerySpec {
            metrics: vec!["task_success".into()],
            filters: None,
            grain: None,
        },
        spec_ref: Some(eid.clone()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    sspec.spec_id = sspec.spec_id();
    let r = ok(&call(
        &mut svc,
        "lab.analysis.analyze",
        analysis_params(&sspec),
    ));
    // `{record, report, body}` — the A1 body carries the per-configuration
    // summaries (native + hosted subjects alike — the projected rows fold
    // through the same `eval_run` path).
    let body = r.get("body").expect("body");
    assert_eq!(
        body.get("schema").and_then(Json::as_str),
        Some("analysis_report_body/1")
    );
    assert_eq!(body.get("kind").and_then(Json::as_str), Some("summarize"));
    let report_id = body
        .get("report_id")
        .and_then(Json::as_str)
        .expect("report_id")
        .to_string();
    assert_eq!(
        r.get("report")
            .and_then(|p| p.get("report_id"))
            .and_then(Json::as_str),
        Some(report_id.as_str()),
        "the envelope's report identity matches the body"
    );

    // The `compare` half answers honestly: `arm:hosted`'s manifest
    // carries no stamped `budget_enforcement` (the fixture reports
    // none), so `resolve_arms` keeps `BudgetEnforcement::hosted(&[])` —
    // `model_calls` is `Unenforceable` on the hosted arm and
    // `matched_cap` refuses `IncommensurableMatch` — the specified
    // verdict for a dimension the Lab cannot enforce on an unmediated
    // participant, never a fabricated parity claim. (DF-CAP.2-2 closed
    // at CAP.3 — the stamp *is* consulted; see
    // `cap2_hosted_arm_stamped_enforcement_compares` for the proven leg.)
    let mut cspec = hh_lab::analysis::AnalysisSpec {
        spec_id: String::new(),
        kind: "compare".into(),
        query: hh_lab::analysis::QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(Json::obj([
                ("arm_a", Json::str("arm:native")),
                ("arm_b", Json::str("arm:hosted")),
            ])),
            grain: None,
        },
        spec_ref: Some(eid.clone()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    cspec.spec_id = cspec.spec_id();
    let c = call(&mut svc, "lab.analysis.analyze", analysis_params(&cspec));
    assert_eq!(
        err_kind(&c),
        "Refused",
        "the matched compare refuses on the hosted arm's enforceability: {c:?}"
    );
    assert!(
        err_text(&c).contains("IncommensurableMatch"),
        "the refusal is the match contract's, verbatim: {c:?}"
    );
}

/// DF-CAP.2-2 (CAP.3) — the *proven* leg: when the hosted adapter stamps
/// `budget_enforcement` on the subject manifest (`extra
/// ["budget_enforcement"]` at `bound`), `resolve_arms` admits exactly the
/// stamped dims — `model_calls: enforced` satisfies `matched_cap`'s
/// enforceability bar and the hosted/native compare lands a real verdict
/// instead of `IncommensurableMatch`. The stamped map, never the
/// `limits_enforced` claim, is the proof.
#[test]
fn cap2_hosted_arm_stamped_enforcement_compares() {
    let mut svc = service("hosted-stamped");
    hello(&mut svc);
    let (mut hsvc, participant) = hosted_service(ungated_fixture());
    let vid = ok(&call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", participant_body(&participant)),
            (
                "registrar",
                ProvenanceRecord::kernel("hh-embed/cap2", 0).to_json(),
            ),
        ]),
    ))
    .get("version_id")
    .and_then(Json::as_str)
    .unwrap()
    .to_string();

    let spec = exp_spec(ParticipantClass::Native, Some(&vid));
    let tasks = ["task:cap2s.0"];
    let (eid, rp_arm) = exp_register_expand(&mut svc, &spec, &tasks, vec![]);
    let _exp_run = exp_open(&mut svc, &eid);

    let identity = participant.version_identity.clone();
    // The adapter proves enforcement on the matched dim — the stamp is
    // `model_calls: enforced`, the only dim `matched_cap` gates on.
    let mut host = |rpid: &str, arm: &str| {
        drive_hosted_stamped(
            &mut hsvc,
            &identity,
            rpid,
            arm,
            Some(Json::obj([("model_calls", Json::str("enforced"))])),
        )
    };
    let settled = exp_drain(
        &mut svc,
        &eid,
        &rp_arm,
        &[(DimensionId::ModelCalls, 5)],
        &|arm| !arm.contains("hosted"),
        Some((&mut host, vid.as_str())),
    );
    assert_eq!(settled.len(), 4);

    // The stamp landed on the subject manifests — the member the analyze
    // path resolves enforcement from.
    for (_, subject, arm) in &settled {
        if !arm.contains("hosted") {
            continue;
        }
        let m = svc.store().manifest(subject).unwrap();
        assert_eq!(
            m.extra
                .get("budget_enforcement")
                .and_then(|v| v.get("model_calls"))
                .and_then(Json::as_str),
            Some("enforced"),
            "the stamped map rides the subject manifest: {:?}",
            m.extra
        );
    }

    ok(&call(
        &mut svc,
        "lab.experiment.close",
        Json::obj([("experiment_id", Json::str(&eid))]),
    ));

    let results =
        hh_results::store::ResultsStore::open(svc.store().root().join("results")).unwrap();
    let docs = hh_experiment::docs::LabDocs::open(svc.store().root()).unwrap();
    let mut keys = Vec::new();
    for (_, subject, _) in &settled {
        let (row, _v) = results
            .project_and_record(
                svc.store(),
                Some(&docs),
                subject,
                None,
                None,
                hh_results::version::DerivedReason::Initial,
            )
            .unwrap_or_else(|e| panic!("project_and_record {subject}: {e:?}"));
        keys.push(row.key.key_id());
    }

    let mut cspec = hh_lab::analysis::AnalysisSpec {
        spec_id: String::new(),
        kind: "compare".into(),
        query: hh_lab::analysis::QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(Json::obj([
                ("arm_a", Json::str("arm:native")),
                ("arm_b", Json::str("arm:hosted")),
            ])),
            grain: None,
        },
        spec_ref: Some(eid.clone()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    cspec.spec_id = cspec.spec_id();
    let c = call(
        &mut svc,
        "lab.analysis.analyze",
        Json::obj([
            ("spec", cspec.to_json()),
            (
                "rows",
                Json::Arr(keys.iter().map(|k| Json::str(k.as_str())).collect()),
            ),
            (
                "tasks",
                Json::Arr(vec![Json::obj([
                    ("task_id", Json::str("task:cap2s.0")),
                    ("suite_id", Json::str(&spec.suite.suite_ref)),
                    ("split_label", Json::str("held_out")),
                    ("split_hash", Json::str(pinned("split.hash"))),
                    ("stratum", Json::str("private_held_out")),
                ])]),
            ),
            (
                "suites",
                Json::Arr(vec![Json::obj([
                    ("suite_id", Json::str(&spec.suite.suite_ref)),
                    ("retired_for_headline", Json::Bool(false)),
                    ("family", Json::str("coding_terminal")),
                ])]),
            ),
            ("seed", Json::Int(11)),
        ]),
    );
    let r = ok(&c);
    let body = r.get("body").expect("body");
    assert_eq!(body.get("kind").and_then(Json::as_str), Some("compare"));
    let bm = match body.get("comparisons") {
        Some(Json::Arr(a)) => a
            .first()
            .and_then(|c| c.get("budget_match"))
            .expect("the landed comparison carries budget_match"),
        other => panic!("comparisons: {other:?}"),
    };
    assert_eq!(
        bm.get("status").and_then(Json::as_str),
        Some("matched"),
        "the stamped-proven arm meets the matched_cap bar: {body:?}"
    );
}

/// DF-CAP.2-1 (resolved in CAP.3; ADR-0328): the lift now *shapes* the
/// hosted `permission.{requested,decided}` rows to the native
/// `security.permission.{pending,decided}` partitions. This test keeps
/// the guard the defect exercised: a lifted row carrying a member the
/// partition does not declare still refuses `SchemaViolation` at
/// `lab.hosting.attach` — shaping changed what the lift emits, never
/// what the partition admits.
#[test]
fn cap2_hosted_attach_permission_rows_refuse() {
    let mut svc = service("hosted-perm");
    hello(&mut svc);
    let (mut hsvc, participant) = hosted_service(FixtureParticipant::default());
    let vid = ok(&call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", participant_body(&participant)),
            (
                "registrar",
                ProvenanceRecord::kernel("hh-embed/cap2", 0).to_json(),
            ),
        ]),
    ))
    .get("version_id")
    .and_then(Json::as_str)
    .unwrap()
    .to_string();

    // A standalone run carries the attach (the seam failure precedes any
    // experiment involvement — the refusal is on the lifted row itself).
    let (sid, _run_id) = open_new(&mut svc, "open-hosted-perm");
    let leg = drive_hosted(&mut hsvc, &participant.version_identity, "rp", "arm:hosted");
    assert!(
        leg.rows
            .iter()
            .any(|r| r.get("class").and_then(Json::as_str) == Some("security.permission.pending")),
        "the fixture's gated call produces a lifted pending row"
    );
    // Corrupt one lifted pending row with an undeclared member — the
    // partition's member check is the guard under test.
    let mut rows = leg.rows.clone();
    for row in rows.iter_mut() {
        if row.get("class").and_then(Json::as_str) == Some("security.permission.pending") {
            if let Json::Obj(rowmap) = row {
                if let Some(Json::Obj(payload)) = rowmap.get_mut("payload") {
                    payload.insert("params".into(), Json::str("undeclared-hosted-member"));
                }
            }
            break;
        }
    }
    let r = call(
        &mut svc,
        "lab.hosting.attach",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            // `session_id` — the C0 write-authority path (the live
            // session's own writer lease, never leaving the service).
            ("session_id", Json::str(&sid)),
            (
                "session",
                Json::obj([
                    ("session_ref", Json::str(&leg.session_ref)),
                    ("abi_version", Json::str(&leg.abi_version)),
                    ("mediation", Json::str("none")),
                    ("adapter_version_id", Json::str("a/1")),
                ]),
            ),
            ("rows", Json::Arr(rows)),
            ("end_state", leg.end_state.clone()),
        ]),
    );
    // The guard: the audit-partition (Rule C) member check still refuses.
    assert_eq!(
        err_kind(&r),
        "SchemaViolation",
        "DF-CAP.2-1 observed refusal: {r:?}"
    );
    assert!(
        err_text(&r).contains("audit_fields") || err_text(&r).contains("audit-grade"),
        "the refusal names the audit partition: {r:?}"
    );
}

/// DF-CAP.2-1 residual — the composed claim the defect blocks: a hosted
/// session's permission lifecycle (`security.permission.{pending,decided}`
/// lifted rows) lands on the subject run through `lab.hosting.attach`.
/// CAP.3 (ADR-0328) resolved it by shaping the lift to the native
/// partition (`params` → `request`, `approval_wait_ms` → `wait_ms`).
#[test]
fn cap2_hosted_attach_permission_rows_land() {
    let mut svc = service("hosted-perm-ok");
    hello(&mut svc);
    let (mut hsvc, participant) = hosted_service(FixtureParticipant::default());
    let vid = ok(&call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", participant_body(&participant)),
            (
                "registrar",
                ProvenanceRecord::kernel("hh-embed/cap2", 0).to_json(),
            ),
        ]),
    ))
    .get("version_id")
    .and_then(Json::as_str)
    .unwrap()
    .to_string();
    let (sid, run_id) = open_new(&mut svc, "open-hosted-perm-ok");
    let leg = drive_hosted(&mut hsvc, &participant.version_identity, "rp", "arm:hosted");
    let r = call(
        &mut svc,
        "lab.hosting.attach",
        Json::obj([
            ("participant_ref", Json::str(&vid)),
            ("session_id", Json::str(&sid)),
            (
                "session",
                Json::obj([
                    ("session_ref", Json::str(&leg.session_ref)),
                    ("abi_version", Json::str(&leg.abi_version)),
                    ("mediation", Json::str("none")),
                    ("adapter_version_id", Json::str("a/1")),
                ]),
            ),
            ("rows", Json::Arr(leg.rows.clone())),
            ("end_state", leg.end_state.clone()),
        ]),
    );
    let a = ok(&r);
    assert_eq!(a.get("attached"), Some(&Json::Bool(true)));
    let classes: Vec<String> = rows(&svc, &run_id).iter().map(|(c, _)| c.clone()).collect();
    assert!(
        classes.iter().any(|c| c == "security.permission.pending"),
        "DF-CAP.2-1 residual: lifted pending rows land: {classes:?}"
    );
}
