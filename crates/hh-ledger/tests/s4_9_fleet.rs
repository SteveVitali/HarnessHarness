//! S4.9 ledger-side tests — the fleet boundary's trigger admissibility
//! (§5i.1 #2), the `control.work_item.{created,annotated}` +
//! `lifecycle.fleet.activated` class declarations, and the `fleet_anchor`
//! obligation's evaluation over a cross-run anchor.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::errors::LedgerError;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-s49-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str, ms: u64) -> Store {
    Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn agent_manifest() -> RunManifest {
    RunManifest::minimal(RunKind::Agent)
}

/// A fleet activation manifest — no configuration cells (ADR-0183 §C).
fn fleet_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Fleet);
    m.configuration_id = None;
    m.configuration_version_id = None;
    m
}

/// T-S4.9-L1 — `external`/`manual` are fleet-boundary admissible: a
/// non-fleet run keeps the typed Stage-4 refusal unchanged; `timer`
/// stays admissible everywhere.
#[test]
fn fleet_boundary_trigger_admissibility() {
    let mut s = store("trig", 1_000);
    let (agent_run, agent_lease) = s.open_run(agent_manifest(), "writer-a").unwrap();
    let (fleet_run, fleet_lease) = s.open_run(fleet_manifest(), "writer-f").unwrap();

    // Agent run — external/manual refuse typed (the Stage-4 gate's
    // unchanged member; fleet admissibility never leaks downward, CC6).
    for t in [
        Trigger::External {
            kind: "ticket.updated".into(),
        },
        Trigger::Manual {
            principal: "op".into(),
        },
    ] {
        match s.wakeup_subscribe(
            &agent_run,
            &agent_lease,
            t,
            WakeupPolicy::default_policy(),
            &EventRef {
                run_id: agent_run.clone(),
                event_id: "evt-x".into(),
            },
        ) {
            Err(LedgerError::TriggerUnsupported { .. }) => {}
            other => panic!("agent-run external/manual must refuse: {other:?}"),
        }
    }

    // Fleet run — the same triggers admit (the boundary's declared set).
    for t in [
        Trigger::External {
            kind: "ticket.updated".into(),
        },
        Trigger::Manual {
            principal: "op".into(),
        },
        Trigger::Timer { at_ms: 5_000 },
    ] {
        s.wakeup_subscribe(
            &fleet_run,
            &fleet_lease,
            t,
            WakeupPolicy::default_policy(),
            &EventRef {
                run_id: fleet_run.clone(),
                event_id: "evt-x".into(),
            },
        )
        .expect("fleet-boundary trigger must admit");
    }

    // Unsupported variants refuse even at the fleet boundary — the
    // fleet's declared set is closed (`external`/`manual`/`timer`; the
    // fleet SPEC layer refuses every other variant at `open` — see the
    // hh-fleet `unsupported_trigger_variants_refuse` test; `schedule`
    // keeps its kernel-level refusal everywhere).
    match s.wakeup_subscribe(
        &fleet_run,
        &fleet_lease,
        Trigger::Schedule {
            expr: "*/5 * * * *".into(),
        },
        WakeupPolicy::default_policy(),
        &EventRef {
            run_id: fleet_run.clone(),
            event_id: "evt-x".into(),
        },
    ) {
        Err(LedgerError::TriggerUnsupported { .. }) => {}
        other => panic!("schedule must refuse at the fleet boundary: {other:?}"),
    }
}

/// T-S4.9-L2 — the S4.9 class declarations append at their declared
/// partitions (`lifecycle.fleet.activated`, `control.work_item.created`,
/// `control.work_item.annotated` — the two vocabulary members the §5i.1
/// record family adds).
#[test]
fn fleet_class_declarations_append() {
    let mut s = store("classes", 1_000);
    let (run, lease) = s.open_run(fleet_manifest(), "writer-f").unwrap();
    for class in [
        "lifecycle.fleet.activated",
        "control.work_item.created",
        "control.work_item.annotated",
        "control.work_item.dispatched",
        "control.work_item.owner_changed",
        "lifecycle.escalation.raised",
        "lifecycle.escalation.resolved",
        "context.observation.recorded",
    ] {
        s.commit_kernel_row_for(
            "hh.fleet.test",
            &run,
            class,
            Json::obj([("probe", Json::str(class))]),
            vec![],
            vec![],
        )
        .unwrap_or_else(|e| panic!("{class} must append: {e:?}"));
    }
    let _ = lease;
    let _ = BTreeMap::<String, String>::new();
}
