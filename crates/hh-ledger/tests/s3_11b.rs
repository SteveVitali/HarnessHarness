//! S3.11b — the §5g.6 Stage-3 audit executables (AC-R-2.8.6-{3–7,9,11,13}
//! halves that live in the ledger) plus AC-R-2.8.5-10's
//! `audit_view.extensions` provenance component.
//!
//! - AC-H6-3  — Rule P refuses a hook-produced `security.permission.decided`
//!   (`AuditProducerInvalid`); the monitor-routed row lands carrying
//!   `decider = "hook"`.
//! - AC-H6-4  — the permission dossier obligations: `tool_mediation`,
//!   `grant_scope`, `persisted_widening`; `approval_wait_ms` computable from
//!   `requested_at`.
//! - AC-H6-5  — the escalation obligations: `coverage.unmet = ∅` only when
//!   the three `lifecycle.escalation.raised` rows exist; removing one flips
//!   `coverage_ok`.
//! - AC-H6-6  — inclusion proofs for every event against the `final`
//!   checkpoint + consistency proofs between consecutive checkpoints, with
//!   the O(log n) size bound measured.
//! - AC-H6-7  — independent auditors agree on `(run, tree_size, tree_head)`;
//!   an equivocating frame is `AuditFault::Equivocation`.
//! - AC-H6-9  — export loss reports name dropped fields; lifted external
//!   rows are `unverified` + alias-linked.
//! - AC-H6-11 — killed-then-resumed is `chain_ok`; the resume absent with a
//!   later head is `Tampered{truncate}`; otherwise `scopes_closed = false`.
//! - AC-H6-13 — the `completeness` vector's `headline`/`n/a{reason}`
//!   projection half (the veto registration half lives in hh-eval).
//! - AC-R-2.8.5-10 — `audit_view.extensions{declared, effects[]}`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::audit::{
    eval_obligations, AuditFault, Auditor, CheckpointKind, FixedSigner, KeyTable,
};
use hh_ledger::errors::{LedgerError, TamperedKind};
use hh_ledger::event::{Cursor, Event, EventEnvelope, EventFrame, EventPlane, Producer, Scope};
use hh_ledger::export::{export_audit_view, lift_external, ExportConvention};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::tree;
use hh_ledger::views::ViewKind;
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-s311b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn signed_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.signer_key_ids = vec!["test-key".to_string()];
    m
}

fn open(tag: &str, manifest: RunManifest) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest, "writer-a").unwrap();
    (s, run, lease)
}

fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

/// A kernel-authored row (audit-grade classes admit only kernel producers
/// with `authority = kernel` provenance — Rule P).
fn k_ev(id: &str, class: &str, payload: Json) -> Event {
    let mut e = ev(id, class, payload);
    e.producer = Producer::kernel("kernel:test");
    e.provenance = Some(ProvenanceRecord::kernel("kernel:test", 0));
    e
}

/// An audit-grade row scoped to an open effect.
fn eff_ev(id: &str, class: &str, effect_id: &str, payload: Json) -> Event {
    let mut e = k_ev(id, class, payload);
    e.scope.effect_id = Some(effect_id.to_string());
    e
}

fn signer() -> FixedSigner {
    FixedSigner::new("test-key", b"s3.11b-test-signing-key".to_vec())
}

fn keys() -> KeyTable {
    KeyTable(BTreeMap::from([(
        "test-key".to_string(),
        b"s3.11b-test-signing-key".to_vec(),
    )]))
}

fn fill(s: &mut Store, run: &str, lease: &Lease, tag: &str, n: usize) {
    let batch: Vec<Event> = (0..n)
        .map(|i| {
            ev(
                &format!("{tag}-{i}"),
                "context.observation.recorded",
                Json::str(format!("{tag}-{i}")),
            )
        })
        .collect();
    s.append(run, lease, batch).unwrap();
}

fn decided(id: &str, effect_id: &str, decision: &str, scope_: &str) -> Event {
    k_ev(
        id,
        "security.permission.decided",
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("permission_id", Json::str(format!("perm-{id}"))),
            ("decision", Json::str(decision)),
            ("decision_scope", Json::str(scope_)),
            ("requested_at", Json::str("2026-01-01T00:00:00.000Z")),
            ("wait_ms", Json::Int(120)),
            ("decider", Json::str("policy")),
        ]),
    )
}

fn risk() -> Json {
    Json::obj([
        ("reversibility", Json::str("compensable")),
        ("repeat_safety", Json::str("idempotent")),
        ("scope", Json::str("workspace_local")),
    ])
}

/// `action.effect.intended` — opens the effect scope, carries the risk class.
fn intended(id: &str, effect_id: &str) -> Event {
    eff_ev(
        id,
        "action.effect.intended",
        effect_id,
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("effective_risk_class", risk()),
        ]),
    )
}

/// `action.effect.prepared` — the write-ahead stage `committed` requires.
fn prepared(id: &str, effect_id: &str) -> Event {
    eff_ev(
        id,
        "action.effect.prepared",
        effect_id,
        Json::obj([
            ("idempotency_key", Json::str(format!("key-{effect_id}"))),
            (
                "compensation_plan_id",
                Json::str(format!("plan-{effect_id}")),
            ),
        ]),
    )
}

/// `action.effect.authorized` — the intended→prepared transition leg.
fn authorized(id: &str, effect_id: &str) -> Event {
    eff_ev(
        id,
        "action.effect.authorized",
        effect_id,
        Json::obj([("effective_risk_class", risk())]),
    )
}

/// A fenced (post-`prepared`) `action.effect.*` row — invariant 6's
/// `fencing_token == lease.generation`.
fn fenced(id: &str, class: &str, effect_id: &str, gen: u64, extra: Json) -> Event {
    let mut payload = extra;
    if let Json::Obj(m) = &mut payload {
        m.insert("fencing_token".into(), Json::Int(gen as i64));
    }
    eff_ev(id, class, effect_id, payload)
}

/// `audit_view` member accessor helpers.
fn jbool(j: &Json) -> Option<bool> {
    match j {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}
fn jarr(j: &Json) -> Option<&[Json]> {
    match j {
        Json::Arr(a) => Some(a.as_slice()),
        _ => None,
    }
}

fn view_payload(s: &Store, run: &str) -> Json {
    s.project(run, ViewKind::AuditView, None)
        .unwrap()
        .payload
        .clone()
}
fn completeness(s: &Store, run: &str) -> Json {
    view_payload(s, run).get("completeness").unwrap().clone()
}
fn unmet(s: &Store, run: &str) -> Vec<(String, String)> {
    view_payload(s, run)
        .get("coverage")
        .and_then(|c| c.get("unmet"))
        .and_then(|u| jarr(u))
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

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-3 — Rule P: a hook cannot append an audit-grade row; the monitor
// routes the same decision in as `decider = hook`.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_3_hook_producer_refused_monitor_decider_hook_lands() {
    let (mut s, run, lease) = open("h63", signed_manifest());

    // (a) A hook-produced `security.permission.decided` — refused.
    let mut hook = decided("d-hook", "eff-1", "allow", "once");
    hook.producer = Producer {
        component_class: "hook".into(),
        component_variant_ref: "hook:v1".into(),
        participant_ref: "none".into(),
    };
    match s.append(&run, &lease, vec![hook]) {
        Err(LedgerError::AuditProducerInvalid {
            class, producer, ..
        }) => {
            assert_eq!(class, "security.permission.decided");
            assert_eq!(producer, "hook");
        }
        other => panic!("expected AuditProducerInvalid, got {other:?}"),
    }

    // (b) The kernel-producer shape with non-kernel authority — refused on
    // the second half of Rule P.
    let mut weak = decided("d-weak", "eff-2", "allow", "once");
    weak.provenance = Some(ProvenanceRecord::minted(
        Origin::model("m-x", "run-x", "r-1"),
        PersistenceScope::Run,
        0,
    ));
    match s.append(&run, &lease, vec![weak]) {
        Err(LedgerError::AuditProducerInvalid { .. }) => {}
        other => panic!("expected AuditProducerInvalid, got {other:?}"),
    }

    // (c) The same decision routed through the monitor lands as a kernel
    // row carrying `decider = "hook"` — the audit trail names who decided
    // without giving the hook write access.
    let mut via_monitor = decided("d-mon", "eff-3", "allow", "once");
    if let Json::Obj(m) = &mut via_monitor.payload {
        m.insert("decider".into(), Json::str("hook"));
    }
    s.append(&run, &lease, vec![via_monitor]).unwrap();
    let landed = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.event_id == "d-mon")
        .unwrap();
    assert_eq!(landed.producer.component_class, "kernel");
    assert_eq!(
        landed.payload.get("decider").and_then(Json::as_str),
        Some("hook")
    );
    // Rule P rechecked over stored rows — no producer violations.
    assert_eq!(
        completeness(&s, &run).get("producers_ok").and_then(jbool),
        Some(true)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-4 — the permission dossier obligations (tool_mediation, grant_scope,
// persisted_widening) + `approval_wait_ms` computable from `requested_at`.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_4_permission_dossier_coverage() {
    let (mut s, run, lease) = open("h64", signed_manifest());

    // Two non-read_only proposals, each decided exactly once; one read_only
    // proposal needing no decision; one `session`-scope decision + grant;
    // one `session`-scope decision with no grant (unmet grant_scope).
    s.append(
        &run,
        &lease,
        vec![
            k_ev(
                "p-1",
                "action.tool.proposed",
                Json::obj([
                    ("effect_id", Json::str("eff-1")),
                    ("read_only", Json::Bool(false)),
                ]),
            ),
            decided("d-1", "eff-1", "allow", "once"),
            k_ev(
                "p-2",
                "action.tool.proposed",
                Json::obj([
                    ("effect_id", Json::str("eff-2")),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            decided("d-2", "eff-3", "allow", "session"),
            k_ev(
                "g-2",
                "security.permission.granted",
                Json::obj([("permission_id", Json::str("perm-d-2"))]),
            ),
            decided("d-3", "eff-4", "deny", "session"),
            k_ev(
                "p-4",
                "action.tool.proposed",
                Json::obj([("effect_id", Json::str("eff-4"))]),
            ),
            k_ev(
                "fin",
                "lifecycle.run.finished",
                Json::obj([("reason", Json::str("done"))]),
            ),
        ],
    )
    .unwrap();

    let rows = unmet(&s, &run);
    // `grant_scope` fires on `d-3` — `decision_scope ≠ once` with no grant.
    assert!(
        rows.iter()
            .any(|(o, subj)| o == "grant_scope" && subj == "d-3"),
        "grant_scope must be unmet for the grant-less session decision: {rows:?}"
    );
    // `tool_mediation` met: p-1 has exactly one decided; p-2 is read_only;
    // p-4 has its deny.
    assert!(
        !rows.iter().any(|(o, _)| o == "tool_mediation"),
        "tool_mediation must be met: {rows:?}"
    );

    // `requested_at` + `wait_ms` durable ⇒ `approval_wait_ms` computable
    // (the decided row's own audit partition carries both).
    let d1 = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.event_id == "d-1")
        .unwrap();
    assert!(d1
        .payload
        .get("requested_at")
        .and_then(Json::as_str)
        .is_some());
    assert_eq!(d1.payload.get("wait_ms").and_then(Json::as_int), Some(120));

    // A persisted widening grant without a human `lifecycle.definition.changed`
    // is unmet (I-A5) — the obligation half that *can* run at this stage.
    let (mut s2, run2, lease2) = open("h64-w", signed_manifest());
    s2.append(
        &run2,
        &lease2,
        vec![k_ev(
            "g-wide",
            "security.permission.granted",
            Json::obj([
                ("permission_id", Json::str("perm-wide")),
                ("authority_delta", Json::str("widening")),
                ("scope", Json::str("persisted")),
            ]),
        )],
    )
    .unwrap();
    assert!(
        unmet(&s2, &run2)
            .iter()
            .any(|(o, _)| o == "persisted_widening"),
        "persisted widening without the human change row must be unmet"
    );
}

#[test]
fn ac_h6_4_persisted_widening_met_by_human_change_row() {
    // `persisted_widening` is satisfied by a human-origin
    // `lifecycle.definition.changed` — evaluate the obligation fold directly
    // over constructed envelopes (the append path's kernel-origin rule is a
    // separate concern — the *obligation* names the human-origin record).
    let mk = |id: &str, class: &str, payload: Json, prov: Option<ProvenanceRecord>| EventEnvelope {
        event_id: id.into(),
        run_id: "run-t".into(),
        seq: 0,
        ts: TS.into(),
        hlc: None,
        plane: EventPlane::Security,
        class: class.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: BTreeSet::from([ObservabilityLevel::Events]),
        durability: hh_ledger::classes::Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: prov,
        payload,
        hash: String::new(),
        prev_hash: String::new(),
    };
    let grant = mk(
        "g-1",
        "security.permission.granted",
        Json::obj([
            ("authority_delta", Json::str("widening")),
            ("scope", Json::str("persisted")),
        ]),
        Some(ProvenanceRecord::kernel("kernel:test", 0)),
    );
    let changed = mk(
        "c-1",
        "lifecycle.definition.changed",
        Json::obj([("rule_ref", Json::str("policy/egress/0"))]),
        Some(ProvenanceRecord::minted(
            Origin::human("alice", HumanRole::Principal),
            PersistenceScope::Run,
            0,
        )),
    );
    let effects = BTreeMap::new();

    // Grant alone → unmet.
    let (_, unmet) = eval_obligations(&[&grant], &effects, true);
    assert!(unmet
        .iter()
        .any(|u| u.obligation_id == "persisted_widening"));
    // With the human-origin change row → met.
    let (_, unmet) = eval_obligations(&[&grant, &changed], &effects, true);
    assert!(
        !unmet
            .iter()
            .any(|u| u.obligation_id == "persisted_widening"),
        "a human-origin definition.changed discharges the obligation"
    );
    // A *kernel*-origin change row does not — the obligation names the human.
    let changed_kernel = mk(
        "c-2",
        "lifecycle.definition.changed",
        Json::obj([("rule_ref", Json::str("policy/egress/0"))]),
        Some(ProvenanceRecord::kernel("kernel:test", 0)),
    );
    let (_, unmet) = eval_obligations(&[&grant, &changed_kernel], &effects, true);
    assert!(unmet
        .iter()
        .any(|u| u.obligation_id == "persisted_widening"));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-5 — the escalation obligations: `coverage.unmet = ∅` only when the
// three `lifecycle.escalation.raised` rows exist.
// ─────────────────────────────────────────────────────────────────────────────

/// The AC-H6-5 fixture: an abandoned effect, an exhausted budget, and an
/// `unknown` effect still open at `finished` — with the escalations named.
/// `esc_mask` selects which of the three escalations to append.
fn escalation_run(tag: &str, esc_mask: u8) -> (Store, String) {
    let (mut s, run, lease) = open(tag, signed_manifest());
    let gen = lease.generation;
    let mut batch = vec![
        // eff-a — abandoned, carrying its escalation ref.
        intended("i-a", "eff-a"),
        // eff-u — committed, then `unknown{worker_lost}` (the crash path) —
        // still open at `finished`.
        intended("i-u", "eff-u"),
        authorized("a-u", "eff-u"),
        decided("d-u", "eff-u", "allow", "once"),
        prepared("p-u", "eff-u"),
        fenced(
            "c-u",
            "action.effect.committed",
            "eff-u",
            gen,
            Json::obj([("attempt_no", Json::Int(1))]),
        ),
        fenced(
            "u-u",
            "action.effect.unknown",
            "eff-u",
            gen,
            Json::obj([("cause", Json::str("worker_lost"))]),
        ),
        // The budget exhaustion row.
        k_ev(
            "b-1",
            "control.budget.exceeded",
            Json::obj([
                ("budget_id", Json::str("bud-1")),
                ("dimension", Json::str("tokens")),
            ]),
        ),
    ];
    // Escalation rows land before the abandoned row that references one.
    let mut escs = Vec::new();
    if esc_mask & 0b001 != 0 {
        escs.push(k_ev(
            "esc-a",
            "lifecycle.escalation.raised",
            Json::obj([("kind", Json::str("effect_abandoned"))]),
        ));
    }
    if esc_mask & 0b010 != 0 {
        escs.push(k_ev(
            "esc-b",
            "lifecycle.escalation.raised",
            Json::obj([("budget_id", Json::str("bud-1"))]),
        ));
    }
    if esc_mask & 0b100 != 0 {
        escs.push(k_ev(
            "esc-u",
            "lifecycle.escalation.raised",
            Json::obj([("effect_id", Json::str("eff-u"))]),
        ));
    }
    // Insert escalations before the abandoned row (esc-a must exist first —
    // `escalation_ref` is resolved at append, not only at audit time).
    let abandoned = fenced(
        "ab-a",
        "action.effect.abandoned",
        "eff-a",
        gen,
        Json::obj([
            ("escalation_ref", Json::str("esc-a")),
            ("reason", Json::str("operator_abandoned")),
        ]),
    );
    // Order: intentions+unknown+budget, escalations, abandoned, finished.
    batch.append(&mut escs);
    batch.push(abandoned);
    batch.push(k_ev(
        "fin",
        "lifecycle.run.finished",
        Json::obj([("reason", Json::str("done"))]),
    ));
    s.append(&run, &lease, batch).unwrap();
    (s, run)
}

#[test]
fn ac_h6_5_escalation_obligations_coverage_flips() {
    // All three escalations → the three escalation obligations met. (The
    // still-open `unknown` is itself `effect_terminal`-unmet at `finished` —
    // the escalation discharges `unknown_escalated`, it does not close the
    // scope; that residual is the honest unmet, never hidden.)
    let (s, run) = escalation_run("h65-all", 0b111);
    let rows = unmet(&s, &run);
    for ob in [
        "abandoned_escalated",
        "budget_hard_escalated",
        "unknown_escalated",
    ] {
        assert!(
            !rows.iter().any(|(o, _)| o == ob),
            "{ob} must be met: {rows:?}"
        );
    }
    assert!(
        rows.iter()
            .any(|(o, subj)| o == "effect_terminal" && subj == "eff-u"),
        "the still-open unknown stays unmet under effect_terminal: {rows:?}"
    );

    // The clean coverage flip (the AC's success-with-veto hinge): a bare
    // `budget.exceeded` at `finished` is `coverage_ok = false`; adding its
    // escalation is the only change needed to flip it true.
    for (tag, esc) in [("h65-flip-no", false), ("h65-flip-yes", true)] {
        let (mut s, run, lease) = open(tag, signed_manifest());
        let mut batch = vec![k_ev(
            "b-1",
            "control.budget.exceeded",
            Json::obj([
                ("budget_id", Json::str("bud-1")),
                ("dimension", Json::str("tokens")),
            ]),
        )];
        if esc {
            batch.push(k_ev(
                "esc-b",
                "lifecycle.escalation.raised",
                Json::obj([("budget_id", Json::str("bud-1"))]),
            ));
        }
        batch.push(k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        ));
        s.append(&run, &lease, batch).unwrap();
        assert_eq!(
            completeness(&s, &run).get("coverage_ok").and_then(jbool),
            Some(esc),
            "{tag}: coverage_ok must be {esc}"
        );
    }

    // Remove the unknown-effect escalation → `unknown_escalated` unmet.
    let (s, run) = escalation_run("h65-nou", 0b011);
    let rows = unmet(&s, &run);
    assert!(rows.iter().any(|(o, _)| o == "unknown_escalated"));
    // Remove the budget escalation → `budget_hard_escalated` unmet.
    let (s, run) = escalation_run("h65-nob", 0b101);
    assert!(unmet(&s, &run)
        .iter()
        .any(|(o, _)| o == "budget_hard_escalated"));
    // The abandonment leg is enforced at append: `escalation_ref` must
    // resolve to a `lifecycle.escalation.raised` row — an unresolvable ref
    // never lands (the stronger guarantee; the audit-side obligation is the
    // recheck).
    let (mut s, run, lease) = open("h65-noa", signed_manifest());
    let gen = lease.generation;
    s.append(&run, &lease, vec![intended("i-a", "eff-a")])
        .unwrap();
    let orphaned = fenced(
        "ab-a",
        "action.effect.abandoned",
        "eff-a",
        gen,
        Json::obj([
            ("escalation_ref", Json::str("esc-missing")),
            ("reason", Json::str("operator_abandoned")),
        ]),
    );
    assert!(matches!(
        s.append(&run, &lease, vec![orphaned]),
        Err(LedgerError::UnresolvedEventRef { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-6 — inclusion proofs for every event against the `final` checkpoint;
// consistency proofs between consecutive checkpoints; O(log n) reported.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_6_proofs_cover_every_event_and_bound_is_log() {
    let (mut s, run, lease) = open("h66", signed_manifest());
    let mut checkpoint_seqs = Vec::new();
    // Three batches with a checkpoint after each, then finished + final.
    for (tag, n) in [("a", 7usize), ("b", 9), ("c", 11)] {
        fill(&mut s, &run, &lease, tag, n);
        let cp = s
            .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
            .unwrap();
        checkpoint_seqs.push(cp.seq);
    }
    s.append(
        &run,
        &lease,
        vec![k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        )],
    )
    .unwrap();
    let fin = s
        .checkpoint(&run, &lease, CheckpointKind::Final, &mut signer())
        .unwrap();
    checkpoint_seqs.push(fin.seq);
    let final_size = fin.payload.get("tree_size").and_then(Json::as_int).unwrap() as u64;
    let final_head = fin
        .payload
        .get("tree_head")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // (i) Inclusion: every event against the final signed head.
    let events = s.events(&run).unwrap();
    let mut max_path = 0usize;
    for e in events.iter().filter(|e| e.seq < final_size) {
        let proof = s.prove_inclusion(&run, e.seq, Some(final_size)).unwrap();
        assert!(
            tree::verify_inclusion(&proof, &e.hash, &final_head),
            "inclusion proof for seq {} must verify against the final head",
            e.seq
        );
        max_path = max_path.max(proof.path.len());
    }
    // O(log n): the longest path never exceeds ceil(log2(size)) + 1.
    let bound = (64 - (final_size.max(2) - 1).leading_zeros() + 1) as usize;
    assert!(
        max_path <= bound,
        "inclusion path {max_path} exceeds the O(log n) bound {bound} (n={final_size})"
    );

    // (ii) Consistency: every consecutive checkpoint pair.
    for w in checkpoint_seqs.windows(2) {
        let (s1, s2) = (w[0], w[1]);
        let head1 = tree::mth_prefix(
            &events.iter().map(|e| e.hash.clone()).collect::<Vec<_>>(),
            s1 as usize,
        );
        let head2 = tree::mth_prefix(
            &events.iter().map(|e| e.hash.clone()).collect::<Vec<_>>(),
            s2 as usize,
        );
        let proof = s.prove_consistency(&run, s1, s2).unwrap();
        assert!(
            tree::verify_consistency(&proof, &head1, &head2),
            "consistency proof {s1}→{s2} must verify"
        );
        assert!(
            proof.path.len() <= bound + 1,
            "consistency path exceeds the O(log n) bound"
        );
    }
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-7 — independent auditors agree; an equivocating frame is detected.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_7_auditors_agree_equivocation_detected() {
    let (mut s, run, lease) = open("h67", signed_manifest());
    fill(&mut s, &run, &lease, "a", 6);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    fill(&mut s, &run, &lease, "b", 5);
    s.checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut signer())
        .unwrap();

    // Auditor A — fed by `subscribe` (the durable stream as delivered).
    let mut sub = s.subscribe(&run, Cursor::Seq(0)).unwrap();
    let mut aud_a = Auditor::new(&run);
    while let Some(frame) = sub.try_next() {
        aud_a.observe(&frame, Some(&keys())).unwrap();
    }
    // Auditor B — fed from `read`-equivalent rows (`audit_with` replays the
    // durable prefix, the results-store reader's path).
    let mut aud_b = Auditor::new(&run);
    s.audit_with(&run, &mut aud_b, Some(&keys())).unwrap();

    // Agreement on every held (tree_size, tree_head).
    assert_eq!(aud_a.held_heads(), aud_b.held_heads());
    assert_eq!(aud_a.held_heads().len(), 2);
    assert_eq!(aud_a.head(), aud_b.head());
    // And the heads the auditors hold are the store's own recompute.
    let leaves: Vec<String> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    for (size, head) in aud_a.held_heads() {
        assert_eq!(*head, tree::mth_prefix(&leaves, *size as usize));
    }

    // The equivocating-writer fixture: a frame whose bytes do not recompute
    // to the delivered hash — a fork presented as the same seq.
    let mut aud_c = Auditor::new(&run);
    let frames: Vec<EventFrame> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| EventFrame::Durable {
            seq: e.seq,
            hash: e.hash.clone(),
            event: Box::new(e.clone()),
        })
        .collect();
    for f in &frames[..3] {
        aud_c.observe(f, None).unwrap();
    }
    // Forge seq 3: same slot, different content under a replayed hash.
    let EventFrame::Durable { seq, hash, event } = &frames[3] else {
        panic!("durable frame expected")
    };
    let mut forged = (**event).clone();
    forged.payload = Json::str("forked-payload");
    let fault = aud_c.observe(
        &EventFrame::Durable {
            seq: *seq,
            hash: hash.clone(),
            event: Box::new(forged),
        },
        None,
    );
    assert!(
        matches!(fault, Err(AuditFault::Equivocation { at_seq: 3, .. })),
        "the equivocating frame must surface as Equivocation, got {fault:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-9 — export loss reports name the dropped fields; lifted external
// rows are `unverified` and alias-linked (T-LCD-11).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_9_export_loss_reports_and_unverified_lift() {
    let (mut s, run, lease) = open("h69", signed_manifest());
    fill(&mut s, &run, &lease, "a", 4);
    // A scoped row so `scope` is among the members the lowering must name.
    s.append(
        &run,
        &lease,
        vec![
            intended("i-1", "eff-1"),
            decided("d-1", "eff-1", "allow", "once"),
        ],
    )
    .unwrap();
    let events = s.events(&run).unwrap();
    let view = view_payload(&s, &run);
    let view_members: Vec<String> = match &view {
        Json::Obj(m) => m.keys().cloned().collect(),
        _ => vec![],
    };

    // external_audit_record — carries identity + chain coordinates.
    let exp = export_audit_view(events, &view_members, ExportConvention::ExternalAuditRecord);
    assert_eq!(exp.loss_report.rows_emitted, events.len() as u64);
    assert!(exp
        .loss_report
        .dropped_fields
        .contains(&"payload".to_string()));
    assert!(
        exp.loss_report
            .dropped_fields
            .contains(&"provenance".to_string()),
        "the record convention drops provenance — the loss report names it"
    );
    assert!(exp
        .loss_report
        .dropped_view_members
        .contains(&"completeness".to_string()));

    // telemetry — cannot carry the hash chain at all.
    let tel = export_audit_view(events, &view_members, ExportConvention::Telemetry);
    for f in [
        "hash",
        "prev_hash",
        "producer",
        "scope",
        "provenance",
        "payload",
    ] {
        assert!(
            tel.loss_report.dropped_fields.contains(&f.to_string()),
            "telemetry loss report must name {f}"
        );
    }
    // The rows themselves carry only what the convention admits.
    let Json::Obj(row) = &tel.rows[0] else {
        panic!("telemetry row must be an object")
    };
    assert!(row.contains_key("name") && row.contains_key("attributes"));
    assert!(!row.contains_key("hash"));

    // The lift direction — every external row is `unverified`, linked by
    // alias back to its own chain.
    let lifted = lift_external(&[
        Json::obj([
            ("record_id", Json::str("ext-0")),
            ("kind", Json::str("deny")),
        ]),
        Json::obj([("id", Json::str("ext-1")), ("name", Json::str("span"))]),
        Json::obj([("kind", Json::str("orphan"))]), // no id → positional alias
    ]);
    assert_eq!(lifted.len(), 3);
    assert_eq!(lifted[0].alias, "ext-0");
    assert_eq!(lifted[1].alias, "ext-1");
    assert_eq!(lifted[2].alias, "external:2");
    assert_eq!(lifted[2].kind.as_deref(), Some("orphan"));
    for row in &lifted {
        assert_eq!(row.verification_status, "unverified");
    }
    assert_eq!(
        lifted[0]
            .to_json()
            .get("verification_status")
            .and_then(Json::as_str),
        Some("unverified")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-11 — killed mid-batch + resumed ⇒ `chain_ok`; the resume absent with
// a later head ⇒ `Tampered{truncate}`; absent with no later head ⇒ the run
// reports `scopes_closed = false` (a crash is indistinguishable from a
// truncation — the record keeps the two answers honest).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_11_killed_resumed_chain_ok_and_honest_truncation() {
    // (a) Kill mid-batch (a committed effect, no terminal) → restore lands
    // `lifecycle.run.resumed{recovery_decision}` → `chain_ok`.
    let (mut s, run, lease) = open("h611-resume", signed_manifest());
    let gen = lease.generation;
    s.append(
        &run,
        &lease,
        vec![
            intended("i-1", "eff-1"),
            eff_ev("a-1", "action.effect.authorized", "eff-1", Json::obj([])),
            decided("d-1", "eff-1", "allow", "once"),
            prepared("p-1", "eff-1"),
            eff_ev(
                "c-1",
                "action.effect.committed",
                "eff-1",
                Json::obj([
                    ("attempt_no", Json::Int(1)),
                    ("fencing_token", Json::Int(gen as i64)),
                ]),
            ),
        ],
    )
    .unwrap();
    s.release(&lease, "test-simulated-crash").unwrap();
    let rep = s.restore_caused(&run, "writer-b", 60_000, "crash").unwrap();
    let resumed = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.event_id == rep.resumed_event_id)
        .unwrap();
    assert_eq!(resumed.class, "lifecycle.run.resumed");
    assert!(resumed.payload.get("recovery_decision").is_some());
    s.verify(&run).unwrap();
    assert_eq!(
        completeness(&s, &run).get("chain_ok").and_then(jbool),
        Some(true)
    );

    // (b) The same kill without the resume: the prefix verifies (a crash is
    // a valid durable prefix) — but a caller claiming a later head gets
    // `Tampered{truncate}`, and `scopes_closed` is honestly false (the
    // effect scope never closed).
    let (mut s2, run2, lease2) = open("h611-kill", signed_manifest());
    let gen2 = lease2.generation;
    s2.append(
        &run2,
        &lease2,
        vec![
            intended("i-2", "eff-2"),
            authorized("a-2", "eff-2"),
            decided("d-2", "eff-2", "allow", "once"),
            prepared("p-2", "eff-2"),
            eff_ev(
                "c-2",
                "action.effect.committed",
                "eff-2",
                Json::obj([
                    ("attempt_no", Json::Int(1)),
                    ("fencing_token", Json::Int(gen2 as i64)),
                ]),
            ),
        ],
    )
    .unwrap();
    // The prefix itself is sound.
    s2.verify(&run2).unwrap();
    // A claimed later head → truncate.
    let tip = s2.head(&run2).unwrap().seq;
    match s2.verify_run(&run2, None, Some(tip + 3), None) {
        Err(LedgerError::Tampered(t)) => assert_eq!(t.kind, TamperedKind::Truncate),
        other => panic!("expected Tampered{{truncate}}, got {other:?}"),
    }
    // No claimed head → `scopes_closed = false` (the open effect is
    // reported, never smoothed over).
    let comp = completeness(&s2, &run2);
    assert_eq!(comp.get("scopes_closed").and_then(jbool), Some(false));
    assert_eq!(comp.get("chain_ok").and_then(jbool), Some(true));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H6-13 (projection half) — the `completeness` vector: `headline` reflects
// every component; `n/a{reason}` is an honest non-failure, never a silent
// false and never a fabricated true.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h6_13_completeness_headline_and_na_reasons() {
    // An unsigned run: checkpoints and cross-run report `n/a{reason}`.
    let (mut s, run, lease) = open("h613-na", RunManifest::minimal(RunKind::Agent));
    fill(&mut s, &run, &lease, "a", 3);
    let comp = completeness(&s, &run);
    assert_eq!(comp.get("headline").and_then(jbool), Some(true));
    for member in ["checkpoints_ok", "cross_run_ok"] {
        let v = comp.get(member).unwrap();
        assert!(
            v.get("n/a").and_then(Json::as_str).is_some(),
            "{member} must render n/a{{reason}}, got {v:?}"
        );
    }

    // A run with an unmet obligation: `coverage_ok` false → headline false
    // (the eval side renders this success-with-veto).
    let (mut s2, run2, lease2) = open("h613-fail", signed_manifest());
    s2.append(
        &run2,
        &lease2,
        vec![
            k_ev(
                "p-1",
                "action.tool.proposed",
                Json::obj([("effect_id", Json::str("eff-x"))]),
            ),
            k_ev(
                "fin",
                "lifecycle.run.finished",
                Json::obj([("reason", Json::str("done"))]),
            ),
        ],
    )
    .unwrap();
    let comp2 = completeness(&s2, &run2);
    assert_eq!(comp2.get("coverage_ok").and_then(jbool), Some(false));
    assert_eq!(comp2.get("headline").and_then(jbool), Some(false));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.5-10 — `audit_view.extensions`: the run's declared extension ids
// plus the per-effect context/producer answer.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_r_2_8_5_10_audit_view_extensions_provenance() {
    let (mut s, run, lease) = open("h-ext", signed_manifest());
    let mut assembled = ev(
        "ctx-1",
        "context.assembled",
        Json::obj([
            ("model_call_id", Json::str("mc-1")),
            (
                "context_label",
                Json::obj([("taint", Json::Arr(vec![Json::str("extension:ext-x")]))]),
            ),
        ]),
    );
    assembled.producer = Producer {
        component_class: "context".into(),
        component_variant_ref: "none".into(),
        participant_ref: "none".into(),
    };
    assembled.provenance = Some(ProvenanceRecord::kernel("kernel:context", 0));
    assembled.scope.model_call_id = Some("mc-1".into());
    assembled.scope.turn_id = Some("t-1".into());

    s.append(
        &run,
        &lease,
        vec![
            {
                let mut e = k_ev("t-1", "lifecycle.turn.started", Json::obj([]));
                e.scope.turn_id = Some("t-1".into());
                e
            },
            {
                let mut e = k_ev("mc-1", "model.call.requested", Json::obj([]));
                e.scope.turn_id = Some("t-1".into());
                e.scope.model_call_id = Some("mc-1".into());
                e
            },
            assembled,
            // The extension's own declaration row — `declared` + the
            // contributes table the producer side joins through.
            k_ev(
                "ext-1",
                "security.extension.resolved",
                Json::obj([
                    ("extension_id", Json::str("ext-y")),
                    ("contributes", Json::Arr(vec![Json::str("cap-x")])),
                ]),
            ),
            {
                let mut e = intended("i-1", "eff-1");
                if let Json::Obj(m) = &mut e.payload {
                    m.insert("capability".into(), Json::str("cap-x"));
                }
                e.scope.model_call_id = Some("mc-1".into());
                e.scope.turn_id = Some("t-1".into());
                e
            },
        ],
    )
    .unwrap();

    let ext = view_payload(&s, &run).get("extensions").unwrap().clone();
    assert_eq!(
        ext.get("declared")
            .and_then(jarr)
            .map(|a| a.iter().filter_map(Json::as_str).collect::<Vec<_>>()),
        Some(vec!["ext-y"])
    );
    let effects = ext.get("effects").and_then(jarr).unwrap();
    assert_eq!(effects.len(), 1);
    let row = &effects[0];
    assert_eq!(row.get("effect_id").and_then(Json::as_str), Some("eff-1"));
    assert_eq!(
        row.get("model_call_id").and_then(Json::as_str),
        Some("mc-1")
    );
    assert_eq!(
        row.get("capability_ref").and_then(Json::as_str),
        Some("cap-x")
    );
    assert_eq!(
        row.get("context_extensions")
            .and_then(jarr)
            .map(|a| a.iter().filter_map(Json::as_str).collect::<Vec<_>>()),
        Some(vec!["ext-x"]),
        "the proposing call's extension taint must project onto the effect"
    );
    assert_eq!(
        row.get("producer_extensions")
            .and_then(jarr)
            .map(|a| a.iter().filter_map(Json::as_str).collect::<Vec<_>>()),
        Some(vec!["ext-y"]),
        "the capability's contributing extension must name the producer"
    );
}
