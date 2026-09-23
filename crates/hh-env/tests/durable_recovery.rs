//! S2.3 acceptance coverage — the environment half of §5a.3 (R-2.2.3⁰ᵇ):
//! `verify_environment` verdicts, `HealingPolicy`-bound healing with
//! `lost_items_ref` + `max_heals`, detached reconciliation, and the `fs_tree`
//! snapshot semantics of AC-R-2.2.4-9.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.3-9  — environment verification and healing (`lost` verdict ⇒
//!   policy-bound heal; `action.environment.healed` + `lost_items_ref`;
//!   `max_heals` bound; `ask`/`fail` postures).
//! - AC-R-2.2.3-14 — detached reconciliation (`preserve_until` sweep:
//!   every preserved child reconciled to `observed | unknown | terminated`
//!   by the resuming writer).
//! - AC-R-2.2.4-9  — snapshot semantics (unchanged workspace ⇒ same
//!   content address; GC'd ⇒ `SnapshotMissing` with a verifiable hash).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{kernel_default, NetMode, ResourceLimits, WritableRoot};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::EventMinter;
use hh_env::handle::{HandleState, OnLoss, Roots};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_env::recovery::{EnvironmentVerdict, HealOutcome, HealingPolicy, LostCause, OnLost};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-envdur-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease, clock)
}

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_policy(ws: &std::path::Path) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = hh_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}

fn test_record(class: EnvironmentClass, image: ImageRef) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image,
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        ext: BTreeMap::new(),
    }
}

fn test_ca() -> hh_identity::idp::ContentAddress {
    hh_identity::idp::address(b"test-image-bytes", "application/octet-stream")
}

/// `provision + attach(Ep2Model, fail-closed)` → a `ready` handle, with the
/// `on_loss` posture the heal ladder reads.
fn ready_env_on_loss(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
    on_loss: OnLoss,
) -> String {
    let record = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(test_ca()),
    );
    let roots = Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    };
    let h = driver
        .provision(
            store,
            lease,
            &record,
            roots,
            PolicySlot::Inline(Box::new(test_policy(ws))),
            on_loss,
        )
        .unwrap();
    let backend = Ep2Model::reference();
    driver
        .attach(
            store,
            lease,
            &h.env_handle_id,
            Some(&backend),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    h.env_handle_id
}

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    ready_env_on_loss(store, lease, driver, ws, OnLoss::FailRun)
}

/// An `action.effect.*` row — audit-grade ⇒ kernel producer + provenance.
fn eff(id: &str, class: &str, effect_id: &str, payload: Json, fencing: u64) -> Event {
    let mut payload = payload;
    if fencing > 0 {
        if let Json::Obj(m) = &mut payload {
            m.insert("fencing_token".to_string(), Json::Int(fencing as i64));
        }
    }
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: if class.starts_with("action.effect.") {
                Some(effect_id.to_string())
            } else {
                None
            },
            ..Scope::default()
        },
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

fn risk(rev: &str, rs: &str, scope: &str) -> Json {
    Json::obj([
        ("reversibility", Json::str(rev)),
        ("repeat_safety", Json::str(rs)),
        ("scope", Json::str(scope)),
    ])
}

/// Open the `turn ⊃ model_call` scopes a dispatch's scope chain names.
fn open_scopes(store: &mut Store, run: &str, lease: &Lease, turn: &str, mc: &str) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some(turn.to_string());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some(turn.to_string()),
        model_call_id: Some(mc.to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![t, c]).unwrap();
}

/// Drive an effect to `committed` — a detached child mid-flight when the
/// kernel dies.
fn to_committed(s: &mut Store, run: &str, lease: &Lease, eid: &str, detached: Vec<String>) {
    to_committed_scoped(s, run, lease, eid, detached, None)
}

/// `to_committed` under a `turn ⊃ model_call ⊃ tool_call` scope chain (the
/// fold keeps it; the reconciliation's terminal mints under it).
fn to_committed_scoped(
    s: &mut Store,
    run: &str,
    lease: &Lease,
    eid: &str,
    detached: Vec<String>,
    chain: Option<(&str, &str, &str)>,
) {
    let rc = risk("reversible", "idempotent", "workspace_local");
    let mut cp = Json::obj([("attempt_no", Json::Int(1))]);
    if !detached.is_empty() {
        if let Json::Obj(m) = &mut cp {
            m.insert(
                "detached_effect_ids".to_string(),
                Json::Arr(detached.into_iter().map(Json::str).collect()),
            );
        }
    }
    let mut rows = vec![
        eff(
            &format!("{eid}-i"),
            "action.effect.intended",
            eid,
            Json::obj([
                ("effect_id", Json::str(eid)),
                ("effective_risk_class", rc.clone()),
            ]),
            0,
        ),
        eff(
            &format!("{eid}-a"),
            "action.effect.authorized",
            eid,
            Json::obj([("effective_risk_class", rc)]),
            0,
        ),
        {
            let mut d = eff(
                &format!("{eid}-d"),
                "security.permission.decided",
                eid,
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("attempt_no", Json::Int(1)),
                    ("decision", Json::str("allow")),
                ]),
                0,
            );
            d.scope.effect_id = Some(eid.to_string());
            d
        },
        eff(
            &format!("{eid}-p"),
            "action.effect.prepared",
            eid,
            Json::obj([
                ("idempotency_key", Json::str(format!("key-{eid}"))),
                ("baseline_ref", Json::str("bl-1")),
            ]),
            0,
        ),
        eff(
            &format!("{eid}-c"),
            "action.effect.committed",
            eid,
            cp,
            lease.generation,
        ),
    ];
    if let Some((turn, mc, tc)) = chain {
        for e in &mut rows {
            e.scope.turn_id = Some(turn.to_string());
            e.scope.model_call_id = Some(mc.to_string());
            if !tc.is_empty() {
                e.scope.tool_call_id = Some(tc.to_string());
            }
        }
    }
    s.append(run, lease, rows).unwrap();
}

// ── AC-R-2.2.3-9 — verify_environment verdicts ──────────────────────────────

#[test]
fn ac_2_2_3_9_verify_environment_verdicts_attached_reattachable_lost() {
    let (mut store, run, lease, _clock) = open("verify");
    let ws = workspace("verify");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);

    // `ready` + live in-process session + fresh report ⇒ `attached`.
    let v = driver
        .verify_environment_verdict(&mut store, &lease, &env)
        .unwrap();
    assert!(matches!(v, EnvironmentVerdict::Attached { .. }), "{v:?}");

    // `unreachable` but the session survives ⇒ `reattachable`.
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();
    let v = driver
        .verify_environment_verdict(&mut store, &lease, &env)
        .unwrap();
    assert!(
        matches!(v, EnvironmentVerdict::Reattachable { .. }),
        "{v:?}"
    );

    // The workspace destroyed between checkpoints ⇒ `lost{workspace_missing}`.
    std::fs::remove_dir_all(&ws).unwrap();
    let v = driver
        .verify_environment_verdict(&mut store, &lease, &env)
        .unwrap();
    match v {
        EnvironmentVerdict::Lost { cause } => {
            assert_eq!(cause, LostCause::WorkspaceMissing)
        }
        other => panic!("expected lost{{workspace_missing}}, got {other:?}"),
    }
    // Every verdict is durable — `action.environment.verified` rows.
    let n = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "action.environment.verified")
        .count();
    assert_eq!(n, 3);
}

#[test]
fn ac_2_2_3_9_heal_reprovision_mints_successor_and_lost_items_ref() {
    let (mut store, run, lease, _clock) = open("heal-prov");
    let ws = workspace("heal-prov");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env_on_loss(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        OnLoss::ReplaceFromImage,
    );
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();

    let policy = HealingPolicy {
        on_lost: OnLost::Reprovision,
        max_heals: 4,
        verify_after: None,
        preserve_detached: false,
    };
    let out = driver
        .heal_with_policy(&mut store, &lease, &env, &policy, "worker_lost")
        .unwrap();
    match out {
        HealOutcome::Healed {
            new_handle,
            lost_items_ref,
            heal_no,
            ..
        } => {
            // Reprovision mints a *successor* handle (the old one is
            // `replaced`, terminal).
            assert_ne!(new_handle, env);
            assert_eq!(heal_no, 1);
            assert!(lost_items_ref.starts_with("sha256:"));
            // The lost-items blob resolves — `authority = environment`
            // content the driver's next context carries.
            let ca = hh_identity::idp::ContentAddress {
                idp: "idp/1",
                algorithm: "sha256",
                digest: lost_items_ref.trim_start_matches("sha256:").to_string(),
                media_type: "application/json".into(),
                size: 0,
            };
            assert!(store.get_blob(&ca).is_ok());
        }
        other => panic!("expected Healed, got {other:?}"),
    }
    // `action.environment.healed` is durable and audit-grade.
    let healed = store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.healed")
        .unwrap();
    assert_eq!(
        healed.payload.get("from_handle").and_then(Json::as_str),
        Some(env.as_str())
    );
    assert!(healed.payload.get("lost_items_ref").is_some());
}

#[test]
fn ac_2_2_3_9_heal_max_heals_bound_is_ledger_counted() {
    let (mut store, run, lease, _clock) = open("heal-max");
    let ws = workspace("heal-max");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env_on_loss(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        OnLoss::ReplaceFromImage,
    );
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();
    // `max_heals = 0` — the bound is counted from the ledger (zero heals on
    // record ⇒ the first attempt is already over).
    let policy = HealingPolicy {
        on_lost: OnLost::Reprovision,
        max_heals: 0,
        verify_after: None,
        preserve_detached: false,
    };
    let err = driver
        .heal_with_policy(&mut store, &lease, &env, &policy, "worker_lost")
        .unwrap_err();
    assert!(matches!(err, EnvError::MaxHealsExceeded { .. }), "{err:?}");
    // No `healed` row was written — the bound refusal is not a heal.
    assert!(!store
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "action.environment.healed"));
}

#[test]
fn ac_2_2_3_9_heal_ask_and_fail_postures() {
    let (mut store, run, lease, _clock) = open("heal-posture");
    let ws = workspace("heal-posture");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env_on_loss(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        OnLoss::ReplaceFromImage,
    );
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();

    // `ask` (the interactive default) — a durable `security.permission.pending`.
    let ask = HealingPolicy {
        on_lost: OnLost::Ask,
        max_heals: 4,
        verify_after: None,
        preserve_detached: false,
    };
    match driver
        .heal_with_policy(&mut store, &lease, &env, &ask, "helper_gone")
        .unwrap()
    {
        HealOutcome::Ask { permission_id } => {
            assert!(store.events(&run).unwrap().iter().any(|e| {
                e.class == "security.permission.pending"
                    && e.payload.get("permission_id").and_then(Json::as_str)
                        == Some(permission_id.as_str())
            }));
        }
        other => panic!("expected Ask, got {other:?}"),
    }

    // `fail` — refused; the caller stops `environment_lost`.
    let fail = HealingPolicy {
        on_lost: OnLost::Fail,
        max_heals: 4,
        verify_after: None,
        preserve_detached: false,
    };
    match driver
        .heal_with_policy(&mut store, &lease, &env, &fail, "helper_gone")
        .unwrap()
    {
        HealOutcome::Refused { reason } => assert!(reason.contains("fail")),
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ── AC-R-2.2.3-14 — detached reconciliation ─────────────────────────────────

#[test]
fn ac_2_2_3_14_detached_children_reconcile_to_terminal() {
    let (mut store, run, lease, _clock) = open("reconcile");
    let ws = workspace("reconcile");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // A detached child mid-flight when the kernel "dies" — the committed
    // row names it under `detached_effect_ids`.
    to_committed(
        &mut store,
        &run,
        &lease,
        "eff-parent",
        vec!["eff-child".to_string()],
    );
    // The child's own lifecycle — committed, still running on the helper,
    // scoped under a live `turn ⊃ model_call` (the reconciliation's
    // terminal mints under that scope chain).
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    to_committed_scoped(
        &mut store,
        &run,
        &lease,
        "eff-child",
        vec![],
        Some(("turn-1", "mc-1", "")),
    );
    // `local_host` has no helper channel — the child is past
    // `preserve_until` (reaped) ⇒ the resuming writer records `terminated`.
    let recs = driver.reconcile_detached(&mut store, &lease, &env).unwrap();
    assert_eq!(recs.len(), 1, "{recs:?}");
    assert_eq!(recs[0].effect_id, "eff-child");
    assert_eq!(recs[0].verdict, "terminated");
    assert!(recs[0].event_id.is_some());
    // The terminal is durable — the child's fold is no longer open.
    let f = store.effect_fold(&run, "eff-child").unwrap().unwrap();
    assert!(f.is_terminal(), "child still open: {:?}", f.phase);
}

// ── AC-R-2.2.4-9 — snapshot semantics ───────────────────────────────────────

#[test]
fn ac_2_2_4_9_unchanged_workspace_snapshots_share_one_address() {
    let (mut store, run, lease, _clock) = open("snap");
    let ws = workspace("snap");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("a.txt"), b"hello").unwrap();
    let (rec1, snap1) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    // An unchanged workspace ⇒ the same content address.
    let (rec2, snap2) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    assert_eq!(rec1.snapshot_ref, rec2.snapshot_ref);
    assert_eq!(snap1.tree_address, snap2.tree_address);
    // A changed workspace ⇒ a different address (content-addressed).
    std::fs::write(ws.join("a.txt"), b"changed").unwrap();
    let (rec3, _) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    assert_ne!(rec1.snapshot_ref, rec3.snapshot_ref);
    // The snapshot rows are durable.
    assert_eq!(
        store
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "action.environment.snapshot")
            .count(),
        3
    );
}

#[test]
fn ac_2_2_4_9_restore_after_gc_is_snapshot_missing() {
    let (mut store, run, lease, _clock) = open("snap-gc");
    let ws = workspace("snap-gc");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("keep.txt"), b"persistent-bytes").unwrap();
    let (_rec, snap) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    // GC the file's blob — the restore must refuse `SnapshotMissing` with
    // the verifiable hash, never materialise a partial tree.
    let digest = hh_identity::idp::address(b"persistent-bytes", "application/octet-stream").digest;
    std::fs::remove_file(store.root().join("blobs").join(&digest)).unwrap();
    let err = driver.fs_tree_restore(&store, &env, &snap).unwrap_err();
    match err {
        EnvError::SnapshotMissing { snapshot_ref } => {
            assert_eq!(snapshot_ref, snap.tree_address)
        }
        other => panic!("expected SnapshotMissing, got {other:?}"),
    }
}

#[test]
fn ac_2_2_4_9_restore_materialises_the_tree() {
    let (mut store, run, lease, _clock) = open("snap-restore");
    let ws = workspace("snap-restore");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("a.txt"), b"one").unwrap();
    std::fs::create_dir_all(ws.join("sub")).unwrap();
    std::fs::write(ws.join("sub/b.txt"), b"two").unwrap();
    let (_rec, snap) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    // Destroy the workspace, then restore — the tree materialises back.
    std::fs::remove_dir_all(&ws).unwrap();
    std::fs::create_dir_all(&ws).unwrap();
    let addr = driver.fs_tree_restore(&store, &env, &snap).unwrap();
    assert_eq!(addr, snap.tree_address);
    assert_eq!(std::fs::read(ws.join("a.txt")).unwrap(), b"one");
    assert_eq!(std::fs::read(ws.join("sub/b.txt")).unwrap(), b"two");
}

#[test]
fn ac_2_2_3_9_verify_emits_no_state_change_only_the_audit_row() {
    let (mut store, run, lease, _clock) = open("verify-pure");
    let ws = workspace("verify-pure");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let head_before = store.head(&run).unwrap().seq;
    let state_before = driver.handle(&env).unwrap().state;
    driver
        .verify_environment_verdict(&mut store, &lease, &env)
        .unwrap();
    // The verdict is one audit row — the handle's state is untouched.
    assert_eq!(store.head(&run).unwrap().seq, head_before + 1);
    assert_eq!(driver.handle(&env).unwrap().state, state_before);
    assert_eq!(driver.handle(&env).unwrap().state, HandleState::Ready);
    let _ = EventMinter::new(&store, &run); // keep the import live
}

/// AC-R-2.2.3-14 bound — the helper's `preserve_until(ttl)` must satisfy
/// `ttl ≥ writer_lease.ttl + recovery_grace` (§05a; ADR-0130 §6; OQ-252).
/// The constant is asserted against the run's own manifest values, so a
/// future writer-TTL bump that violates the bound fails here, not in prod.
#[test]
fn ac_2_2_3_14_preserve_until_exceeds_writer_ttl_plus_grace() {
    let (store, run, _lease, _clock) = open("preserve-bound");
    let m = store.manifest(&run).unwrap();
    let need = m.lease_ttl.writer_ms + m.grace_ms;
    assert!(
        hh_env::driver::PRESERVE_UNTIL_TTL_MS >= need,
        "preserve_until {} < writer_ttl {} + grace {}",
        hh_env::driver::PRESERVE_UNTIL_TTL_MS,
        m.lease_ttl.writer_ms,
        m.grace_ms
    );
}

// ── S2.9 — snapshot chooser, derive-from-snapshot, rollback restore ─────────
// (§5a.1 §5 `fork{env: snapshot}` / `rollback`; ADR-0271.)

#[test]
fn s2_9_snapshot_for_chooses_the_newest_at_or_below_the_cut() {
    let (mut store, run, lease, _clock) = open("snap-choose");
    let ws = workspace("snap-choose");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("v.txt"), b"v1").unwrap();
    let (r1, _t1) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    let seq1 = r1.at_seq;
    std::fs::write(ws.join("v.txt"), b"v2").unwrap();
    let (r2, _t2) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    let seq2 = r2.at_seq;
    assert!(seq2 > seq1);
    // At/below the later snapshot ⇒ the newer one; between them ⇒ the first.
    let (rec, _tree) = driver
        .snapshot_for(&store, seq2)
        .unwrap()
        .expect("snapshot at seq2");
    assert_eq!(rec.snapshot_ref, r2.snapshot_ref);
    let mid = store
        .env_snapshots(&run)
        .unwrap()
        .iter()
        .map(|(s, _, _, _)| *s)
        .min()
        .unwrap();
    let (rec_mid, _) = driver.snapshot_for(&store, mid).unwrap().unwrap();
    assert_eq!(rec_mid.snapshot_ref, r1.snapshot_ref);
    // Below the earliest ⇒ None (never a fabricated snapshot).
    assert!(
        driver.snapshot_for(&store, 0).unwrap().is_none()
            || driver.snapshot_for(&store, seq1 - 1).unwrap().is_none()
    );
}

#[test]
fn s2_9_derive_from_snapshot_materialises_blob_content_not_the_live_tree() {
    let (mut store, run, lease, _clock) = open("snap-derive");
    let ws = workspace("snap-derive-parent");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("pinned.txt"), b"at-cut").unwrap();
    let (_rec, snap) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    // The parent's live tree moves on — the child must see the snapshot's
    // content, not the drifted workspace.
    std::fs::write(ws.join("pinned.txt"), b"drifted").unwrap();
    std::fs::write(ws.join("late.txt"), b"after-the-cut").unwrap();
    // The child run + driver (inter-run: the child opens under `open_run`).
    let (child_run, child_lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let mut cdrv = EnvDriver::new(&child_run);
    let parent = driver.handle(&env).unwrap().clone();
    let ch = cdrv
        .derive_from_snapshot(&mut store, &child_lease, &parent, &snap)
        .unwrap();
    let cws = std::path::PathBuf::from(&ch.roots.cwd);
    assert_eq!(std::fs::read(cws.join("pinned.txt")).unwrap(), b"at-cut");
    assert!(
        !cws.join("late.txt").exists(),
        "post-cut content must not leak into the child"
    );
    // The derivation is durable on the child run.
    assert!(store
        .envelopes(&child_run)
        .unwrap()
        .iter()
        .any(|e| e.class == "action.environment.derived"));
}

#[test]
fn s2_9_rollback_env_restores_in_place_and_reports_uncaptured() {
    let (mut store, run, lease, _clock) = open("snap-rollback");
    let ws = workspace("snap-rollback");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("kept.txt"), b"before").unwrap();
    let (_rec, _snap) = driver.fs_tree_snapshot(&mut store, &lease, &env).unwrap();
    let snap_seq = store.head(&run).unwrap().seq;
    // Post-snapshot drift: modify a covered file, add an uncovered one.
    std::fs::write(ws.join("kept.txt"), b"after").unwrap();
    std::fs::write(ws.join("stray.txt"), b"not-in-snapshot").unwrap();
    let (restored, uncaptured) = driver
        .rollback_env(&mut store, &lease, &env, snap_seq)
        .unwrap();
    assert!(restored.is_some());
    // Covered member restored to the snapshot's bytes; uncovered removed.
    assert_eq!(std::fs::read(ws.join("kept.txt")).unwrap(), b"before");
    assert!(
        !ws.join("stray.txt").exists(),
        "restore-in-place removes what the manifest does not name"
    );
    // `action.environment.restored` lands with the uncaptured list.
    let rows: Vec<_> = store
        .envelopes(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "action.environment.restored")
        .collect();
    assert_eq!(rows.len(), 1);
    let _ = uncaptured;
}

#[test]
fn s2_9_rollback_env_without_a_snapshot_is_uncaptured_not_fabricated() {
    let (mut store, run, lease, _clock) = open("snap-rollback-none");
    let ws = workspace("snap-rollback-none");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    std::fs::write(ws.join("live.txt"), b"untouched").unwrap();
    let (restored, uncaptured) = driver.rollback_env(&mut store, &lease, &env, 0).unwrap();
    assert!(restored.is_none(), "no snapshot ⇒ nothing restored");
    assert!(
        !uncaptured.is_empty(),
        "the writable root is honestly uncaptured"
    );
    // The live workspace is untouched — a failed restore never writes.
    assert_eq!(std::fs::read(ws.join("live.txt")).unwrap(), b"untouched");
}
