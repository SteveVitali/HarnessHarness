//! R2.4 — the declared `snapshot_cadence` producer (DF-S2.9-3) and the
//! lifecycle substrate (`upload`/`download`/`diff`/`restore`, DF-S2.10-1)
//! at the `EnvDriver` layer.
//!
//! The boundary batteries (`crates/hh-embed/tests/r2_4.rs` +
//! `cap_2_composed`'s un-ignored cadence leg) drive these paths over
//! `hh-embed/1`; this battery pins the driver semantics directly:
//!
//! - `cadence_take` fires only on the declared trigger, takes `fs_tree`
//!   on a `ready` local handle, takes `memory` on a `suspended` provider
//!   handle whose class declares `snapshot.memory = supported`, and skips
//!   — `Ok(None)`, never a fabricated row — when no declared kind is
//!   state-eligible.
//! - `every_n_effects:N` counts the durable `action.effect.*` terminal
//!   rows since the newest snapshot — a pure prefix fold, no shadow
//!   counters.
//! - `env_upload`/`env_download` move content-addressed bytes through a
//!   writable root + the blob pool; `fs_tree_diff`/`restore_successor`
//!   round-trip a baseline.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_containment::attach::PolicySlot;
use hh_containment::policy::ResourceLimits;
use hh_env::driver::{EnvDriver, SnapshotTrigger};
use hh_env::errors::EnvError;
use hh_env::events::EventMinter;
use hh_env::handle::{EnvCapabilityDeclaration, HandleState, OnLoss, Roots, SnapshotCadence, Tri};
use hh_env::provider::{ProviderAdapter, ProviderHandle, ProvisionRequest};
use hh_env::record::{
    CanaryChannel, EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe,
};
use hh_env::snapshot::{SnapshotKind, TakenBy};
use hh_identity::idp::address;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r24-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let d = dir(tag);
    std::fs::create_dir_all(&d).unwrap();
    // Canonicalized — `store.root()` names derived env workspaces; the
    // driver's `canonicalize` resolves `/var` → `/private/var` before the
    // roots check.
    let d = std::fs::canonicalize(&d).unwrap();
    let mut s = Store::open_with(
        d,
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn test_record(class: EnvironmentClass) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image: ImageRef::ContentAddress(address(b"r24-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: canary_channels_for(class),
    }
}

/// R2.10 (DF-S1.13-3) — a Stage-3-class (`needs_adapter`) image manifest
/// MUST declare ≥1 canary channel; `provision`/`provision_hosted` register
/// each on the run's credential broker (the §5g.3 §9 tripwire — the broker
/// never injects it; any use is `leak_detected` + `Refused{canary}`).
fn test_canary(channel_id: &str) -> CanaryChannel {
    CanaryChannel {
        channel_id: channel_id.to_string(),
        spec: hh_secrets::SecretChannelSpec {
            kind: hh_secrets::CredentialKind::ApiKey,
            source: hh_secrets::SecretSource::OperatorVault {
                vault_ref: format!("canary:{channel_id}"),
            },
            destinations: vec![],
            allowed_env_names: None,
            delivery_modes: std::collections::BTreeSet::new(),
            max_lifetime_ms: None,
            rotation_policy: None,
            sender_constraint: hh_secrets::SenderConstraint::None,
            constraints: Default::default(),
            bindable: false,
            access_class: hh_secrets::AccessClass::KernelOnly,
            canary: true,
            description: "image-manifest tripwire".into(),
        },
    }
}

fn canary_channels_for(class: EnvironmentClass) -> Vec<CanaryChannel> {
    if class.needs_adapter() {
        vec![test_canary("img.canary")]
    } else {
        vec![]
    }
}

fn test_broker() -> hh_secrets::CredentialBroker {
    hh_secrets::CredentialBroker::new(
        Box::new(hh_secrets::StaticVault::default()),
        "canary-test-key",
    )
}

/// A workspace with one file already in it — `fs_tree` walks real bytes.
fn roots_with_seed(tag: &str) -> (Roots, PathBuf) {
    let ws = dir(tag);
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("seed.txt"), b"seed").unwrap();
    // `/var` → `/private/var` (macOS): the driver's canonicalizer resolves
    // symlinks — the declared roots must name the resolved path.
    let ws = std::fs::canonicalize(&ws).unwrap();
    (
        Roots {
            workspace_roots: vec![ws.display().to_string()],
            writable_roots: vec![ws.display().to_string()],
            cwd: ws.display().to_string(),
        },
        ws,
    )
}

/// Provision a `local_host` handle and attach it for real — the
/// `Ep2Model` backend installs the session + containment report
/// `verify_environment` gates on (acceptance.rs's `ready_env` shape).
fn provision_local(
    s: &mut Store,
    lease: &Lease,
    d: &mut EnvDriver,
    tag: &str,
) -> (String, PathBuf) {
    let (roots, ws) = roots_with_seed(tag);
    let mut pol = hh_containment::policy::kernel_default(1_000);
    for r in &roots.writable_roots {
        pol.fs
            .write
            .allow
            .push(hh_containment::policy::WritableRoot {
                root: r.clone(),
                read_only_subpaths: vec![],
                protected_metadata_names: vec![],
            });
    }
    let h = d
        .provision(
            s,
            lease,
            &test_record(EnvironmentClass::LocalHost),
            roots,
            PolicySlot::Inline(Box::new(pol)),
            OnLoss::FailRun,
            None,
        )
        .unwrap();
    let backend = hh_containment::backend::Ep2Model::reference();
    d.attach(
        s,
        lease,
        &h.env_handle_id,
        Some(&backend),
        hh_containment::attach::AttachMode::FailClosed,
        true,
        &[],
    )
    .unwrap();
    (h.env_handle_id, ws)
}

/// The provider adapter declaring `snapshot.memory = supported` —
/// S5.8's `MemProvider` shape (the remote side is an honest stub: the
/// adapter answers, the kernel records a `foreign_digest` claim).
#[derive(Default)]
struct Calls {
    memory_snapshots: usize,
}

struct MemProvider {
    calls: Arc<Mutex<Calls>>,
}

impl ProviderAdapter for MemProvider {
    fn class(&self) -> EnvironmentClass {
        EnvironmentClass::RemoteEphemeral
    }
    fn capability_declaration(&self) -> EnvCapabilityDeclaration {
        let mut c = EnvCapabilityDeclaration::provider_class();
        c.snapshot.insert("memory".to_string(), Tri::Supported);
        c
    }
    fn provision(&mut self, req: &ProvisionRequest) -> Result<ProviderHandle, EnvError> {
        Ok(ProviderHandle {
            remote_id: format!("remote:{}", req.env_handle_id),
            region: None,
            extra: BTreeMap::new(),
        })
    }
    fn suspend(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn resume(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn teardown(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn memory_snapshot(&mut self, h: &ProviderHandle) -> Result<String, EnvError> {
        self.calls.lock().unwrap().memory_snapshots += 1;
        Ok(format!("provider-snap:{}", h.remote_id))
    }
}

fn provision_remote(s: &mut Store, lease: &Lease, d: &mut EnvDriver, tag: &str) -> String {
    let (roots, _) = roots_with_seed(tag);
    // R2.10 — Stage-3-class provisions register the manifest's canary on
    // the run's broker.
    let mut broker = test_broker();
    let h = d
        .provision(
            s,
            lease,
            &test_record(EnvironmentClass::RemoteEphemeral),
            roots,
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    let id = h.env_handle_id.clone();
    let hh = d.handle_mut(&id).unwrap();
    if hh.state == HandleState::Declared {
        hh.transition(HandleState::Provisioning, 1_000).unwrap();
    }
    if hh.state == HandleState::Provisioning {
        hh.transition(HandleState::Ready, 1_001).unwrap();
    }
    id
}

fn snapshot_rows(s: &Store, run: &str) -> Vec<(u64, Json)> {
    s.events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "action.environment.snapshot")
        .map(|e| (e.seq, e.payload.clone()))
        .collect()
}

// ── cadence_take ────────────────────────────────────────────────────────────

/// `on_turn_end` on a `ready` local handle fires `fs_tree` at the turn
/// boundary — `taken_by:"cadence"` durable, never an instrument call.
#[test]
fn cadence_on_turn_end_takes_fs_tree_at_the_boundary() {
    let (mut s, run, lease) = open("cad-tt");
    let mut d = EnvDriver::new(&run);
    let (id, _ws) = provision_local(&mut s, &lease, &mut d, "tt-ws");
    d.set_cadence(&id, SnapshotCadence::OnTurnEnd).unwrap();
    assert_eq!(snapshot_rows(&s, &run).len(), 0);
    let rec = d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::TurnBoundary)
        .unwrap()
        .expect("on_turn_end fires");
    assert_eq!(rec.kind, SnapshotKind::FsTree);
    assert_eq!(rec.taken_by, TakenBy::Cadence);
    let rows = snapshot_rows(&s, &run);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].1.get("taken_by").and_then(Json::as_str),
        Some("cadence")
    );
    assert_eq!(
        rows[0].1.get("snapshot_ref").and_then(Json::as_str),
        Some(rec.snapshot_ref.as_str())
    );
}

/// The declared cadence fires *only* on its own trigger — every other
/// boundary answers `Ok(None)` (never a fabricated row).
#[test]
fn cadence_fires_only_on_the_declared_trigger() {
    let (mut s, run, lease) = open("cad-trig");
    let mut d = EnvDriver::new(&run);
    let (id, _ws) = provision_local(&mut s, &lease, &mut d, "tr-ws");
    d.set_cadence(&id, SnapshotCadence::OnTurnEnd).unwrap();
    for t in [SnapshotTrigger::Idle, SnapshotTrigger::EffectSettled] {
        assert!(
            d.cadence_take(&mut s, &lease, &id, t).unwrap().is_none(),
            "on_turn_end must not fire on {t:?}"
        );
    }
    assert_eq!(snapshot_rows(&s, &run).len(), 0);
    // And the `never` default — an undeclared cadence produces nothing.
    let (id2, _ws2) = provision_local(&mut s, &lease, &mut d, "tr-ws2");
    for t in [
        SnapshotTrigger::TurnBoundary,
        SnapshotTrigger::Idle,
        SnapshotTrigger::EffectSettled,
    ] {
        assert!(d.cadence_take(&mut s, &lease, &id2, t).unwrap().is_none());
    }
    assert_eq!(snapshot_rows(&s, &run).len(), 0);
}

/// A suspended provider handle whose class declares `memory` takes the
/// provider checkpoint at the boundary (the memory leg of the cadence —
/// quiesced, `foreign_digest`, adapter-backed).
#[test]
fn cadence_takes_provider_memory_where_declared_and_quiesced() {
    let (mut s, run, lease) = open("cad-mem");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "mem-ws");
    d.suspend(&mut s, &lease, &id, None).unwrap();
    d.set_cadence(&id, SnapshotCadence::OnTurnEnd).unwrap();
    let rec = d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::TurnBoundary)
        .unwrap()
        .expect("memory take fires on the suspended declared handle");
    assert_eq!(rec.kind, SnapshotKind::Memory);
    assert!(rec.quiesced);
    assert_eq!(calls.lock().unwrap().memory_snapshots, 1);
}

/// A `ready` provider handle — `memory` declared but not quiesced,
/// `fs_tree` undeclared — skips honestly (`Ok(None)`, no row). The
/// declaration is satisfiable (suspend first); the boundary is not.
#[test]
fn cadence_skips_when_no_declared_kind_is_takeable() {
    let (mut s, run, lease) = open("cad-skip");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "sk-ws");
    d.set_cadence(&id, SnapshotCadence::OnTurnEnd).unwrap();
    assert!(d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::TurnBoundary)
        .unwrap()
        .is_none());
    assert_eq!(calls.lock().unwrap().memory_snapshots, 0);
    assert_eq!(snapshot_rows(&s, &run).len(), 0);
}

/// `every_n_effects:N` counts durable `action.effect.*` terminal rows
/// since the newest snapshot — the fold over the prefix, no shadow
/// counter. Two effects settle, `n=2` fires on the second; a snapshot
/// resets the window.
#[test]
fn cadence_every_n_effects_counts_the_durable_prefix() {
    let (mut s, run, lease) = open("cad-n");
    let mut d = EnvDriver::new(&run);
    let (id, _ws) = provision_local(&mut s, &lease, &mut d, "n-ws");
    d.set_cadence(&id, SnapshotCadence::EveryNEffects(2))
        .unwrap();
    // The effect scope opens with `intended` and the terminal row the
    // cadence counts is `refused` (the Π-decider's direct refusal — a
    // lawful `intended → refused` transition that skips the
    // prepare/commit chain).
    let settle = |s: &mut Store, lease: &Lease, n: u64| {
        let m = EventMinter::new(s, &run);
        let mut i = m
            .mint(
                "action.effect.intended",
                Json::obj([
                    ("effect_id", Json::str(format!("eff-{n}"))),
                    ("capability", Json::str("host.exec.shell")),
                    (
                        "effective_risk_class",
                        Json::obj([
                            ("reversibility", Json::str("reversible")),
                            ("repeat_safety", Json::str("idempotent")),
                            ("scope", Json::str("workspace_local")),
                        ]),
                    ),
                ]),
            )
            .unwrap();
        i.scope.effect_id = Some(format!("eff-{n}"));
        let mut o = m
            .mint(
                "action.effect.refused",
                Json::obj([
                    ("effect_id", Json::str(format!("eff-{n}"))),
                    ("attempt_no", Json::Int(1)),
                    ("decider", Json::str("kernel")),
                    ("reason", Json::str("denied")),
                    ("reason_code", Json::str("denied")),
                    ("fencing_token", Json::Int(lease.generation as i64)),
                ]),
            )
            .unwrap();
        o.scope.effect_id = Some(format!("eff-{n}"));
        s.append(&run, lease, vec![i, o]).unwrap();
    };
    settle(&mut s, &lease, 1);
    assert!(d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::EffectSettled)
        .unwrap()
        .is_none());
    settle(&mut s, &lease, 2);
    assert!(d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::EffectSettled)
        .unwrap()
        .is_some());
    // The take resets the window — a third trigger with no new settled
    // effect does not fire again.
    assert!(d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::EffectSettled)
        .unwrap()
        .is_none());
    settle(&mut s, &lease, 3);
    settle(&mut s, &lease, 4);
    assert!(d
        .cadence_take(&mut s, &lease, &id, SnapshotTrigger::EffectSettled)
        .unwrap()
        .is_some());
    assert_eq!(snapshot_rows(&s, &run).len(), 2);
}

/// `SnapshotCadence::parse` — the one canonical spelling table (CC1):
/// `every_n_effects` must carry `:<n≥1>`; every other spelling is `None`.
#[test]
fn cadence_parse_is_the_closed_spelling_table() {
    assert_eq!(
        SnapshotCadence::parse("never"),
        Some(SnapshotCadence::Never)
    );
    assert_eq!(
        SnapshotCadence::parse("on_turn_end"),
        Some(SnapshotCadence::OnTurnEnd)
    );
    assert_eq!(
        SnapshotCadence::parse("on_idle"),
        Some(SnapshotCadence::OnIdle)
    );
    assert_eq!(
        SnapshotCadence::parse("every_n_effects:3"),
        Some(SnapshotCadence::EveryNEffects(3))
    );
    for bad in [
        "every_n_effects",
        "every_n_effects:0",
        "every_n_effects:x",
        "on_turn_start",
        "",
    ] {
        assert_eq!(SnapshotCadence::parse(bad), None, "{bad}");
    }
}

// ── upload / download / diff / restore substrate ────────────────────────────

/// `env_upload` lands bytes in the writable root *and* the blob pool;
/// `env_download` re-addresses the live path — a content-addressed round
/// trip both directions.
#[test]
fn upload_download_round_trip() {
    let (mut s, run, lease) = open("ud");
    let mut d = EnvDriver::new(&run);
    let (id, ws) = provision_local(&mut s, &lease, &mut d, "ud-ws");
    let new_path = format!("{}/new.txt", ws.display());
    let (ca, size) = d
        .env_upload(&mut s, &lease, &id, &new_path, Some(b"hello r2.4"), None)
        .unwrap();
    assert_eq!(size, 10);
    assert_eq!(std::fs::read(&new_path).unwrap(), b"hello r2.4");
    let (ca2, size2) = d.env_download(&mut s, &lease, &id, &new_path).unwrap();
    assert_eq!(ca.id(), ca2.id());
    assert_eq!(size, size2);
    // Upload-by-address — the blob pool's copy writes the same bytes.
    let copy_path = format!("{}/copy.txt", ws.display());
    let (ca3, _) = d
        .env_upload(&mut s, &lease, &id, &copy_path, None, Some(&ca.id()))
        .unwrap();
    assert_eq!(ca3.id(), ca.id());
    assert_eq!(std::fs::read(&copy_path).unwrap(), b"hello r2.4");
    // The audit rows landed durable.
    let evs = s.events(&run).unwrap();
    assert!(evs.iter().any(|e| e.class == "action.environment.uploaded"));
    assert!(evs
        .iter()
        .any(|e| e.class == "action.environment.downloaded"));
    // Honest refusals — outside the writable roots, an absent path, an
    // absent address.
    assert!(d
        .env_upload(&mut s, &lease, &id, "/etc/hh-no.txt", Some(b"x"), None)
        .is_err());
    let absent = format!("{}/absent.txt", ws.display());
    assert!(d.env_download(&mut s, &lease, &id, &absent).is_err());
    let x_path = format!("{}/x.txt", ws.display());
    assert!(d
        .env_upload(&mut s, &lease, &id, &x_path, None, Some("sha256:deadbeef"))
        .is_err());
}

/// `fs_tree_diff` sees the live change-set against a named baseline;
/// `restore_successor` materialises the baseline's content in a fresh
/// environment — the successor's tree is the snapshot's, not the live
/// one.
#[test]
fn diff_and_successor_restore_round_trip() {
    let (mut s, run, lease) = open("dr");
    let mut d = EnvDriver::new(&run);
    let (id, ws) = provision_local(&mut s, &lease, &mut d, "dr-ws");
    // Baseline covers `seed.txt` + `present.txt`.
    let present = format!("{}/present.txt", ws.display());
    let (ca_old, _) = d
        .env_upload(&mut s, &lease, &id, &present, Some(b"kept"), None)
        .unwrap();
    let (baseline, _tree) = d
        .fs_tree_snapshot_as(&mut s, &lease, &id, TakenBy::Subject)
        .unwrap();
    // The live change: a file the baseline never saw.
    let added = format!("{}/added.txt", ws.display());
    d.env_upload(&mut s, &lease, &id, &added, Some(b"new"), None)
        .unwrap();
    let entries = d.fs_tree_diff(&s, &id, &baseline.snapshot_ref).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.relpath.ends_with("added.txt") && e.change == "added"),
        "diff sees the addition: {entries:?}"
    );
    // Successor restore — the new env's tree is the baseline's content.
    let succ = d
        .restore_successor(&mut s, &lease, &id, &baseline.snapshot_ref)
        .unwrap();
    assert_ne!(succ.env_handle_id, id);
    assert_eq!(succ.state, HandleState::Ready);
    // The successor's diff against the same baseline is empty — a real
    // restore, not a label.
    let entries = d
        .fs_tree_diff(&s, &succ.env_handle_id, &baseline.snapshot_ref)
        .unwrap();
    assert!(
        entries.is_empty(),
        "successor restores the baseline: {entries:?}"
    );
    // `present.txt` round-trips content-addressed in the successor's own
    // root; `added.txt` is gone.
    let succ_root = d.handle(&succ.env_handle_id).unwrap().roots.workspace_roots[0].clone();
    let succ_present = format!("{succ_root}/present.txt");
    let (ca, _) = d
        .env_download(&mut s, &lease, &succ.env_handle_id, &succ_present)
        .unwrap();
    assert_eq!(ca.id(), ca_old.id());
    let succ_added = format!("{succ_root}/added.txt");
    assert!(d
        .env_download(&mut s, &lease, &succ.env_handle_id, &succ_added)
        .is_err());
    // The detached parent still reads through `fs_read` — verify only
    // gates the environment verbs' session, `fs_read` checks ready too —
    // so the old download path is now a typed refusal, not stale bytes.
    assert!(d.env_download(&mut s, &lease, &id, &present).is_err());
    // The parent detached (retained for inspection) — durable, never
    // silent; `restored{mode:"successor"}` names the successor.
    let evs = s.events(&run).unwrap();
    assert!(evs.iter().any(|e| e.class == "action.environment.restored"));
    assert!(evs.iter().any(|e| {
        e.class == "action.environment.detached"
            && e.payload.get("reason").and_then(Json::as_str) == Some("replaced_by_successor")
    }));
    // A bogus baseline names the typed refusal, never an empty diff.
    assert!(d.fs_tree_diff(&s, &id, "sha256:bogus").is_err());
    let _ = ws;
}
