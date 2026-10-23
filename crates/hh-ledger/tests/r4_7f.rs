//! R4.7f — the `subagent_anchor` + `producer_resolution` obligations graduate
//! out of `DEFERRED_OBLIGATIONS` into `OBLIGATIONS` with real evaluators
//! (DF-S1.15-1's hermetic members; §5g.6 I-A3; ADR-0066 D5):
//!
//! * `subagent_anchor` — every `control.subagent.spawned` joins the child's
//!   seq-0 `lifecycle.run.created` (`spawn_event`/`parent_anchor`, I-A6's
//!   reciprocal citation) and a `result | cancelled` carrying the verified
//!   child head (`detached` and `cancelled{spawn_interrupted}` are the
//!   declared non-anchored terminals — each verified, never assumed).
//! * `producer_resolution` — every non-kernel `producer.component_variant_ref`
//!   resolves to a `security.extension.loaded` pin attestation or a
//!   sealed-definition member (the run's `harness_def_ref` or a
//!   `lifecycle.registry.*` `version_id`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_ledger::audit::eval_obligations;
use hharness_ledger::event::{Event, EventEnvelope, EventPlane, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{
    EventRef, ObservabilityLevel, ParticipantClass, RunKind, RunManifest,
};
use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ledger::views::ViewKind;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-r47f-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str) -> Store {
    Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn agent_manifest() -> RunManifest {
    RunManifest::minimal(RunKind::Agent)
}

fn jarr(j: &Json) -> Option<&[Json]> {
    match j {
        Json::Arr(a) => Some(a.as_slice()),
        _ => None,
    }
}

/// The spawn fixture: a parent run with a `control.decision`, a
/// `control.subagent.spawned{child_run_id, delegation_ref,
/// parent_head_at_spawn}` row, and (when `open_child`) the child run opened
/// with the reciprocal `spawn_event` + `extra.parent_anchor` manifest
/// anchors — the honest I-A6 pair.
struct SpawnFixture {
    s: Store,
    parent: String,
    spawned_id: String,
    child_run: String,
    delegation_ref: String,
}

fn spawn_fixture(tag: &str, open_child: bool, anchor_ok: bool) -> SpawnFixture {
    let mut s = store(tag);
    let (parent, _lease) = s.open_run(agent_manifest(), "writer-p").unwrap();
    let decision = s
        .commit_kernel_row_for(
            "kernel.test",
            &parent,
            "control.decision",
            Json::obj([("decision", Json::str("delegate"))]),
            vec![],
            vec![],
        )
        .unwrap();
    let delegation_ref = format!("{parent}:{}", decision.event_id);
    let head = s.head(&parent).unwrap();
    let child_run = format!("{parent}-child-1");
    let spawned = s
        .commit_kernel_row_for(
            "kernel.subagent",
            &parent,
            "control.subagent.spawned",
            Json::obj([
                ("child_run_id", Json::str(&child_run)),
                ("delegation_ref", Json::str(&delegation_ref)),
                (
                    "parent_head_at_spawn",
                    Json::obj([
                        ("seq", Json::Int(head.seq as i64)),
                        ("hash", Json::str(&head.hash)),
                    ]),
                ),
            ]),
            vec![],
            vec![],
        )
        .unwrap();
    if open_child {
        let mut cm = agent_manifest();
        cm.parent_run_id = Some(parent.clone());
        cm.spawn_event = Some(EventRef {
            run_id: parent.clone(),
            event_id: spawned.event_id.clone(),
        });
        cm.extra.insert(
            "parent_anchor".to_string(),
            Json::obj([
                ("run_id", Json::str(&parent)),
                ("seq", Json::Int(head.seq as i64)),
                (
                    "hash",
                    Json::str(if anchor_ok {
                        head.hash.as_str()
                    } else {
                        "sha256:forged"
                    }),
                ),
            ]),
        );
        s.open_run_with_id(&child_run, cm, "writer-c").unwrap();
    }
    SpawnFixture {
        s,
        parent,
        spawned_id: spawned.event_id,
        child_run,
        delegation_ref,
    }
}

/// Append the parent's `control.subagent.result|cancelled|detached`
/// terminal row (`child_head` member shaped by `head`).
fn terminal(
    f: &mut SpawnFixture,
    class: &str,
    extra: Vec<(&'static str, Json)>,
    head: Option<Json>,
) {
    let mut m: Vec<(&'static str, Json)> = vec![
        ("child_run_id", Json::str(&f.child_run)),
        ("delegation_ref", Json::str(&f.delegation_ref)),
    ];
    if let Some(h) = head {
        m.push(("child_head", h));
    }
    m.extend(extra);
    f.s.commit_kernel_row_for(
        "kernel.subagent",
        &f.parent,
        class,
        Json::obj(m),
        vec![],
        vec![],
    )
    .unwrap();
}

fn finish(f: &mut SpawnFixture) {
    f.s.commit_kernel_row_for(
        "kernel.test",
        &f.parent,
        "lifecycle.run.finished",
        Json::obj([("reason", Json::str("done"))]),
        vec![],
        vec![],
    )
    .unwrap();
}

fn view_payload(s: &Store, run: &str) -> Json {
    s.project(run, ViewKind::AuditView, None)
        .unwrap()
        .payload
        .clone()
}

fn unmet(s: &Store, run: &str) -> Vec<(String, String)> {
    view_payload(s, run)
        .get("coverage")
        .and_then(|c| c.get("unmet"))
        .and_then(jarr)
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r.get("obligation_id")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        r.get("subject")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn checked(s: &Store, run: &str) -> Vec<String> {
    view_payload(s, run)
        .get("coverage")
        .and_then(|c| c.get("obligations_checked"))
        .and_then(jarr)
        .map(|rows| {
            rows.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The honest child-head claim — the child's durable tip's `{seq, hash}`.
fn child_head(f: &SpawnFixture) -> Json {
    let h = f.s.head(&f.child_run).unwrap();
    Json::obj([
        ("seq", Json::Int(h.seq as i64)),
        ("hash", Json::str(&h.hash)),
    ])
}

// ─── subagent_anchor ─────────────────────────────────────────────────────

#[test]
fn subagent_anchor_honest() {
    let mut f = spawn_fixture("honest", true, true);
    let head = child_head(&f);
    terminal(&mut f, "control.subagent.result", vec![], Some(head));
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        !rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "honest spawn→created→result must be met: {rows:?}"
    );
    assert!(
        checked(&f.s, &f.parent)
            .iter()
            .any(|o| o == "subagent_anchor"),
        "subagent_anchor must be in obligations_checked"
    );
}

#[test]
fn subagent_anchor_missing_terminal_fires() {
    let mut f = spawn_fixture("no-term", true, true);
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter()
            .any(|(o, s)| o == "subagent_anchor" && s == &f.spawned_id),
        "spawned with no result|cancelled|detached must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_no_head_fires() {
    let mut f = spawn_fixture("no-head", true, true);
    terminal(&mut f, "control.subagent.result", vec![], None);
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "a result without child_head must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_forged_head_fires() {
    let mut f = spawn_fixture("forged", true, true);
    let real = f.s.head(&f.child_run).unwrap();
    terminal(
        &mut f,
        "control.subagent.result",
        vec![],
        Some(Json::obj([
            ("seq", Json::Int(real.seq as i64)),
            ("hash", Json::str("sha256:not-the-child-head")),
        ])),
    );
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "a child_head claim contradicting the child's prefix must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_wrong_seq_fires() {
    let mut f = spawn_fixture("wrong-seq", true, true);
    // Append one more child event so seq-1 exists, then claim head at seq 1
    // with seq 0's hash — the claim does not recompute.
    f.s.commit_kernel_row_for(
        "kernel.test",
        &f.child_run,
        "context.observation.recorded",
        Json::obj([("k", Json::str("v"))]),
        vec![],
        vec![],
    )
    .unwrap();
    let seq0_hash = f.s.events(&f.child_run).unwrap()[0].hash.clone();
    terminal(
        &mut f,
        "control.subagent.result",
        vec![],
        Some(Json::obj([
            ("seq", Json::Int(1)),
            ("hash", Json::str(&seq0_hash)),
        ])),
    );
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "a child_head at the wrong seq must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_parent_anchor_mismatch_fires() {
    // The child opens, but `extra.parent_anchor.hash` contradicts the spawned
    // row's `parent_head_at_spawn` — the reciprocal citation is broken.
    let mut f = spawn_fixture("anchor-mismatch", true, false);
    let head = child_head(&f);
    terminal(&mut f, "control.subagent.result", vec![], Some(head));
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "a forged parent_anchor must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_spawn_event_mismatch_fires() {
    // The child's `spawn_event` names a different parent row — the join
    // member exists but points at the wrong event.
    let mut f = spawn_fixture("spawn-mismatch", true, true);
    // The second child's `spawn_event` names a *different* parent row (the
    // `other` decision, not the spawned row) — `parent_anchor` stays correct,
    // so the spawn_event leg is the isolated defect. The manifest is durable
    // at open: build the child with the wrong anchor deliberately.
    let other =
        f.s.commit_kernel_row_for(
            "kernel.test",
            &f.parent,
            "control.decision",
            Json::obj([("decision", Json::str("other"))]),
            vec![],
            vec![],
        )
        .unwrap();
    let ph2 = f.s.head(&f.parent).unwrap();
    let mut cm = agent_manifest();
    cm.parent_run_id = Some(f.parent.clone());
    cm.spawn_event = Some(EventRef {
        run_id: f.parent.clone(),
        event_id: other.event_id.clone(),
    });
    cm.extra.insert(
        "parent_anchor".to_string(),
        Json::obj([
            ("run_id", Json::str(&f.parent)),
            ("seq", Json::Int(ph2.seq as i64)),
            ("hash", Json::str(&ph2.hash)),
        ]),
    );
    let child2 = format!("{}-2", f.child_run);
    let spawned2_head = Json::obj([
        ("seq", Json::Int(ph2.seq as i64)),
        ("hash", Json::str(&ph2.hash)),
    ]);
    f.s.open_run_with_id(&child2, cm, "writer-c").unwrap();
    let spawned2 =
        f.s.commit_kernel_row_for(
            "kernel.subagent",
            &f.parent,
            "control.subagent.spawned",
            Json::obj([
                ("child_run_id", Json::str(&child2)),
                ("delegation_ref", Json::str(&f.delegation_ref)),
                ("parent_head_at_spawn", spawned2_head),
            ]),
            vec![],
            vec![],
        )
        .unwrap();
    let h2 = f.s.head(&child2).unwrap();
    f.s.commit_kernel_row_for(
        "kernel.subagent",
        &f.parent,
        "control.subagent.result",
        Json::obj([
            ("child_run_id", Json::str(&child2)),
            ("delegation_ref", Json::str(&f.delegation_ref)),
            (
                "child_head",
                Json::obj([
                    ("seq", Json::Int(h2.seq as i64)),
                    ("hash", Json::str(&h2.hash)),
                ]),
            ),
        ]),
        vec![],
        vec![],
    )
    .unwrap();
    // The first child gets its honest terminal too — it must stay met.
    let head = child_head(&f);
    terminal(&mut f, "control.subagent.result", vec![], Some(head));
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter()
            .any(|(o, s)| o == "subagent_anchor" && s == &spawned2.event_id),
        "a mismatched spawn_event must fire on that spawned row: {rows:?}"
    );
    assert!(
        !rows
            .iter()
            .any(|(o, s)| o == "subagent_anchor" && s == &f.spawned_id),
        "the honestly-anchored spawned row must stay met: {rows:?}"
    );
}

#[test]
fn subagent_anchor_spawn_interrupted_discharges() {
    // `cancelled{failure_kind:spawn_interrupted}` — the recovery contract's
    // honest "the child never opened" terminal; `other_events` answers None.
    let mut f = spawn_fixture("interrupted", false, true);
    terminal(
        &mut f,
        "control.subagent.cancelled",
        vec![
            ("reason", Json::str("infrastructure_failure")),
            ("failure_kind", Json::str("spawn_interrupted")),
        ],
        None,
    );
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        !rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "spawn_interrupted with no held child must be met: {rows:?}"
    );
}

#[test]
fn subagent_anchor_interrupted_contradicted_fires() {
    // `cancelled{spawn_interrupted}` while the child run IS held — the
    // record contradicts the store.
    let mut f = spawn_fixture("interrupted-held", true, true);
    terminal(
        &mut f,
        "control.subagent.cancelled",
        vec![
            ("reason", Json::str("infrastructure_failure")),
            ("failure_kind", Json::str("spawn_interrupted")),
        ],
        None,
    );
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "spawn_interrupted over a held child must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_detached_discharges() {
    // T6 — `detached` is the declared terminal for a child leaving the
    // parent's coordination; the creation anchors still verify.
    let mut f = spawn_fixture("detached", true, true);
    terminal(&mut f, "control.subagent.detached", vec![], None);
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        !rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "detached over a verified creation anchor must be met: {rows:?}"
    );
}

#[test]
fn subagent_anchor_detached_unheld_fires() {
    // `detached` names a child the store cannot show — unresolvable.
    let mut f = spawn_fixture("detached-unheld", false, true);
    terminal(&mut f, "control.subagent.detached", vec![], None);
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "detached over an unresolvable child must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_delegation_ref_mismatch_fires() {
    let mut f = spawn_fixture("deleg-mismatch", true, true);
    let head = child_head(&f);
    terminal(
        &mut f,
        "control.subagent.result",
        vec![("delegation_ref", Json::str("other-run:evt-9"))],
        Some(head),
    );
    finish(&mut f);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "a terminal citing a different decision must fire: {rows:?}"
    );
}

#[test]
fn subagent_anchor_pending_while_unfinished() {
    // `terminal_only` — a spawned-without-terminal while the parent is
    // still running is pending work, never a violation.
    let f = spawn_fixture("pending", true, true);
    let rows = unmet(&f.s, &f.parent);
    assert!(
        !rows.iter().any(|(o, _)| o == "subagent_anchor"),
        "unfinished: spawned-without-terminal is pending: {rows:?}"
    );
    assert!(
        !checked(&f.s, &f.parent)
            .iter()
            .any(|o| o == "subagent_anchor"),
        "terminal_only obligations are not checked before finished"
    );
}

// ─── producer_resolution ─────────────────────────────────────────────────

/// A hand-built envelope — the same shape `eval_obligations` folds.
fn ev(id: &str, class: &str, producer: Producer, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: id.into(),
        run_id: "run-t".into(),
        seq: 0,
        ts: TS.into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Lifecycle),
        class: class.into(),
        schema_version: 1,
        producer,
        participant_class: ParticipantClass::Native,
        observability_level: BTreeSet::from([ObservabilityLevel::Events]),
        durability: hharness_ledger::classes::Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        payload,
        hash: String::new(),
        prev_hash: String::new(),
    }
}

fn producer(class: &str, variant_ref: &str) -> Producer {
    Producer {
        component_class: class.into(),
        component_variant_ref: variant_ref.into(),
        participant_ref: "none".into(),
    }
}

fn eval(evs: &[&EventEnvelope], manifest: &RunManifest) -> Vec<String> {
    let effects = BTreeMap::new();
    let (_, unmet) = eval_obligations(evs, &effects, true, manifest, &|_: &str| -> Option<
        Vec<EventEnvelope>,
    > { None });
    unmet
        .iter()
        .filter(|u| u.obligation_id == "producer_resolution")
        .map(|u| u.subject.clone())
        .collect()
}

#[test]
fn producer_resolution_kernel_label_met() {
    // Kernel-produced rows' `component_variant_ref` is a kernel-internal
    // component label — Rule P's domain, never a registered-producer ref.
    let e = ev(
        "e-k",
        "control.decision",
        producer("kernel", "kernel.subagent"),
        Json::obj([]),
    );
    let m = RunManifest::minimal(RunKind::Agent);
    assert!(eval(&[&e], &m).is_empty());
}

#[test]
fn producer_resolution_none_and_empty_skip() {
    // `"none"` and the empty spelling are the declared unbound forms.
    for v in ["none", ""] {
        let e = ev(
            "e-n",
            "context.observation.recorded",
            producer("plugin", v),
            Json::obj([]),
        );
        let m = RunManifest::minimal(RunKind::Agent);
        assert!(eval(&[&e], &m).is_empty(), "{v:?} must be skipped");
    }
}

#[test]
fn producer_resolution_unresolved_fires() {
    let e = ev(
        "e-u",
        "context.observation.recorded",
        producer("extension", "ext:sha256:unresolved"),
        Json::obj([]),
    );
    let m = RunManifest::minimal(RunKind::Agent);
    let unmet = eval(&[&e], &m);
    assert_eq!(unmet, vec!["e-u".to_string()]);
}

#[test]
fn producer_resolution_loaded_extension_met() {
    // The `security.extension.loaded` pin attestation — `extension_id`,
    // `content` and `resolved` are all resolution targets.
    let loaded = ev(
        "e-l",
        "security.extension.loaded",
        producer("kernel", "kernel.registry"),
        Json::obj([
            ("extension_id", Json::str("ext:payments/1.2.3")),
            ("content", Json::str("sha256:ext-content")),
            ("resolved", Json::str("sha256:ext-locator")),
            ("pin_check", Json::str("pinned")),
        ]),
    );
    let m = RunManifest::minimal(RunKind::Agent);
    for (tag, v) in [
        ("extension_id", "ext:payments/1.2.3"),
        ("content-pin", "sha256:ext-content"),
        ("resolved-locator", "sha256:ext-locator"),
    ] {
        let e = ev(
            "e-p",
            "context.observation.recorded",
            producer("extension", v),
            Json::obj([]),
        );
        assert!(
            eval(&[&loaded, &e], &m).is_empty(),
            "{tag} must resolve through the loaded row"
        );
    }
}

#[test]
fn producer_resolution_sealed_definition_met() {
    // `manifest.harness_def_ref` — the run's sealed-definition member.
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some("def:sealed-harness/7".into());
    let e = ev(
        "e-s",
        "context.observation.recorded",
        producer("component", "def:sealed-harness/7"),
        Json::obj([]),
    );
    assert!(eval(&[&e], &m).is_empty());
}

#[test]
fn producer_resolution_registry_met() {
    // The registry fold — `lifecycle.registry.*` `version_id`s resolve.
    let registered = ev(
        "e-r",
        "lifecycle.registry.registered",
        producer("kernel", "registry"),
        Json::obj([
            ("kind", Json::str("variant")),
            ("version_id", Json::str("local/tracker/0.4.1")),
        ]),
    );
    let published = ev(
        "e-p",
        "lifecycle.registry.published",
        producer("kernel", "registry"),
        Json::obj([("version_id", Json::str("hh/tool/2.0.0"))]),
    );
    let m = RunManifest::minimal(RunKind::Agent);
    for (tag, v) in [
        ("registered", "local/tracker/0.4.1"),
        ("published", "hh/tool/2.0.0"),
    ] {
        let e = ev(
            "e-c",
            "context.observation.recorded",
            producer("component", v),
            Json::obj([]),
        );
        assert!(
            eval(&[&registered, &published, &e], &m).is_empty(),
            "{tag} version_id must resolve"
        );
    }
}

#[test]
fn producer_resolution_stale_version_fires() {
    // A registered name at a *different* version — the recorded version
    // must resolve, not merely the name.
    let loaded = ev(
        "e-l",
        "security.extension.loaded",
        producer("kernel", "kernel.registry"),
        Json::obj([("extension_id", Json::str("ext:payments/1.2.3"))]),
    );
    let e = ev(
        "e-v",
        "context.observation.recorded",
        producer("extension", "ext:payments/9.9.9"),
        Json::obj([]),
    );
    let m = RunManifest::minimal(RunKind::Agent);
    let unmet = eval(&[&loaded, &e], &m);
    assert_eq!(unmet, vec!["e-v".to_string()]);
}

// ─── store-level integration (audit_view path) ───────────────────────────

#[test]
fn producer_resolution_unresolved_fires_via_audit_view() {
    // A non-kernel-produced non-audit row lands through `append` (Rule P
    // admits it — the class's producer set is unrestricted); audit_view's
    // `producer_resolution` catches the unresolvable ref.
    let mut s = store("pr-view");
    let (run, lease) = s.open_run(agent_manifest(), "writer-a").unwrap();
    let e = Event {
        event_id: "evt-forged".into(),
        class: "context.observation.recorded".into(),
        ts: TS.into(),
        hlc: None,
        producer: producer("extension", "ext:never-loaded"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload: Json::obj([("k", Json::str("v"))]),
    };
    s.append(&run, &lease, vec![e]).unwrap();
    let rows = unmet(&s, &run);
    assert!(
        rows.iter()
            .any(|(o, s)| o == "producer_resolution" && s == "evt-forged"),
        "an unresolvable producer ref must fire via audit_view: {rows:?}"
    );
}

#[test]
fn coverage_membership_and_completeness() {
    // Both ids are evaluated and reported; `obligations_deferred` is the
    // empty set (R4.7f — custody is not an obligation id); an unmet
    // obligation flips `coverage_ok`/the headline.
    let mut f = spawn_fixture("coverage", true, true);
    let head = child_head(&f);
    terminal(&mut f, "control.subagent.result", vec![], Some(head));
    finish(&mut f);
    let view = view_payload(&f.s, &f.parent);
    let cov = view.get("coverage").expect("coverage");
    assert_eq!(
        cov.get("obligations_deferred"),
        Some(&Json::Arr(vec![])),
        "DEFERRED_OBLIGATIONS must be empty"
    );
    for id in ["subagent_anchor", "producer_resolution"] {
        assert!(
            checked(&f.s, &f.parent).iter().any(|o| o == id),
            "{id} must be in obligations_checked"
        );
    }
    let completeness = view.get("completeness").expect("completeness");
    assert_eq!(
        completeness.get("coverage_ok"),
        Some(&Json::Bool(true)),
        "clean coverage → coverage_ok"
    );

    // Corrupt: a spawned with no terminal + finished → unmet → coverage_ok
    // and the headline both drop.
    let mut g = spawn_fixture("coverage-bad", true, true);
    finish(&mut g);
    let view = view_payload(&g.s, &g.parent);
    let completeness = view.get("completeness").expect("completeness");
    assert_eq!(completeness.get("coverage_ok"), Some(&Json::Bool(false)));
    assert_eq!(completeness.get("headline"), Some(&Json::Bool(false)));
}
