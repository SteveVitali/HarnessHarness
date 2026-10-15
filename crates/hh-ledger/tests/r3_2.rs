//! R3.2 acceptance — the remaining DF-S2.3-1 producer legs that live
//! inside `hh-ledger` itself (BL-44):
//!
//! - **R-2.4.3 `resume_set`** — the §5c.3 materialized view is a pure
//!   fold over the durable `context.memory.written`/`invalidated`
//!   prefix (layer-S heads by name); `Store::resume_set_heads` names
//!   the heads `continue_goal` derives `carried.resume_set_heads` from;
//!   the `checkpoint` view carries the member; the fold is
//!   restart-stable (AC-R-2.4.3-7: `deriver = kernel`, rebuildable to
//!   equal `view_hash`).
//! - **R-2.12.6 non-fleet ingress** — `wakeup_subscribe` admits
//!   `external`/`manual` (follow_up) on `run_kind != fleet` manifests
//!   *only* where `extra.ingress_adapters[]` declares the coverage
//!   (`source_ref` exact or `"*"`; `manual` kind-level); undeclared is
//!   the typed `TriggerUnsupported`, never a parked subscription.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::goal::ContinueCarried;
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::views::ViewKind;
use hh_ledger::wakeup::{Coalesce, DeliveryMode, Trigger, WakeupPolicy};
use hh_ledger::LedgerError;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-r32-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open_store(tag: &str) -> Store {
    Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = open_store(tag);
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn eref(run: &str) -> EventRef {
    EventRef {
        run_id: run.to_string(),
        event_id: "e-0".to_string(),
    }
}

fn kernel_row(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

/// A `context.memory.written` row with the members the fold reads —
/// `{memory_id, version_id, kind, scope}` (the real producer's payload
/// carries more; the fold is member-tolerant).
fn mem_written(id: &str, name: &str, version: &str, scope: &str) -> Event {
    kernel_row(
        id,
        "context.memory.written",
        Json::obj([
            ("memory_id", Json::str(name)),
            ("version_id", Json::str(version)),
            ("kind", Json::str("fact")),
            ("scope", Json::str(scope)),
            ("store", Json::str("memory")),
        ]),
    )
}

fn mem_invalidated(id: &str, version: &str) -> Event {
    kernel_row(
        id,
        "context.memory.invalidated",
        Json::obj([
            ("version_id", Json::str(version)),
            ("reason", Json::str("revoked")),
            ("by", Json::str("kernel:test")),
        ]),
    )
}

fn pol() -> WakeupPolicy {
    WakeupPolicy {
        delivery_mode: DeliveryMode::FollowUp,
        coalesce: Coalesce::Latest,
        max_pending: 8,
        expires_at_ms: None,
        attendance_required: None,
        occurrence_key_fn: None,
    }
}

/// An agent manifest declaring `ingress_adapters[]` — the
/// `connection_info{ingress_adapters}` channel's durable spelling.
fn manifest_with_ingress(entries: Vec<Json>) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.extra
        .insert("ingress_adapters".to_string(), Json::Arr(entries));
    m
}

// ── R-2.4.3 — the `resume_set` producer leg ─────────────────────────────

#[test]
fn r3_2_resume_set_folds_session_heads_only() {
    let (mut s, run, lease) = open("resume-set");
    // Two session-scope names (one re-written — the head is the latest),
    // one run-scope write (never a layer-S head), one invalidated head.
    s.append(
        &run,
        &lease,
        vec![
            mem_written("m1", "ctx-1", "v1", "session"),
            mem_written("m2", "plan-9", "v2", "session"),
            mem_written("m3", "scratch", "v3", "run"),
            mem_written("m4", "ctx-1", "v4", "session"),
            mem_written("m5", "stale", "v5", "session"),
            mem_invalidated("m6", "v5"),
        ],
    )
    .unwrap();
    // `resume_set_heads` — names in canonical order; `ctx-1` resolves to
    // its latest live version; `scratch` (run-scope) and `stale`
    // (invalidated) are out.
    assert_eq!(s.resume_set_heads(&run).unwrap(), vec!["ctx-1", "plan-9"]);
    // The §5c.3 view itself — `deriver = kernel` over the durable prefix.
    let v = s.project(&run, ViewKind::ResumeSet, None).unwrap();
    assert_eq!(v.kind, ViewKind::ResumeSet);
    let heads = v
        .payload
        .get("heads")
        .and_then(|h| match h {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .expect("heads array");
    assert_eq!(heads.len(), 2, "{heads:?}");
    assert_eq!(heads[0].get("name").and_then(Json::as_str), Some("ctx-1"));
    assert_eq!(
        heads[0].get("version_id").and_then(Json::as_str),
        Some("v4"),
        "the head is the latest live version"
    );
    // The `checkpoint` view carries the member — the checkpoint is the
    // suspend/restore hand-off the resume set rides (ADR-0130 §1).
    let cp = s.project(&run, ViewKind::Checkpoint, None).unwrap();
    assert_eq!(
        cp.payload.get("resume_set_heads"),
        Some(&Json::Arr(vec![Json::str("ctx-1"), Json::str("plan-9")])),
        "{:?}",
        cp.payload
    );
    // `until` binds the watermark — a fold over a prefix drops later
    // writes/invalidations.
    let early = s
        .project(
            &run,
            ViewKind::ResumeSet,
            Some(heads_seq_of(&s, &run, "m2")),
        )
        .unwrap();
    let early_heads = early.payload.get("heads").and_then(|h| match h {
        Json::Arr(a) => Some(a.len()),
        _ => None,
    });
    assert_eq!(early_heads, Some(2), "{early:?}");
}

#[test]
fn r3_2_resume_set_is_restart_stable() {
    let d = dir("resume-set-restart");
    let run;
    {
        let mut s = Store::open_with(
            d.clone(),
            Box::new(ManualClock::at(1_000)),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (r, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        run = r;
        s.append(
            &run,
            &lease,
            vec![
                mem_written("m1", "ctx-1", "v1", "session"),
                mem_written("m2", "plan-9", "v2", "session"),
            ],
        )
        .unwrap();
    }
    // Reopen — the fold is over the WAL, never process memory.
    let s2 = Store::open_with(
        d,
        Box::new(ManualClock::at(2_000)),
        Some(Box::new(SeqIds::starting_at(1_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    assert_eq!(s2.resume_set_heads(&run).unwrap(), vec!["ctx-1", "plan-9"]);
    let v = s2.project(&run, ViewKind::ResumeSet, None).unwrap();
    assert_eq!(v.kind.as_str(), "resume_set");
}

/// `continue_goal` records the derived set verbatim — the producer leg's
/// ledger-side record: `carried.resume_set_heads` is the new activation
/// manifest's durable member (the derivation itself lives at the
/// boundary — `hh-embed`'s `continue_goal`; the ledger records what it
/// is handed).
#[test]
fn r3_2_continue_goal_records_carried_resume_set() {
    let mut s = open_store("continue-carried");
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.goal_ref = Some("goal-r32".to_string());
    let (a1, lease) = s.open_run(m, "writer-a").unwrap();
    s.append(
        &a1,
        &lease,
        vec![kernel_row(
            "fin-1",
            "lifecycle.run.finished",
            Json::obj([("stop_reason", Json::str("completed"))]),
        )],
    )
    .unwrap();
    let carried = ContinueCarried {
        resume_set_heads: vec!["ctx-1".to_string(), "plan-9".to_string()],
        budget_id: None,
    };
    let c2 = s
        .continue_goal("goal-r32", &a1, &carried, None, "writer-a")
        .unwrap();
    let m2 = s.manifest(&c2.run_id).unwrap();
    assert_eq!(
        m2.extra
            .get("carried")
            .and_then(|c| c.get("resume_set_heads")),
        Some(&Json::Arr(vec![Json::str("ctx-1"), Json::str("plan-9")])),
        "the continuation manifest records the carried set"
    );
}

fn heads_seq_of(s: &Store, run: &str, event_id: &str) -> u64 {
    s.events(run)
        .unwrap()
        .iter()
        .find(|e| e.event_id == event_id)
        .map(|e| e.seq)
        .unwrap()
}

// ── R-2.12.6 — non-fleet ingress-adapter admission ──────────────────────

#[test]
fn r3_2_external_requires_declared_adapter() {
    let (mut s, run, lease) = open("ext-undeclared");
    // A plain agent run declares nothing — `external` could never
    // produce: the typed refusal at subscribe, never a parked
    // subscription (R-2.12.6).
    let err = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "webhook".to_string(),
                source_ref: Some("ing:ci".to_string()),
                filter: None,
            },
            pol(),
            &eref(&run),
        )
        .unwrap_err();
    assert!(
        matches!(err, LedgerError::TriggerUnsupported { .. }),
        "undeclared external must refuse typed: {err:?}"
    );
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .all(|e| e.class != "control.wakeup.scheduled"),
        "no durable subscription lands"
    );
    // The same trigger admits where the manifest declares the adapter.
    let (run2, lease2) = s
        .open_run(
            manifest_with_ingress(vec![Json::obj([
                ("source_ref", Json::str("ing:ci")),
                ("triggers", Json::Arr(vec![Json::str("external")])),
            ])]),
            "writer-a",
        )
        .unwrap();
    let sub = s.wakeup_subscribe(
        &run2,
        &lease2,
        Trigger::External {
            kind: "webhook".to_string(),
            source_ref: Some("ing:ci".to_string()),
            filter: Some("event = push".to_string()),
        },
        pol(),
        &eref(&run2),
    );
    assert!(sub.is_ok(), "declared external admits: {sub:?}");
}

#[test]
fn r3_2_external_source_ref_binding() {
    let mut s = open_store("ext-source-ref");
    let (run, lease) = s
        .open_run(
            manifest_with_ingress(vec![
                Json::obj([
                    ("source_ref", Json::str("ing:ci")),
                    ("triggers", Json::Arr(vec![Json::str("external")])),
                ]),
                Json::obj([
                    ("source_ref", Json::str("*")),
                    ("triggers", Json::Arr(vec![Json::str("external")])),
                ]),
            ]),
            "writer-a",
        )
        .unwrap();
    // A `source_ref` an entry names exactly — admitted.
    assert!(s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "k".to_string(),
                source_ref: Some("ing:ci".to_string()),
                filter: None,
            },
            pol(),
            &eref(&run),
        )
        .is_ok());
    // The wildcard entry covers any other declared-source spelling.
    assert!(s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "k".to_string(),
                source_ref: Some("ing:other".to_string()),
                filter: None,
            },
            pol(),
            &eref(&run),
        )
        .is_ok());
}

#[test]
fn r3_2_external_source_ref_mismatch_refuses() {
    let mut s = open_store("ext-mismatch");
    let (run, lease) = s
        .open_run(
            manifest_with_ingress(vec![Json::obj([
                ("source_ref", Json::str("ing:ci")),
                ("triggers", Json::Arr(vec![Json::str("external")])),
            ])]),
            "writer-a",
        )
        .unwrap();
    // A `source_ref` no declared entry covers — the adapter that would
    // produce it was never registered: typed refusal.
    let err = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "k".to_string(),
                source_ref: Some("ing:unrelated".to_string()),
                filter: None,
            },
            pol(),
            &eref(&run),
        )
        .unwrap_err();
    assert!(matches!(err, LedgerError::TriggerUnsupported { .. }));
    // A `triggers` list that omits `external` declares nothing for it.
    let (run2, lease2) = s
        .open_run(
            manifest_with_ingress(vec![Json::obj([
                ("source_ref", Json::str("ing:ci")),
                ("triggers", Json::Arr(vec![Json::str("manual")])),
            ])]),
            "writer-a",
        )
        .unwrap();
    let err2 = s
        .wakeup_subscribe(
            &run2,
            &lease2,
            Trigger::External {
                kind: "k".to_string(),
                source_ref: Some("ing:ci".to_string()),
                filter: None,
            },
            pol(),
            &eref(&run2),
        )
        .unwrap_err();
    assert!(matches!(err2, LedgerError::TriggerUnsupported { .. }));
}

#[test]
fn r3_2_manual_follow_up_admitted_where_declared() {
    let mut s = open_store("manual-declared");
    // `manual` + `follow_up` off the fleet boundary — admitted where the
    // manifest declares `manual` ingress (R-2.12.6's generalization of
    // the S4.9 fleet boundary).
    let (run, lease) = s
        .open_run(
            manifest_with_ingress(vec![Json::obj([
                ("source_ref", Json::str("ing:ops")),
                ("triggers", Json::Arr(vec![Json::str("manual")])),
            ])]),
            "writer-a",
        )
        .unwrap();
    let sub = s.wakeup_subscribe(
        &run,
        &lease,
        Trigger::Manual {
            principal: "op".to_string(),
        },
        pol(),
        &eref(&run),
    );
    assert!(sub.is_ok(), "declared manual admits: {sub:?}");
    // Undeclared — the Stage-4 boundary refusal stands (the `steer` leg
    // stays admitted on any run kind — R2.6's durable queue).
    let (run2, lease2) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let err = s
        .wakeup_subscribe(
            &run2,
            &lease2,
            Trigger::Manual {
                principal: "op".to_string(),
            },
            pol(),
            &eref(&run2),
        )
        .unwrap_err();
    assert!(matches!(err, LedgerError::TriggerUnsupported { .. }));
    let steer = s.wakeup_subscribe(
        &run2,
        &lease2,
        Trigger::Manual {
            principal: "op".to_string(),
        },
        WakeupPolicy {
            delivery_mode: DeliveryMode::Steer,
            ..pol()
        },
        &eref(&run2),
    );
    assert!(steer.is_ok(), "steer leg admits on any run kind: {steer:?}");
}
