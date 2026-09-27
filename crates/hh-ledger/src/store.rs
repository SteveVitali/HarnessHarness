//! The store — `Store` owns every run's WAL, the per-run writer lease, the
//! content-addressed blob pool and the subscriber fan-out.
//!
//! Storage class (§5a.1 §5):
//!
//! - **WAL** — `<root>/runs/<run_id>/events.wal`, canonical-JSONL records:
//!   `{"k":"e","v":<envelope>}` then `{"k":"c","n":<last_seq>}` (the commit marker).
//!   `append` writes the batch's lines, syncs, writes the marker, syncs again, and only
//!   then makes the events visible — **durable-before-visible** (ADR-0026 §5); a
//!   crash mid-batch leaves uncommitted lines the replay drops wholesale (a torn tail
//!   is never surfaced as durable).
//! - **Lease** — `<root>/runs/<run_id>/lease.json`, written via create-new /
//!   tmp+rename: the exclusive-writer primitive with stale-holder detection across
//!   processes on one host. `lease_generation` strictly increases on takeover and is
//!   the fencing token on every event.
//! - **Blobs** — `<root>/blobs/<digest>` — content-addressed, deduplicated by
//!   construction (same bytes ⇒ same address ⇒ same file).
//!
//! Everything else (`events` vec, `by_event_id`, `ir_index`, `open_scopes`, `head`) is
//! a **rebuildable index** — delete it, replay the WAL, and the same structure returns
//! (AC-3). Nothing here is itself authoritative except the WAL and the blob pool.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::{SystemTime, UNIX_EPOCH};

use hh_identity::idp::ContentAddress;
use hh_provenance::{ContentKind, Origin, ProvenanceRecord};
use hh_wire::json::{self, Json};

use crate::audit::{
    AuditFault, AuditKeyResolver, AuditSigner, Auditor, BlobStatus, CheckpointKind, CHECKPOINT_ALG,
};
use crate::classes::{self, Durability, ScopeKind};
use crate::effect::{self, EffectCtx, EffectFold, EffectPhase};
use crate::errors::{LedgerError, MissingReason, Tampered, TamperedKind};
use crate::event::{
    Cursor, Direction, EphemeralRecord, Event, EventEnvelope, EventFrame, EventPlane, Head, Page,
    Producer, ReadFilter, Scope, SeqRange,
};
use crate::ids::{
    effect_id as derive_effect_id, is_pinned_id, valid_ts, Clock, IdSource, SeqIds, SystemClock,
    TimeIds, GENESIS_HASH, ROOT_EVENT,
};
use crate::manifest::{EventRef, LineageLink, ObservabilityLevel, ParticipantClass, RunManifest};
use crate::schema::SCHEMA_VERSION;
use crate::views::{self, View, ViewKind};

/// The subscriber channel depth before `lagged` fires.
const SUB_BUFFER: usize = 1024;

/// The default blob-size ceiling — snapshot-scale (the retention/GC policy refines it
/// at Stage 2; ADR-0234).
pub const DEFAULT_BLOB_MAX_BYTES: usize = 1 << 30;

/// The ledger's own component ref for system rows (`lifecycle.run.created`,
/// `lifecycle.lease.*`).
const KERNEL_LEDGER: &str = "kernel:ledger";

/// The kernel component ref for effect-lifecycle rows the ledger itself writes
/// (`commit_effect`, recovery `unknown`/`abandoned` — §5a.2; ADR-0030 §6).
pub(crate) const KERNEL_EFFECT: &str = "kernel:effect";

// ─────────────────────────────────────────────────────────────────────────────
// Lease
// ─────────────────────────────────────────────────────────────────────────────

/// `Lease{lease_id, lease_generation, expires_at}` — the writer token (`acquire_writer`
/// / `renew` / `release`; §5a.1 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The run.
    pub run_id: String,
    /// The lease id.
    pub lease_id: String,
    /// The holder identity.
    pub holder: String,
    /// The fencing token — strictly increases on takeover.
    pub generation: u64,
    /// Expiry (wall ms — the stale-holder detection mechanism, never a ledger fact).
    pub expires_at_ms: u64,
}

/// A `redact` target (R-2.8.6 §5g.6): either a blob address directly, or a
/// declared `content_refs` member of one event. `audit_fields` members answer
/// `NotRedactable` — structural audit data is never erasable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactTarget {
    /// A blob address (`idp/1` id).
    Address(String),
    /// `<event_id>.<field>` — the member must be a declared `content_refs`
    /// field of the event's class.
    Field {
        /// The event carrying the member.
        event_id: String,
        /// The payload member.
        field: String,
    },
}

/// The persisted lease record (`lease.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct LeaseRecord {
    lease_id: String,
    holder: String,
    generation: u64,
    acquired_at_ms: u64,
    expires_at_ms: u64,
    status: LeaseStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaseStatus {
    Active,
    Released,
    Fenced,
}

impl LeaseStatus {
    fn as_str(self) -> &'static str {
        match self {
            LeaseStatus::Active => "active",
            LeaseStatus::Released => "released",
            LeaseStatus::Fenced => "fenced",
        }
    }
}

fn lease_record_json(r: &LeaseRecord) -> Json {
    Json::Obj(BTreeMap::from([
        ("lease_id".to_string(), Json::str(&r.lease_id)),
        ("holder".to_string(), Json::str(&r.holder)),
        ("generation".to_string(), Json::Int(r.generation as i64)),
        (
            "acquired_at_ms".to_string(),
            Json::Int(r.acquired_at_ms as i64),
        ),
        (
            "expires_at_ms".to_string(),
            Json::Int(r.expires_at_ms as i64),
        ),
        ("status".to_string(), Json::str(r.status.as_str())),
    ]))
}

fn lease_record_from_json(j: &Json) -> Option<LeaseRecord> {
    let status = match j.get("status").and_then(Json::as_str)? {
        "active" => LeaseStatus::Active,
        "released" => LeaseStatus::Released,
        "fenced" => LeaseStatus::Fenced,
        _ => return None,
    };
    Some(LeaseRecord {
        lease_id: j.get("lease_id").and_then(Json::as_str)?.to_string(),
        holder: j.get("holder").and_then(Json::as_str)?.to_string(),
        generation: j.get("generation").and_then(Json::as_int)? as u64,
        acquired_at_ms: j.get("acquired_at_ms").and_then(Json::as_int)? as u64,
        expires_at_ms: j.get("expires_at_ms").and_then(Json::as_int)? as u64,
        status,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-run state
// ─────────────────────────────────────────────────────────────────────────────

struct Subscriber {
    tx: SyncSender<EventFrame>,
    lagged_from: Option<u64>,
}

/// Everything in `RunState` except `dir`/`subscribers`/`lease` is a rebuildable
/// projection of the WAL — `rebuild` recomputes it from disk.
pub(crate) struct RunState {
    dir: PathBuf,
    run_id: String,
    manifest: RunManifest,
    pub(crate) events: Vec<EventEnvelope>,
    by_event_id: HashMap<String, u64>,
    ir_index: HashMap<String, Vec<u64>>,
    pub(crate) open_scopes: BTreeMap<String, ScopeKind>,
    /// The `action.effect.*` fold — `effect_id → EffectFold` (§5a.2; R-2.2.2). Fed
    /// on the commit path and rebuilt from the WAL, so `Store::effect_fold` and
    /// the I-1 write-ahead gate answer without a rescan.
    pub(crate) effects: BTreeMap<String, EffectFold>,
    /// The `security.permission.decided` gate fold — `(effect_id, attempt_no) →
    /// final verdict` (ADR-0052 D6 complete mediation; §5g.1 I-H7). Same
    /// commit/rebuild discipline as `effects`.
    pub(crate) decisions: effect::DecisionFolds,
    pub(crate) head: Option<Head>,
    pub(crate) finished: bool,
    /// `lifecycle.run.suspended` … `lifecycle.run.resumed`/`finished` (ADR-0131
    /// §3 — a suspended run is exempt from liveness-based takeover).
    pub(crate) suspended: bool,
    /// The scoped-lease fold — `lifecycle.lease.*` rows whose `scope` ≠
    /// `writer` (`effect:`/`resource:`/`environment:`/`wakeup:` — §5a.3;
    /// ADR-0131 §1; S2.3).
    pub(crate) scoped_leases: BTreeMap<String, crate::leases::ScopedLease>,
    /// The wakeup fold — `control.wakeup.*` rows → `subscription_id →
    /// WakeupSubscription` (§5a.3; ADR-0131 §4; S2.3).
    pub(crate) wakeups: BTreeMap<String, crate::wakeup::WakeupSubscription>,
    /// The HLC node id — `Some` only on continuation/child runs (the
    /// `R-2.2.3⁰ᵇ` slice stamps `hlc` on their events; plain runs carry none —
    /// additive, CC8).
    pub(crate) hlc_node: Option<String>,
    /// The last stamped HLC (the tick base; rebuilt from the events' `hlc`).
    pub(crate) hlc_last: Option<crate::hlc::Hlc>,
    /// The compact-range audit-tree frontier over the committed leaf hashes —
    /// pushed on commit and rebuilt on replay like every other fold (CC1;
    /// derived, never load-bearing — `tree::mth` over `events` recomputes it).
    pub(crate) tree: crate::tree::CompactRange,
    subscribers: Vec<Subscriber>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

/// The event store. One `Store` per root; `&mut self` on every mutating op is the
/// in-process single-writer guard; the persisted lease is the cross-process one.
pub struct Store {
    root: PathBuf,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    blob_max_bytes: usize,
    runs: BTreeMap<String, RunState>,
    /// Blob tombstones — `<alg>:<digest>` → the reason bytes are gone, folded
    /// from every run's `lifecycle.ledger.redacted`/`lifecycle.ledger.gc` rows
    /// (rebuilt on open, updated on op). `get_blob`'s missing-file path and
    /// `audit_view`'s accounting both consult it — a deleted blob is a
    /// recorded fact, never an unexplained hole (ADR-0068 R3).
    tombstones: BTreeMap<String, MissingReason>,
    /// The audit-signature key resolver — the seam custody hangs off
    /// (R-2.8.3 `kernel_use` answers a `FixedSigner`; a broker-backed
    /// resolver can replace it without touching the ledger). `None` ⇒
    /// signatures verify shape-only (`unverified`), never fabricated ok.
    audit_keys: Option<Box<dyn AuditKeyResolver>>,
}

/// A live tail — `Stream<EventFrame>`. The replayed `durable` frames + `sync` ride in
/// `replay` (unbounded — it is a page, same as `read` returns); live frames then
/// arrive over the bounded channel (`lagged` marks overflow).
pub struct Subscription {
    replay: std::collections::VecDeque<EventFrame>,
    rx: Receiver<EventFrame>,
}

impl Subscription {
    /// Non-blocking `next` — `None` when nothing is buffered (live tail is quiet).
    pub fn try_next(&mut self) -> Option<EventFrame> {
        self.replay.pop_front().or_else(|| self.rx.try_recv().ok())
    }
}

impl Iterator for Subscription {
    type Item = EventFrame;

    fn next(&mut self) -> Option<EventFrame> {
        self.replay.pop_front().or_else(|| self.rx.recv().ok())
    }
}

/// One `lineage` entry — `{run_id, up_to_seq, head_hash}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageEntry {
    /// The run.
    pub run_id: String,
    /// The seq the entry binds at (the link's `at_seq`, or the run's head for the
    /// terminal entry).
    pub up_to_seq: u64,
    /// The head hash at `up_to_seq`.
    pub head_hash: String,
}

impl Store {
    /// Open (or create) a store at `root` with the system clock, time-ordered ids and
    /// the default blob ceiling. Replays every run's WAL.
    pub fn open(root: impl AsRef<Path>) -> Result<Store, LedgerError> {
        Self::open_with(root, Box::new(SystemClock), None, DEFAULT_BLOB_MAX_BYTES)
    }

    /// Open with an injected clock / id source / blob ceiling — the deterministic seam
    /// (golden corpus, tests).
    pub fn open_with(
        root: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Option<Box<dyn IdSource>>,
        blob_max_bytes: usize,
    ) -> Result<Store, LedgerError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("runs"))
            .and_then(|_| fs::create_dir_all(root.join("blobs")))
            .map_err(|e| LedgerError::Io {
                detail: format!("create store dirs: {e}"),
            })?;
        // The store tag — a per-store uniqueness component for allocated ids
        // (created once, persisted; not a ledger fact).
        let tag_path = root.join("store.tag");
        let tag = match fs::read_to_string(&tag_path) {
            Ok(t) => t.trim().to_string(),
            Err(_) => {
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let tag = format!(
                    "{:08x}",
                    (nanos ^ (std::process::id() as u128)) & 0xffff_ffff
                );
                fs::write(&tag_path, &tag).map_err(|e| LedgerError::Io {
                    detail: format!("write store tag: {e}"),
                })?;
                tag
            }
        };
        let ids: Box<dyn IdSource> =
            ids.unwrap_or_else(|| Box::new(TimeIds::new(Box::new(SystemClock), tag)));
        let mut store = Store {
            root,
            ids,
            clock,
            blob_max_bytes,
            runs: BTreeMap::new(),
            tombstones: BTreeMap::new(),
            audit_keys: None,
        };
        store.load_all()?;
        Ok(store)
    }

    /// Install the audit-signature key resolver (R-2.8.6 Stage 2) — the seam
    /// `verify_run`/`audit_view` resolve checkpoint `key_id`s through. The
    /// embed layer wires a `kernel_use`-resolved signer here; tests use
    /// `FixedSigner`/`KeyTable` directly.
    pub fn set_audit_key_resolver(&mut self, resolver: Box<dyn AuditKeyResolver>) {
        self.audit_keys = Some(resolver);
    }

    /// The installed resolver, if any.
    pub fn audit_key_resolver(&self) -> Option<&dyn AuditKeyResolver> {
        self.audit_keys.as_deref()
    }

    /// The store root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Allocate a fresh id of `kind` through the store's `IdSource` — the seam
    /// ledger-bound consumers (the `hh-budget` account) use so event/budget/
    /// reservation ids come from the one injected source (golden-corpus
    /// determinism is preserved for `SeqIds`-backed stores).
    pub fn alloc_id(&self, kind: &str) -> String {
        self.ids.alloc(kind)
    }

    /// The store clock, wall-ms.
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// `rfc3339_ms(now_ms())` — the `ts` stamp for caller-built `Event`s.
    pub fn ts_now(&self) -> String {
        rfc3339_ms(self.clock.now_ms())
    }

    /// The C1-tier gate — an op the stage slices at `tier-c1` refuses typed
    /// `UnsupportedTier` on a `--no-default-features` build (removability(0)'s
    /// honest-refusal leg, CC6 — never a silent skip).
    #[allow(dead_code)]
    pub(crate) fn tier_c1(&self, op: &'static str) -> Result<(), LedgerError> {
        #[cfg(feature = "tier-c1")]
        {
            let _ = op;
            Ok(())
        }
        #[cfg(not(feature = "tier-c1"))]
        {
            Err(LedgerError::UnsupportedTier { tier: "c1", op })
        }
    }

    /// The run's head `event_id` — the `parent_event_id` for a new caller event
    /// extending the branch head ([`crate::ids::ROOT_EVENT`] when the run is
    /// still at genesis).
    pub fn head_event_id(&self, run_id: &str) -> Result<String, LedgerError> {
        let state = self.run(run_id)?;
        Ok(state
            .head
            .as_ref()
            .map(|h| h.event_id.clone())
            .unwrap_or_else(|| ROOT_EVENT.to_string()))
    }

    /// The persisted writer-lease generation — the fencing token every
    /// post-`prepared` `action.effect.*` row must carry (§5a.2 invariant 6) and
    /// the `CommitToken`'s epoch check (ADR-0100 I-1).
    pub fn current_lease_generation(&self, run_id: &str) -> Result<u64, LedgerError> {
        if !self.runs.contains_key(run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            });
        }
        let rec =
            read_lease_file(&self.lease_path(run_id))?.ok_or_else(|| LedgerError::Fenced {
                lease_generation: 0,
                current_generation: 0,
                detail: "no lease record".into(),
            })?;
        Ok(rec.generation)
    }

    fn load_all(&mut self) -> Result<(), LedgerError> {
        let runs_dir = self.root.join("runs");
        let entries = fs::read_dir(&runs_dir).map_err(|e| LedgerError::Io {
            detail: format!("read runs dir: {e}"),
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| LedgerError::Io {
                detail: format!("read run dir entry: {e}"),
            })?;
            if !entry.path().is_dir() {
                continue;
            }
            let run_id = entry.file_name().to_string_lossy().to_string();
            let state = self.rebuild(&run_id)?;
            self.runs.insert(run_id, state);
        }
        // Fold the tombstone map — `lifecycle.ledger.{redacted,gc}` rows are
        // durable facts, so their targets survive restart (ADR-0068 R3).
        for state in self.runs.values() {
            for env in &state.events {
                let (member, reason) = match env.class.as_str() {
                    "lifecycle.ledger.redacted" => ("targets", MissingReason::Redacted),
                    "lifecycle.ledger.gc" => ("addresses", MissingReason::Gc),
                    _ => continue,
                };
                if let Some(Json::Arr(items)) = env.payload.get(member) {
                    for a in items.iter().filter_map(Json::as_str) {
                        self.tombstones.insert(a.to_string(), reason);
                    }
                }
            }
        }
        Ok(())
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        self.root.join("runs").join(run_id)
    }

    fn wal_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("events.wal")
    }

    fn lease_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("lease.json")
    }

    /// Replay the WAL and rebuild every derived structure (the *only* way run state
    /// exists — the in-memory form is a projection, never the record).
    fn rebuild(&self, run_id: &str) -> Result<RunState, LedgerError> {
        let dir = self.run_dir(run_id);
        let wal = dir.join("events.wal");
        let committed = replay_committed(&wal).map_err(|e| LedgerError::Io {
            detail: format!("replay {}: {e}", wal.display()),
        })?;
        let mut state = RunState {
            dir,
            run_id: run_id.to_string(),
            manifest: RunManifest::minimal(crate::manifest::RunKind::Agent),
            events: Vec::new(),
            by_event_id: HashMap::new(),
            ir_index: HashMap::new(),
            open_scopes: BTreeMap::new(),
            effects: BTreeMap::new(),
            decisions: BTreeMap::new(),
            head: None,
            finished: false,
            suspended: false,
            scoped_leases: BTreeMap::new(),
            wakeups: BTreeMap::new(),
            hlc_node: None,
            hlc_last: None,
            tree: crate::tree::CompactRange::default(),
            subscribers: Vec::new(),
        };
        for env in committed {
            if env.seq == 0 && env.class == "lifecycle.run.created" {
                state.manifest = RunManifest::from_json(&env.payload)?;
                // A lineage-bearing run stamps `hlc` — the node id is the run
                // id itself (one node identity per run lifetime; the writer
                // identity rides `holder` on the lease rows).
                if manifest_has_lineage(&state.manifest) {
                    state.hlc_node = Some(run_id.to_string());
                }
            }
            state.by_event_id.insert(env.event_id.clone(), env.seq);
            for r in &env.ir_refs {
                state
                    .ir_index
                    .entry(r.version_id.clone())
                    .or_default()
                    .push(env.seq);
            }
            apply_scope_marks(&mut state.open_scopes, &state.effects, &env);
            effect::fold_event(&mut state.effects, &env);
            effect::apply_decision(&mut state.decisions, &state.effects, &env);
            crate::leases::fold_lease_row(&mut state.scoped_leases, &env);
            crate::wakeup::fold_wakeup_row(&mut state.wakeups, &env);
            if let Some(h) = env.hlc.as_deref().and_then(crate::hlc::Hlc::parse) {
                state.hlc_last = Some(h);
            }
            match env.class.as_str() {
                "lifecycle.run.finished" => state.finished = true,
                "lifecycle.run.suspended" => state.suspended = true,
                "lifecycle.run.resumed" => state.suspended = false,
                _ => {}
            }
            state.head = Some(Head {
                seq: env.seq,
                event_id: env.event_id.clone(),
                hash: env.hash.clone(),
            });
            state.tree.push(env.hash.clone());
            state.events.push(env);
        }
        Ok(state)
    }

    pub(crate) fn run(&self, run_id: &str) -> Result<&RunState, LedgerError> {
        self.runs
            .get(run_id)
            .ok_or_else(|| LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            })
    }

    // ── open_run ─────────────────────────────────────────────────────────

    /// `open_run(manifest, holder) → (RunId, Lease)` — allocates a time-ordered
    /// `RunId`, appends `lifecycle.run.created` at seq 0 carrying the manifest,
    /// acquires the writer lease (§5a.1 §2). The manifest is immutable thereafter.
    pub fn open_run(
        &mut self,
        manifest: RunManifest,
        holder: &str,
    ) -> Result<(String, Lease), LedgerError> {
        manifest.validate()?;
        // Lineage anchors: `forked_from`/`continued_from` must resolve and bind the
        // source head hash at `at_seq` (ADR-0027 §6; ADR-0131 §5).
        for link in [&manifest.forked_from, &manifest.continued_from]
            .into_iter()
            .flatten()
        {
            self.check_lineage_link(link)?;
        }
        if let Some(parent) = &manifest.parent_run_id {
            if !self.runs.contains_key(parent) {
                return Err(LedgerError::UnknownForkPoint {
                    run_id: parent.clone(),
                });
            }
        }
        if let Some(ev) = &manifest.spawn_event {
            self.resolve_event_ref(ev)?;
        }
        let run_id = self.ids.alloc("run");
        let dir = self.run_dir(&run_id);
        fs::create_dir_all(&dir).map_err(|e| LedgerError::Io {
            detail: format!("create run dir: {e}"),
        })?;
        let anchor = manifest
            .forked_from
            .as_ref()
            .or(manifest.continued_from.as_ref())
            .map(|l| l.head_hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let mut state = RunState {
            dir,
            run_id: run_id.clone(),
            manifest: manifest.clone(),
            events: Vec::new(),
            by_event_id: HashMap::new(),
            ir_index: HashMap::new(),
            open_scopes: BTreeMap::new(),
            effects: BTreeMap::new(),
            decisions: BTreeMap::new(),
            head: None,
            finished: false,
            suspended: false,
            scoped_leases: BTreeMap::new(),
            wakeups: BTreeMap::new(),
            // A continuation/child run stamps `hlc` on every event — seeded
            // causally above the lineage anchor's own stamp (ADR-0131 §5).
            hlc_node: manifest_has_lineage(&manifest).then(|| run_id.clone()),
            hlc_last: None,
            tree: crate::tree::CompactRange::default(),
            subscribers: Vec::new(),
        };
        // Seed the continuation clock from the lineage source's stamp — the
        // `continued_from`/`forked_from` anchor event's own `hlc`, else the
        // parent run's head stamp (ADR-0131 §5's causal order).
        if state.hlc_node.is_some() {
            let src_hlc = manifest
                .continued_from
                .as_ref()
                .or(manifest.forked_from.as_ref())
                .and_then(|l| {
                    self.runs
                        .get(&l.run_id)
                        .and_then(|s| s.events.get(l.at_seq as usize))
                })
                .or_else(|| {
                    manifest
                        .parent_run_id
                        .as_ref()
                        .and_then(|p| self.runs.get(p))
                        .and_then(|s| s.events.last())
                })
                .and_then(|e| e.hlc.as_deref())
                .and_then(crate::hlc::Hlc::parse);
            state.hlc_last = Some(crate::hlc::Hlc::seed(
                self.clock.now_ms(),
                src_hlc.as_ref(),
                state.hlc_node.as_deref().unwrap_or("run"),
            ));
        }
        // seq 0 — `lifecycle.run.created{manifest}` under the anchor.
        let created = system_event(
            &*self.ids,
            &*self.clock,
            &state,
            0,
            "lifecycle.run.created",
            manifest.to_json(),
            &anchor,
            1, // generation 1 — the lease record lands right after.
        );
        commit_envelopes(
            &mut state,
            vec![Staged::Durable(created)],
            self.clock.now_ms(),
        )?;
        // The writer lease (generation 1) + the audited `lease.acquired` row.
        let rec = LeaseRecord {
            lease_id: self.ids.alloc("lease"),
            holder: holder.to_string(),
            generation: 1,
            acquired_at_ms: self.clock.now_ms(),
            expires_at_ms: self.clock.now_ms() + manifest.lease_ttl.writer_ms,
            status: LeaseStatus::Active,
        };
        write_lease_file(&state.dir, &rec)?;
        let lease = Lease {
            run_id: run_id.clone(),
            lease_id: rec.lease_id.clone(),
            holder: rec.holder.clone(),
            generation: rec.generation,
            expires_at_ms: rec.expires_at_ms,
        };
        emit_lease_row(
            &*self.ids,
            &*self.clock,
            &mut state,
            "lifecycle.lease.acquired",
            &rec,
            Json::Null,
        )?;
        self.runs.insert(run_id.clone(), state);
        Ok((run_id, lease))
    }

    fn check_lineage_link(&self, link: &LineageLink) -> Result<(), LedgerError> {
        let src = self
            .runs
            .get(&link.run_id)
            .ok_or_else(|| LedgerError::UnknownForkPoint {
                run_id: link.run_id.clone(),
            })?;
        let head = src.head.as_ref().map(|h| h.seq).unwrap_or(0);
        if link.at_seq > head {
            return Err(LedgerError::SourceIncomplete {
                run_id: link.run_id.clone(),
                at_seq: link.at_seq,
                head,
            });
        }
        let at = &src.events[link.at_seq as usize];
        if at.hash != link.head_hash {
            return Err(LedgerError::ForkPointNotCoherent {
                run_id: link.run_id.clone(),
                at_seq: link.at_seq,
                open_scopes: Vec::new(),
            });
        }
        Ok(())
    }

    fn resolve_event_ref(&self, r: &EventRef) -> Result<(), LedgerError> {
        let src = self
            .runs
            .get(&r.run_id)
            .ok_or_else(|| LedgerError::UnresolvedEventRef {
                run_id: r.run_id.clone(),
                event_id: r.event_id.clone(),
            })?;
        if !src.by_event_id.contains_key(&r.event_id) {
            return Err(LedgerError::UnresolvedEventRef {
                run_id: r.run_id.clone(),
                event_id: r.event_id.clone(),
            });
        }
        Ok(())
    }

    // ── writer lease ─────────────────────────────────────────────────────

    /// `acquire_writer(run, holder, ttl_ms) → Lease` — one valid writer per run;
    /// takeover of an expired/released lease bumps the generation and is audited
    /// (`lifecycle.lease.fenced` then `lifecycle.lease.acquired`).
    pub fn acquire_writer(
        &mut self,
        holder: &str,
        run_id: &str,
        ttl_ms: u64,
    ) -> Result<Lease, LedgerError> {
        if !self.runs.contains_key(run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            });
        }
        let now = self.clock.now_ms();
        let path = self.lease_path(run_id);
        if let Some(rec) = read_lease_file(&path)? {
            if rec.status == LeaseStatus::Active && now < rec.expires_at_ms {
                return Err(LedgerError::WouldBlock {
                    active_holder: rec.holder,
                });
            }
            let generation = rec.generation + 1;
            let new_rec = LeaseRecord {
                lease_id: self.ids.alloc("lease"),
                holder: holder.to_string(),
                generation,
                acquired_at_ms: now,
                expires_at_ms: now + ttl_ms,
                status: LeaseStatus::Active,
            };
            write_lease_file(&self.run_dir(run_id), &new_rec)?;
            // Confirm we won the race — the record on disk is ours.
            if read_lease_file(&path)? != Some(new_rec.clone()) {
                return Err(LedgerError::Fenced {
                    lease_generation: generation,
                    current_generation: generation,
                    detail: "lost the takeover race".into(),
                });
            }
            let state = self.runs.get_mut(run_id).unwrap();
            if rec.status == LeaseStatus::Active {
                // The stale holder is fenced by the takeover (audited).
                emit_lease_row(
                    &*self.ids,
                    &*self.clock,
                    state,
                    "lifecycle.lease.fenced",
                    &new_rec,
                    Json::Obj(BTreeMap::from([
                        ("scope".to_string(), Json::str("writer")),
                        ("stale_lease_id".to_string(), Json::str(&rec.lease_id)),
                        (
                            "stale_generation".to_string(),
                            Json::Int(rec.generation as i64),
                        ),
                        ("reason".to_string(), Json::str("takeover")),
                    ])),
                )?;
            }
            emit_lease_row(
                &*self.ids,
                &*self.clock,
                state,
                "lifecycle.lease.acquired",
                &new_rec,
                Json::Null,
            )?;
            return Ok(Lease {
                run_id: run_id.to_string(),
                lease_id: new_rec.lease_id,
                holder: new_rec.holder,
                generation: new_rec.generation,
                expires_at_ms: new_rec.expires_at_ms,
            });
        }
        // No lease record at all (shouldn't happen after open_run — treat as a fresh
        // acquisition at generation 1).
        let rec = LeaseRecord {
            lease_id: self.ids.alloc("lease"),
            holder: holder.to_string(),
            generation: 1,
            acquired_at_ms: now,
            expires_at_ms: now + ttl_ms,
            status: LeaseStatus::Active,
        };
        write_lease_file(&self.run_dir(run_id), &rec)?;
        let state = self.runs.get_mut(run_id).unwrap();
        emit_lease_row(
            &*self.ids,
            &*self.clock,
            state,
            "lifecycle.lease.acquired",
            &rec,
            Json::Null,
        )?;
        Ok(Lease {
            run_id: run_id.to_string(),
            lease_id: rec.lease_id,
            holder: rec.holder,
            generation: rec.generation,
            expires_at_ms: rec.expires_at_ms,
        })
    }

    /// `renew(lease)` — extends the live lease; a stale or expired token is `Fenced`.
    pub fn renew(&mut self, lease: &Lease) -> Result<Lease, LedgerError> {
        if !self.runs.contains_key(&lease.run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: lease.run_id.clone(),
            });
        }
        let now = self.clock.now_ms();
        let path = self.lease_path(&lease.run_id);
        let rec = read_lease_file(&path)?.ok_or_else(|| LedgerError::Fenced {
            lease_generation: lease.generation,
            current_generation: 0,
            detail: "no lease record".into(),
        })?;
        if rec.lease_id != lease.lease_id || rec.generation != lease.generation {
            return Err(LedgerError::Fenced {
                lease_generation: lease.generation,
                current_generation: rec.generation,
                detail: "superseded by a takeover".into(),
            });
        }
        if now >= rec.expires_at_ms {
            return Err(LedgerError::Fenced {
                lease_generation: lease.generation,
                current_generation: rec.generation,
                detail: "lease expired; re-acquire".into(),
            });
        }
        let new_rec = LeaseRecord {
            expires_at_ms: now + (rec.expires_at_ms - rec.acquired_at_ms).max(1),
            ..rec
        };
        write_lease_file(&self.run_dir(&lease.run_id), &new_rec)?;
        let state = self.runs.get_mut(&lease.run_id).unwrap();
        emit_lease_row(
            &*self.ids,
            &*self.clock,
            state,
            "lifecycle.lease.renewed",
            &new_rec,
            Json::Null,
        )?;
        Ok(Lease {
            run_id: lease.run_id.clone(),
            lease_id: new_rec.lease_id,
            holder: new_rec.holder,
            generation: new_rec.generation,
            expires_at_ms: new_rec.expires_at_ms,
        })
    }

    /// `release(lease, reason)` — marks the lease released and audits it.
    pub fn release(&mut self, lease: &Lease, reason: &str) -> Result<(), LedgerError> {
        let run_id = lease.run_id.clone();
        let new_rec = self.mark_released(lease)?;
        let state = self.runs.get_mut(&run_id).unwrap();
        emit_lease_row(
            &*self.ids,
            &*self.clock,
            state,
            "lifecycle.lease.released",
            &new_rec,
            Json::Obj(BTreeMap::from([("reason".to_string(), Json::str(reason))])),
        )?;
        Ok(())
    }

    /// `release_silent(lease)` — the same lease-record transition as
    /// `release` but without minting `lifecycle.lease.released`: used
    /// when the run's event stream is already sealed at
    /// `lifecycle.run.finished`, where a post-terminal row would break
    /// the stream ≡ export byte-identity (`run events --from 0` must
    /// equal what the live stream carried — AC-R-2.11.1-3). The lease
    /// file still records `Released`.
    pub fn release_silent(&mut self, lease: &Lease) -> Result<(), LedgerError> {
        self.mark_released(lease)?;
        Ok(())
    }

    /// Shared fencing check + lease-record update for `release` /
    /// `release_silent`. Returns the new record so callers may audit it.
    fn mark_released(&mut self, lease: &Lease) -> Result<LeaseRecord, LedgerError> {
        if !self.runs.contains_key(&lease.run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: lease.run_id.clone(),
            });
        }
        let path = self.lease_path(&lease.run_id);
        let rec = read_lease_file(&path)?.ok_or_else(|| LedgerError::Fenced {
            lease_generation: lease.generation,
            current_generation: 0,
            detail: "no lease record".into(),
        })?;
        if rec.lease_id != lease.lease_id || rec.generation != lease.generation {
            return Err(LedgerError::Fenced {
                lease_generation: lease.generation,
                current_generation: rec.generation,
                detail: "superseded by a takeover".into(),
            });
        }
        let new_rec = LeaseRecord {
            status: LeaseStatus::Released,
            ..rec
        };
        write_lease_file(&self.run_dir(&lease.run_id), &new_rec)?;
        Ok(new_rec)
    }

    /// The shared lease fence — re-reads the persisted record so a
    /// cross-process takeover fences this holder even though our in-memory
    /// copy is stale, emits `lifecycle.lease.fenced` on a stale token, and
    /// returns the active record (its `generation` stamps the op's rows).
    /// `append` and the kernel audit ops (`checkpoint`/`gc`/`redact`) share
    /// it — one fence, never two spellings (CC1).
    fn active_lease(&mut self, run_id: &str, lease: &Lease) -> Result<LeaseRecord, LedgerError> {
        let rec =
            read_lease_file(&self.lease_path(run_id))?.ok_or_else(|| LedgerError::Fenced {
                lease_generation: lease.generation,
                current_generation: 0,
                detail: "no lease record".into(),
            })?;
        let stale = rec.status != LeaseStatus::Active
            || rec.lease_id != lease.lease_id
            || rec.generation != lease.generation
            || self.clock.now_ms() >= rec.expires_at_ms;
        if stale {
            let state = self.runs.get_mut(run_id).unwrap();
            let reason = if rec.lease_id != lease.lease_id || rec.generation != lease.generation {
                "superseded by a takeover"
            } else if rec.status != LeaseStatus::Active {
                "lease not active"
            } else {
                "lease expired"
            };
            emit_lease_row(
                &*self.ids,
                &*self.clock,
                state,
                "lifecycle.lease.fenced",
                &rec,
                Json::Obj(BTreeMap::from([
                    ("stale_lease_id".to_string(), Json::str(&lease.lease_id)),
                    (
                        "stale_generation".to_string(),
                        Json::Int(lease.generation as i64),
                    ),
                    ("reason".to_string(), Json::str(reason)),
                ])),
            )?;
            return Err(LedgerError::Fenced {
                lease_generation: lease.generation,
                current_generation: rec.generation,
                detail: reason.into(),
            });
        }
        Ok(rec)
    }

    // ── append ───────────────────────────────────────────────────────────

    /// `append(run, lease, events) → SeqRange` — an atomic multi-event unit:
    /// everything validates before anything is written; the batch's WAL lines are
    /// synced, the commit marker is synced, and only then do the events become
    /// visible (durable-before-visible; no partial batch).
    pub fn append(
        &mut self,
        run_id: &str,
        lease: &Lease,
        events: Vec<Event>,
    ) -> Result<SeqRange, LedgerError> {
        if !self.runs.contains_key(run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            });
        }
        // Cross-run references resolve against committed events only (immutable
        // borrow, before the run state is taken mutably).
        for ev in &events {
            for r in &ev.causes {
                self.resolve_event_ref(r)?;
            }
        }
        // The lease fence — re-read the persisted record so a cross-process takeover
        // fences this holder even though our in-memory copy is stale.
        let rec = self.active_lease(run_id, lease)?;
        let state = self.runs.get_mut(run_id).unwrap();
        if state.finished {
            return Err(LedgerError::RunFinished {
                run_id: run_id.to_string(),
            });
        }
        // ── validate the whole batch (batch-local state) ────────────────
        let mut known_ids: HashSet<String> = state.by_event_id.keys().cloned().collect();
        let mut open: BTreeMap<String, ScopeKind> = state.open_scopes.clone();
        // The §5a.2 effect fold, batch-local: each `action.effect.*` validates
        // against committed ∪ earlier-in-batch state (one machine — CC1).
        let mut effect_folds = state.effects.clone();
        // The complete-mediation gate fold, batch-local with the same committed
        // ∪ earlier-in-batch semantics (one machine — CC1).
        let mut decision_folds = state.decisions.clone();
        let mut staged: Vec<Staged> = Vec::new();
        let mut next_seq = state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0);
        let mut prev_hash = state
            .head
            .as_ref()
            .map(|h| h.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        for ev in events.into_iter() {
            let spec = classes::lookup(&ev.class).ok_or_else(|| LedgerError::SchemaViolation {
                detail: format!("unknown class {}", ev.class),
            })?;
            if !valid_ts(&ev.ts) {
                return Err(LedgerError::SchemaViolation {
                    detail: format!("ts {} is not RFC 3339 UTC ms", ev.ts),
                });
            }
            if ev.event_id.is_empty()
                || ev.event_id == ROOT_EVENT
                || ev.event_id.contains('.')
                || ev.event_id.contains('/')
            {
                return Err(LedgerError::SchemaViolation {
                    detail: format!("event_id {} is not an allocated id", ev.event_id),
                });
            }
            if known_ids.contains(&ev.event_id) {
                return Err(LedgerError::DuplicateEventId {
                    event_id: ev.event_id,
                });
            }
            // Parent: the root sentinel, or a committed/earlier-in-batch event —
            // checked *before* this event's own id enters the set (a self-parent is
            // an UnknownParent, never a cycle).
            if ev.parent_event_id != ROOT_EVENT && !known_ids.contains(&ev.parent_event_id) {
                return Err(LedgerError::UnknownParent {
                    parent_event_id: ev.parent_event_id,
                });
            }
            known_ids.insert(ev.event_id.clone());
            // Scope chain + open/close rules (fold-aware effect-scope closes).
            check_scopes(&ev, spec, &mut open, &effect_folds)?;
            // §5a.2 effect lifecycle (R-2.2.2): phase transitions, raise-only
            // risk, derived idempotency keys and the post-`prepared` fencing
            // token — checked against the committed fold ∪ this batch.
            {
                let committed_class_of = |eid: &str| -> Option<String> {
                    state
                        .by_event_id
                        .get(eid)
                        .and_then(|s| state.events.get(*s as usize))
                        .map(|e| e.class.clone())
                };
                let batch_class_of = |eid: &str| -> Option<String> {
                    staged.iter().find_map(|s| match s {
                        Staged::Durable(e) if e.event_id == eid => Some(e.class.clone()),
                        _ => None,
                    })
                };
                let ectx = EffectCtx {
                    run_id,
                    generation: rec.generation,
                    committed_class_of: &committed_class_of,
                    batch_class_of: &batch_class_of,
                };
                effect::validate_event(&ev, &mut effect_folds, &decision_folds, &ectx)?;
            }
            // ADR-0052 D6 / §5g.1 I-H7: the permission-decision gate — at most one
            // final `decided` per `(effect_id, attempt)`; `committed` events read
            // this fold for their allow pre-record. Runs after the effect fold so
            // the decided row resolves its attempt against the newest state.
            effect::validate_decision(&ev, &mut decision_folds, &effect_folds)?;
            // The I-1 write-ahead gate (AC-R-2.2.2-10; ADR-0100): a dispatch
            // naming a non-`read_only` effect may land only while the durable
            // `committed` record is the effect's live phase.
            if ev.class == "action.tool.started" {
                if let Some(eid) = ev.scope.effect_id.as_deref() {
                    match effect_folds.get(eid) {
                        Some(f)
                            if f.risk_class.is_read_only() || f.phase == EffectPhase::Committed => {
                        }
                        Some(_) => {
                            return Err(LedgerError::NotCommitted {
                                effect_id: eid.to_string(),
                            })
                        }
                        None => {
                            return Err(LedgerError::UnknownEffect {
                                effect_id: eid.to_string(),
                            })
                        }
                    }
                }
            }
            // Rule P (§5g.6 §2; ADR-0066 D4): an audit-grade row is accepted only
            // if `producer.component_class ∈ class.producers` — kernel-only. The
            // kernel-origin rule (§5a.1 §5) is the same component check for the
            // lifecycle/environment/registry rows.
            let producer_ok = if spec.audit_grade {
                spec.producers
                    .iter()
                    .any(|p| *p == ev.producer.component_class)
            } else if spec.kernel_origin {
                ev.producer.component_class == crate::event::KERNEL_COMPONENT
            } else {
                true
            };
            if !producer_ok {
                return Err(LedgerError::AuditProducerInvalid {
                    class: ev.class.clone(),
                    producer: ev.producer.component_class.clone(),
                    detail: "component_class ∉ class.producers".into(),
                });
            }
            // Rule C (§5g.6 I-A1): the audit_fields/content_refs partition —
            // checked here, before the provenance fold, so a malformed audit row
            // is refused as a schema error. A `Text` leaf has no place in
            // `audit_fields` — an audit-grade event declaring `free_text`
            // content is refused outright.
            if spec.audit_grade {
                if ev.content_kind == Some(ContentKind::FreeText) {
                    return Err(LedgerError::SchemaViolation {
                        detail: format!(
                            "audit-grade {} carries a Text leaf (content_kind = \
                             free_text) — audit_fields are ids/hashes/enums/ints/refs only",
                            ev.class
                        ),
                    });
                }
                check_audit_partition(&ev.class, &ev.payload, spec)?;
            }
            // Provenance rules (ADR-0033 §7; mandatory table; kernel origin; R-TEXT;
            // scope ceilings).
            let prov = if spec.requires_provenance {
                Some(
                    ev.provenance
                        .as_ref()
                        .ok_or_else(|| LedgerError::MissingProvenance {
                            what: ev.class.clone(),
                        })?,
                )
            } else {
                ev.provenance.as_ref()
            };
            if let Some(p) = prov {
                p.validate(None)?;
                if spec.kernel_origin && !matches!(p.origin, Origin::Kernel { .. }) {
                    return Err(LedgerError::KernelOriginRequired {
                        class: ev.class.clone(),
                    });
                }
                // Rule P, second half (§5g.6 §2): an audit-grade row carries
                // `provenance.authority = kernel` — anything less claims a kernel
                // fact it is not.
                if spec.audit_grade && p.authority != hh_provenance::AuthorityClass::Kernel {
                    return Err(LedgerError::AuditProducerInvalid {
                        class: ev.class.clone(),
                        producer: ev.producer.component_class.clone(),
                        detail: format!("provenance.authority = {} ≠ kernel", p.authority.as_str()),
                    });
                }
                if ev.content_kind == Some(ContentKind::FreeText)
                    && p.authority > hh_provenance::AuthorityClass::External
                {
                    return Err(LedgerError::TextAboveExternal {
                        authority: p.authority,
                    });
                }
                check_scope_ceiling(p)?;
            }
            // Monitor check 5 — endorsement legitimacy at append (ADR-0035 §1).
            if ev.class == "security.label.endorsed" {
                let endorsed =
                    hh_provenance::LabelEndorsed::from_json(&ev.payload).map_err(|e| {
                        LedgerError::SchemaViolation {
                            detail: format!("label.endorsed payload: {}", e.detail),
                        }
                    })?;
                let subject_seq = state.by_event_id.get(&endorsed.subject_ref).copied();
                let subject = subject_seq
                    .and_then(|s| state.events.get(s as usize))
                    .and_then(|e| e.provenance.as_ref())
                    .ok_or_else(|| LedgerError::UnresolvedEventRef {
                        run_id: run_id.to_string(),
                        event_id: endorsed.subject_ref.clone(),
                    })?;
                let kind = ev.content_kind.unwrap_or(ContentKind::Other);
                hh_provenance::check_endorsement(&endorsed, subject, kind)?;
            }
            if spec.durability == Durability::Ephemeral {
                staged.push(Staged::Eph(stamp_ephemeral(state, &ev, rec.generation)));
                continue;
            }
            // Durable: strip declared ephemeral members (delivered as a fragment frame).
            let mut payload = ev.payload.clone();
            let mut fragment = BTreeMap::new();
            for &f in spec.ephemeral_fields {
                if let Some(v) = payload.get(f) {
                    fragment.insert(f.to_string(), v.clone());
                }
            }
            if let Json::Obj(ref mut m) = payload {
                for &f in spec.ephemeral_fields {
                    m.remove(f);
                }
            }
            let canonical_len = payload.to_canonical_string().len();
            if canonical_len >= spec.offload_threshold {
                return Err(LedgerError::SchemaViolation {
                    detail: format!(
                        "payload of {} is {canonical_len} canonical bytes ≥ the class \
                         offload threshold {}; offload through put_blob and reference",
                        ev.class, spec.offload_threshold
                    ),
                });
            }
            let seq = next_seq;
            next_seq += 1;
            let mut env = EventEnvelope {
                event_id: ev.event_id.clone(),
                run_id: run_id.to_string(),
                seq,
                ts: ev.ts.clone(),
                hlc: ev.hlc.clone(),
                plane: EventPlane::of_class(&ev.class).ok_or_else(|| {
                    LedgerError::SchemaViolation {
                        detail: format!("class {} has no family prefix", ev.class),
                    }
                })?,
                class: ev.class.clone(),
                schema_version: SCHEMA_VERSION,
                producer: ev.producer.clone(),
                participant_class: state.manifest.participant_class,
                observability_level: {
                    let mut o = state.manifest.observability_level.clone();
                    if state.manifest.participant_class == ParticipantClass::Native {
                        o.insert(ObservabilityLevel::Ledger);
                    }
                    o
                },
                durability: Durability::Ledger,
                scope: ev.scope.clone(),
                lease_generation: rec.generation,
                parent_event_id: ev.parent_event_id.clone(),
                causes: ev.causes.clone(),
                refs: ev.refs.clone(),
                ir_refs: ev.ir_refs.clone(),
                surface_ids: ev.surface_ids.clone(),
                provenance: ev.provenance.clone(),
                prev_hash: prev_hash.clone(),
                payload,
                hash: String::new(),
            };
            env.hash = env.recompute_hash();
            prev_hash = env.hash.clone();
            let mut eph_after: Option<EphemeralRecord> = None;
            if !fragment.is_empty() {
                let mut frag = stamp_ephemeral(
                    state,
                    &Event {
                        event_id: env.event_id.clone(),
                        class: ev.class.clone(),
                        ts: ev.ts.clone(),
                        hlc: ev.hlc.clone(),
                        producer: ev.producer.clone(),
                        scope: ev.scope.clone(),
                        parent_event_id: env.parent_event_id.clone(),
                        causes: ev.causes.clone(),
                        refs: ev.refs.clone(),
                        ir_refs: ev.ir_refs.clone(),
                        surface_ids: ev.surface_ids.clone(),
                        provenance: ev.provenance.clone(),
                        content_kind: ev.content_kind,
                        payload: Json::Obj(fragment),
                    },
                    rec.generation,
                );
                frag.durable_event_seq = Some(seq);
                eph_after = Some(frag);
            }
            staged.push(Staged::Durable(env));
            if let Some(frag) = eph_after {
                staged.push(Staged::Eph(frag));
            }
        }
        // ── commit (durable-before-visible) ─────────────────────────────
        let now_ms = self.clock.now_ms();
        commit_envelopes(state, staged, now_ms)
    }

    // ── read / head / subscribe ──────────────────────────────────────────

    /// `read(run, cursor, filter, direction, limit) → Page` — durable events only;
    /// cursors are inclusive.
    pub fn read(
        &self,
        run_id: &str,
        cursor: Cursor,
        filter: Option<&ReadFilter>,
        direction: Direction,
        limit: usize,
    ) -> Result<Page, LedgerError> {
        let state = self.run(run_id)?;
        let start_seq = match &cursor {
            Cursor::Seq(n) => *n,
            Cursor::EventId(id) => {
                *state
                    .by_event_id
                    .get(id)
                    .ok_or_else(|| LedgerError::UnknownCursor {
                        detail: format!("event_id {id} unknown"),
                    })?
            }
            Cursor::Now => {
                return Err(LedgerError::UnknownCursor {
                    detail: "read requires a durable cursor (Now is subscribe-only)".into(),
                })
            }
        };
        let head_seq = state.head.as_ref().map(|h| h.seq).unwrap_or(0);
        if start_seq > head_seq && !state.events.is_empty() {
            return Err(LedgerError::UnknownCursor {
                detail: format!("from_seq {start_seq} beyond head {head_seq}"),
            });
        }
        let matches = |e: &&EventEnvelope| -> bool {
            let Some(f) = filter else { return true };
            if let Some(p) = f.plane {
                if e.plane != p {
                    return false;
                }
            }
            if let Some(c) = &f.class {
                if let Some(prefix) = c.strip_suffix(".*") {
                    let dotted = format!("{prefix}.");
                    if !e.class.starts_with(&dotted) {
                        return false;
                    }
                } else if &e.class != c {
                    return false;
                }
            }
            if let Some(s) = &f.scope {
                for (a, b) in [
                    (&s.turn_id, &e.scope.turn_id),
                    (&s.model_call_id, &e.scope.model_call_id),
                    (&s.tool_call_id, &e.scope.tool_call_id),
                    (&s.effect_id, &e.scope.effect_id),
                    (&s.child_run_id, &e.scope.child_run_id),
                    (&s.branch_id, &e.scope.branch_id),
                ] {
                    if let Some(want) = a {
                        if b.as_ref() != Some(want) {
                            return false;
                        }
                    }
                }
            }
            if let Some(refs) = &f.ir_refs {
                for want in refs {
                    let has = e.ir_refs.iter().any(|r| {
                        r.version_id == want.version_id
                            && (want.semantic_id.is_none() || r.semantic_id == want.semantic_id)
                    });
                    if !has {
                        return false;
                    }
                }
            }
            true
        };
        let mut out: Vec<EventEnvelope> = Vec::new();
        match direction {
            Direction::Fwd => {
                for e in state.events.iter().filter(|e| e.seq >= start_seq) {
                    if matches(&e) {
                        out.push(e.clone());
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
            Direction::Rev => {
                for e in state.events.iter().rev().filter(|e| e.seq <= start_seq) {
                    if matches(&e) {
                        out.push(e.clone());
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        let next_cursor = out.last().and_then(|e| match direction {
            Direction::Fwd => {
                if e.seq < head_seq {
                    Some(Cursor::Seq(e.seq + 1))
                } else {
                    None
                }
            }
            Direction::Rev => {
                if e.seq > 0 {
                    Some(Cursor::Seq(e.seq - 1))
                } else {
                    None
                }
            }
        });
        Ok(Page {
            events: out,
            next_cursor,
        })
    }

    /// `head(run) → {seq, event_id, hash}`.
    pub fn head(&self, run_id: &str) -> Result<Head, LedgerError> {
        let state = self.run(run_id)?;
        state.head.clone().ok_or_else(|| LedgerError::UnknownRun {
            run_id: run_id.to_string(),
        })
    }

    /// `subscribe(run, from: Cursor) → Stream<EventFrame>` — replays durable events
    /// from the inclusive cursor, emits `sync`, then live frames. Ephemeral classes
    /// appear here and never in `read`.
    pub fn subscribe(&mut self, run_id: &str, from: Cursor) -> Result<Subscription, LedgerError> {
        let state = self.run(run_id)?;
        let (tx, rx) = sync_channel(SUB_BUFFER);
        let start_seq = match &from {
            Cursor::Seq(n) => *n,
            Cursor::EventId(id) => {
                *state
                    .by_event_id
                    .get(id)
                    .ok_or_else(|| LedgerError::UnknownCursor {
                        detail: format!("event_id {id} unknown"),
                    })?
            }
            Cursor::Now => state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0),
        };
        let head_seq = state.head.as_ref().map(|h| h.seq).unwrap_or(0);
        if start_seq > head_seq + 1 {
            return Err(LedgerError::UnknownCursor {
                detail: format!("from {start_seq} beyond head {head_seq}"),
            });
        }
        let mut last = start_seq.saturating_sub(1);
        let mut replay = std::collections::VecDeque::new();
        for e in state.events.iter().filter(|e| e.seq >= start_seq) {
            replay.push_back(EventFrame::Durable {
                seq: e.seq,
                hash: e.hash.clone(),
                event: Box::new(e.clone()),
            });
            last = e.seq;
        }
        replay.push_back(EventFrame::Sync { at_seq: last });
        let state = self.runs.get_mut(run_id).unwrap();
        state.subscribers.push(Subscriber {
            tx,
            lagged_from: None,
        });
        Ok(Subscription { replay, rx })
    }

    // ── blobs ────────────────────────────────────────────────────────────

    /// `put_blob(bytes) → ContentAddress` — content-addressed under `idp/1`'s blob
    /// domain; identical bytes deduplicate to the same address (and file).
    pub fn put_blob(
        &mut self,
        bytes: &[u8],
        media_type: &str,
    ) -> Result<ContentAddress, LedgerError> {
        if bytes.len() > self.blob_max_bytes {
            return Err(LedgerError::TooLarge {
                bytes: bytes.len(),
                max: self.blob_max_bytes,
            });
        }
        let addr = hh_identity::idp::address(bytes, media_type);
        let path = self.root.join("blobs").join(&addr.digest);
        if !path.exists() {
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, bytes).map_err(io_err)?;
            fs::rename(&tmp, &path).map_err(io_err)?;
            sync_dir(&self.root.join("blobs")).map_err(io_err)?;
        }
        Ok(addr)
    }

    /// `get_blob(address) → bytes | Missing{reason}` — never `Tampered` (ADR-0068 R3).
    /// Bytes present but not hashing back to the address are `BlobCorrupt` (CC3 —
    /// corruption is surfaced, never silently served).
    pub fn get_blob(&self, address: &ContentAddress) -> Result<Vec<u8>, LedgerError> {
        let id = format!("{}:{}", address.algorithm, address.digest);
        hh_identity::idp::parse_id(&id).map_err(|_| LedgerError::SchemaViolation {
            detail: format!("blob address {id} is not an idp/1 id"),
        })?;
        let path = self.root.join("blobs").join(&address.digest);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                // The tombstone fold names the recorded reason when one
                // exists; an unaccounted absence reports `gc` (the default
                // "bytes retired" reason) — never a fabricated presence.
                let reason = self
                    .tombstones
                    .get(&id)
                    .copied()
                    .unwrap_or(MissingReason::Gc);
                return Err(LedgerError::Missing {
                    address: id,
                    reason,
                });
            }
        };
        if hh_identity::idp::idp_digest("blob", &bytes) != address.digest {
            return Err(LedgerError::BlobCorrupt { address: id });
        }
        Ok(bytes)
    }

    // ── verify / project / lineage ───────────────────────────────────────

    /// `verify(run) → ok | Tampered{at_seq, kind}` — the unbounded verify:
    /// the byte-exact chain check plus the checkpoint/anchor recomputation
    /// pass, with signature values checked through the installed resolver
    /// when one is held (`set_audit_key_resolver`). See [`Store::verify_run`]
    /// for the bounded/keyed form.
    pub fn verify(&self, run_id: &str) -> Result<(), LedgerError> {
        self.verify_run(run_id, None, None, self.audit_keys.as_deref())
    }

    /// `verify(run, from?, to?, keys?) → ok | Tampered{at_seq, kind}` —
    /// re-reads the WAL from disk (the durable truth), replays the committed
    /// prefix and checks it **byte-exact**: every stored line must re-render
    /// to its canonical bytes (`NonCanonicalBytes`), every hash recomputes
    /// (`ContentModified`), `prev_hash` chains (`ChainBroken`), seqs are dense
    /// and ordered (`SeqGap`/`Reordered`/`DuplicateSeq`/`DuplicateEventId`),
    /// parents resolve (`DanglingParent`), and audit-grade rows carry a
    /// producer from the class's declared set with `authority = kernel`
    /// provenance (`Producer`).
    ///
    /// The Stage-2 half (R-2.8.6; ADR-0067 §5(b)) then folds the checkpoint
    /// layer: `from`/`to` are caller watermarks — a committed prefix shorter
    /// than either bound is `Truncate{at_seq: head+1}` (absent-with-later-
    /// head). Every `security.audit.checkpoint` row is re-read as a claim and
    /// recomputed: `tree_size == seq`, `tree_head` is the covered range's own
    /// Merkle head (`ForkEquivocation` on disagreement — a signed head the
    /// log cannot substantiate), `chain_hash` the covered tip, `idp`
    /// re-derives over the unsigned claim, `prev_checkpoint` links the prior
    /// claim (`CheckpointInvalid` on a break), signatures carry a declared
    /// `key_id` + the C0 `hmac-sha256` construction (`SigMissing` when empty,
    /// `BadSignature` on a malformed/unregistered/mismatching entry — the
    /// HMAC value itself is checked when `keys` resolves it), and every
    /// `cross_run_anchors` member is recomputed against the named run's
    /// durable prefix (`ForkEquivocation` on a contradiction; a run this
    /// store does not hold is `unresolvable` — surfaced by `audit_view`, not
    /// asserted here). A `final` checkpoint must close the stream; a
    /// `finished` run that declared `signer_key_ids` with no final checkpoint
    /// is `Truncate` — the terminal signed head is absent-with-later-head.
    pub fn verify_run(
        &self,
        run_id: &str,
        from: Option<u64>,
        to: Option<u64>,
        keys: Option<&dyn AuditKeyResolver>,
    ) -> Result<(), LedgerError> {
        if !self.runs.contains_key(run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            });
        }
        let wal = self.wal_path(run_id);
        let replay = replay_committed_raw(&wal).map_err(|e| LedgerError::Io {
            detail: format!("replay WAL for verify: {e}"),
        })?;
        let tampered =
            |at_seq: u64, kind: TamperedKind| LedgerError::Tampered(Tampered { at_seq, kind });
        let committed = &replay.committed;
        let mut ids: HashSet<&str> = HashSet::new();
        let mut seqs: HashSet<u64> = HashSet::new();
        let mut prev_hash = committed
            .first()
            .map(|(_, e)| e.prev_hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        for (i, (line, env)) in committed.iter().enumerate() {
            let expect_seq = i as u64;
            if env.seq != expect_seq {
                return Err(tampered(
                    expect_seq,
                    if seqs.contains(&env.seq) {
                        TamperedKind::DuplicateSeq
                    } else if env.seq > expect_seq {
                        TamperedKind::SeqGap
                    } else {
                        TamperedKind::Reordered
                    },
                ));
            }
            seqs.insert(env.seq);
            if !ids.insert(env.event_id.as_str()) {
                return Err(tampered(env.seq, TamperedKind::DuplicateEventId));
            }
            // Byte-exact before the semantic checks: the stored line must be the
            // canonical re-rendering of the decoded envelope.
            let mut expected = b"{\"k\":\"e\",\"v\":".to_vec();
            expected.extend_from_slice(&env.canonical_bytes());
            expected.push(b'}');
            if *line != expected {
                return Err(tampered(env.seq, TamperedKind::NonCanonicalBytes));
            }
            if env.recompute_hash() != env.hash {
                return Err(tampered(env.seq, TamperedKind::ContentModified));
            }
            if env.prev_hash != prev_hash {
                return Err(tampered(env.seq, TamperedKind::ChainBroken));
            }
            if env.parent_event_id != ROOT_EVENT && !ids.contains(env.parent_event_id.as_str()) {
                return Err(tampered(env.seq, TamperedKind::DanglingParent));
            }
            // §5g.6 `producer`: an audit-grade row in the committed prefix must
            // satisfy Rule P (producer set membership + kernel authority).
            if let Some(spec) = classes::lookup(&env.class) {
                if spec.audit_grade {
                    let producer_ok = spec
                        .producers
                        .iter()
                        .any(|p| *p == env.producer.component_class);
                    let authority_ok = env
                        .provenance
                        .as_ref()
                        .map(|p| p.authority == hh_provenance::AuthorityClass::Kernel)
                        .unwrap_or(false);
                    if !producer_ok || !authority_ok {
                        return Err(tampered(env.seq, TamperedKind::Producer));
                    }
                }
            }
            prev_hash = env.hash.clone();
        }
        if let Some(at) = replay.noncanonical_at {
            return Err(tampered(at as u64, TamperedKind::NonCanonicalBytes));
        }

        // ── caller watermarks — absent-with-later-head truncation ──────
        let covered = committed.len() as u64;
        for bound in [to, from].into_iter().flatten() {
            if bound >= covered {
                return Err(tampered(covered, TamperedKind::Truncate));
            }
        }

        // ── the checkpoint layer ────────────────────────────────────────
        let manifest = &self.runs[run_id].manifest;
        let leaf_hashes: Vec<String> = committed.iter().map(|(_, e)| e.hash.clone()).collect();
        let mut prev_claim: Option<(&EventEnvelope, crate::tree::CheckpointClaim)> = None;
        let mut final_seq: Option<u64> = None;
        let mut finished_seq: Option<u64> = None;
        for (_, env) in committed.iter() {
            if env.class == "lifecycle.run.finished" {
                finished_seq = Some(env.seq);
            }
            if env.class != "security.audit.checkpoint" {
                continue;
            }
            let claim = crate::tree::parse_checkpoint(&env.payload)
                .ok_or_else(|| tampered(env.seq, TamperedKind::CheckpointInvalid))?;
            // `tree_size == seq`: a checkpoint covers the durable prefix that
            // ends just before itself.
            if claim.tree_size != Some(env.seq) {
                return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
            }
            // The signed head must be the covered range's own MTH — a head
            // the log cannot substantiate is a split view.
            if claim.tree_head.as_deref()
                != Some(crate::tree::mth_prefix(&leaf_hashes, env.seq as usize).as_str())
            {
                return Err(tampered(env.seq, TamperedKind::ForkEquivocation));
            }
            // `chain_hash` — the event-hash chain tip at coverage.
            let expect_chain = if env.seq == 0 {
                return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
            } else {
                leaf_hashes[(env.seq - 1) as usize].as_str()
            };
            if claim.chain_hash.as_deref() != Some(expect_chain) {
                return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
            }
            // `idp` re-derives over the unsigned claim bytes.
            if claim.idp.as_deref() != Some(crate::tree::checkpoint_idp(&env.payload).as_str()) {
                return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
            }
            // `prev_checkpoint` links the prior claim — the first carries
            // none (or `null`); every later one names the held claim's
            // `{event_id, tree_size, tree_head}`.
            match &prev_claim {
                None => {
                    if let Some(p) = &claim.prev_checkpoint {
                        if *p != Json::Null {
                            return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
                        }
                    }
                }
                Some((pev, pc)) => {
                    let ok = match claim.prev_checkpoint.as_ref() {
                        Some(Json::Obj(link)) => {
                            link.get("event_id").and_then(Json::as_str) == Some(&pev.event_id)
                                && link
                                    .get("tree_size")
                                    .and_then(Json::as_int)
                                    .map(|v| v as u64)
                                    == pc.tree_size
                                && link.get("tree_head").and_then(Json::as_str)
                                    == pc.tree_head.as_deref()
                        }
                        _ => false,
                    };
                    if !ok {
                        return Err(tampered(env.seq, TamperedKind::CheckpointInvalid));
                    }
                }
            }
            // Signatures — a checkpoint with none is a stripped claim; each
            // entry must name a declared key and the C0 construction, and
            // (with a resolver) the HMAC must match the unsigned claim.
            if claim.signatures.is_empty() {
                return Err(tampered(env.seq, TamperedKind::SigMissing));
            }
            let preimage = crate::tree::checkpoint_sig_preimage(&env.payload);
            for s in &claim.signatures {
                let bad_sig = || tampered(env.seq, TamperedKind::BadSignature);
                let kid = s.get("key_id").and_then(Json::as_str).ok_or_else(bad_sig)?;
                if s.get("alg_ref").and_then(Json::as_str) != Some(CHECKPOINT_ALG) {
                    return Err(bad_sig());
                }
                if !manifest.signer_key_ids.iter().any(|k| k == kid) {
                    return Err(bad_sig());
                }
                let sig = crate::audit::parse_sig(
                    s.get("sig").and_then(Json::as_str).ok_or_else(bad_sig)?,
                )
                .ok_or_else(bad_sig)?;
                if let Some(resolver) = keys {
                    let key = resolver.verify_key(kid).ok_or_else(|| {
                        LedgerError::SignerUnavailable {
                            run_id: run_id.to_string(),
                            detail: format!(
                                "signer_key_ids member {kid} does not resolve through                                  the supplied key resolver"
                            ),
                        }
                    })?;
                    if hh_wire::sha256::hmac_sha256(&key, &preimage).to_vec() != sig {
                        return Err(bad_sig());
                    }
                }
            }
            // Cross-run anchors — each claim recomputes against the named
            // run's durable prefix. A contradiction is `fork_equivocation`;
            // a run this store does not hold is unresolvable — surfaced by
            // `audit_view`, never asserted absent here.
            for a in &claim.cross_run_anchors {
                let other_run = a
                    .get("other_run")
                    .and_then(Json::as_str)
                    .ok_or_else(|| tampered(env.seq, TamperedKind::CheckpointInvalid))?;
                let (size, head) = a
                    .get("other_head")
                    .and_then(|oh| {
                        Some((
                            oh.get("tree_size")?.as_int()? as u64,
                            oh.get("tree_head")?.as_str()?,
                        ))
                    })
                    .ok_or_else(|| tampered(env.seq, TamperedKind::CheckpointInvalid))?;
                if let Some(other) = self.runs.get(other_run) {
                    let other_leaves: Vec<String> =
                        other.events.iter().map(|e| e.hash.clone()).collect();
                    if other_leaves.len() < size as usize
                        || crate::tree::mth_prefix(&other_leaves, size as usize) != head
                    {
                        return Err(tampered(env.seq, TamperedKind::ForkEquivocation));
                    }
                }
            }
            if claim.kind == CheckpointKind::Final.as_str() {
                final_seq = Some(env.seq);
            }
            prev_claim = Some((env, claim));
        }
        // A `final` checkpoint closes the signed stream — rows after it are
        // checkpoint-invalid by construction.
        if let Some(fs) = final_seq {
            if fs != covered - 1 {
                return Err(tampered(fs, TamperedKind::CheckpointInvalid));
            }
        }
        // A finished run that declared signers owes the terminal signed head.
        if let Some(fin) = finished_seq {
            if !manifest.signer_key_ids.is_empty() {
                match final_seq {
                    None => return Err(tampered(fin, TamperedKind::Truncate)),
                    Some(fs) if fs <= fin => {
                        return Err(tampered(fs, TamperedKind::CheckpointInvalid))
                    }
                    _ => {}
                }
            }
        }
        // Manifest lineage anchors re-verified — the same check `open_run`
        // ran, replayed as an audit fact (a moved/rewritten source prefix
        // shows here).
        for link in [&manifest.forked_from, &manifest.continued_from]
            .into_iter()
            .flatten()
        {
            if let Some(src) = self.runs.get(&link.run_id) {
                let ok = src
                    .events
                    .get(link.at_seq as usize)
                    .map(|e| e.hash == link.head_hash)
                    .unwrap_or(false);
                if !ok {
                    return Err(tampered(0, TamperedKind::ForkEquivocation));
                }
            }
        }
        Ok(())
    }

    // ── audit checkpoints / proofs / GC (R-2.8.6 Stage 2) ────────────────

    /// `checkpoint(run, lease, kind, signer)` — the §5g.6 signed audit
    /// checkpoint. Builds the claim over the current compact-range head
    /// (`tree_size` = the next seq — the checkpoint covers the durable prefix
    /// that ends just before itself), links `prev_checkpoint`, lists the
    /// cross-run anchors observed in range, signs the canonical unsigned
    /// claim through `signer` (custody resolved outside — R-2.8.3
    /// `kernel_use`), and appends the `security.audit.checkpoint` row.
    ///
    /// Kernel audit write: rides the writer fence but only `kind = final` may
    /// land after `lifecycle.run.finished` (the terminal checkpoint covers
    /// it by design); `final` is idempotent — a second call returns the
    /// landed row. Refusals: `SignerUnavailable` when the run declares no
    /// `signer_key_ids` or the signer's `key_id` is not among them;
    /// `InconsistentHead` when the new head is not a consistency-verified
    /// extension of the held previous claim — the kernel never signs a head
    /// it cannot substantiate.
    pub fn checkpoint(
        &mut self,
        run_id: &str,
        lease: &Lease,
        kind: CheckpointKind,
        signer: &mut dyn AuditSigner,
    ) -> Result<EventEnvelope, LedgerError> {
        let rec = self.active_lease(run_id, lease)?;
        let state = self.run(run_id)?;
        let manifest = &state.manifest;
        if manifest.signer_key_ids.is_empty() {
            return Err(LedgerError::SignerUnavailable {
                run_id: run_id.to_string(),
                detail: "the run manifest declares no signer_key_ids".into(),
            });
        }
        if !manifest.signer_key_ids.iter().any(|k| k == signer.key_id()) {
            return Err(LedgerError::SignerUnavailable {
                run_id: run_id.to_string(),
                detail: format!(
                    "signer key_id {} is not a manifest signer_key_ids member",
                    signer.key_id()
                ),
            });
        }
        if state.finished && kind != CheckpointKind::Final {
            return Err(LedgerError::RunFinished {
                run_id: run_id.to_string(),
            });
        }
        if kind == CheckpointKind::Final && !state.finished {
            return Err(LedgerError::SchemaViolation {
                detail: "kind=final requires lifecycle.run.finished committed".into(),
            });
        }
        let prev_checkpoint_ev = state
            .events
            .iter()
            .rev()
            .find(|e| e.class == "security.audit.checkpoint");
        if let Some(existing) = prev_checkpoint_ev {
            if kind == CheckpointKind::Final
                && existing.payload.get("kind").and_then(Json::as_str) == Some("final")
            {
                return Ok(existing.clone());
            }
        }
        let leaf_hashes: Vec<String> = state.events.iter().map(|e| e.hash.clone()).collect();
        let tree_size = leaf_hashes.len() as u64;
        let tree_head = crate::tree::mth(&leaf_hashes);
        let chain_hash = state
            .head
            .as_ref()
            .map(|h| h.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        // The consistency-verified-extension gate: recompute the previous
        // claim's head over the current leaves, then verify the new head as
        // an append-only extension of it. A divergence means the log under
        // the signed head changed — refuse to sign.
        let (since_seq, prev_link) = match prev_checkpoint_ev {
            Some(prev_ev) => {
                let prev_claim =
                    crate::tree::parse_checkpoint(&prev_ev.payload).ok_or_else(|| {
                        LedgerError::InconsistentHead {
                            run_id: run_id.to_string(),
                            detail: "the previous checkpoint row does not parse".into(),
                        }
                    })?;
                let prev_size =
                    prev_claim
                        .tree_size
                        .ok_or_else(|| LedgerError::InconsistentHead {
                            run_id: run_id.to_string(),
                            detail: "the previous checkpoint lacks tree_size".into(),
                        })? as usize;
                let prev_head_now = crate::tree::mth_prefix(&leaf_hashes, prev_size);
                if Some(prev_head_now.as_str()) != prev_claim.tree_head.as_deref() {
                    return Err(LedgerError::InconsistentHead {
                        run_id: run_id.to_string(),
                        detail: format!(
                            "the prefix under the last signed head changed \
                             (signed {}, recomputed {prev_head_now})",
                            prev_claim.tree_head.as_deref().unwrap_or("?")
                        ),
                    });
                }
                let proof =
                    crate::tree::prove_consistency(&leaf_hashes, prev_size, leaf_hashes.len())
                        .ok_or_else(|| LedgerError::InconsistentHead {
                            run_id: run_id.to_string(),
                            detail: "no consistency proof exists from the held head".into(),
                        })?;
                if !crate::tree::verify_consistency(
                    &proof,
                    prev_claim.tree_head.as_deref().unwrap_or_default(),
                    &tree_head,
                ) {
                    return Err(LedgerError::InconsistentHead {
                        run_id: run_id.to_string(),
                        detail: "the new head is not a consistent extension".into(),
                    });
                }
                let link = Json::obj([
                    ("event_id", Json::str(&prev_ev.event_id)),
                    (
                        "tree_size",
                        Json::Int(prev_claim.tree_size.unwrap_or(0) as i64),
                    ),
                    (
                        "tree_head",
                        prev_claim
                            .tree_head
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                ]);
                (prev_ev.seq + 1, link)
            }
            None => (0, Json::Null),
        };
        // Cross-run anchors observed in range — manifest lineage links (first
        // checkpoint only) plus `lifecycle.run.forked`/`control.subagent.
        // spawned` rows carrying `scope.child_run_id` since the last claim.
        let mut anchors: Vec<Json> = Vec::new();
        if since_seq == 0 {
            for (link, relation) in [
                (manifest.forked_from.as_ref(), "forked_from"),
                (manifest.continued_from.as_ref(), "continued_from"),
            ]
            .into_iter()
            {
                if let Some(link) = link {
                    if let Some(src) = self.runs.get(&link.run_id) {
                        let src_leaves: Vec<String> =
                            src.events.iter().map(|e| e.hash.clone()).collect();
                        let size = (link.at_seq + 1) as usize;
                        anchors.push(Json::obj([
                            ("other_run", Json::str(&link.run_id)),
                            (
                                "other_head",
                                Json::obj([
                                    ("tree_size", Json::Int(size as i64)),
                                    (
                                        "tree_head",
                                        Json::str(crate::tree::mth_prefix(&src_leaves, size)),
                                    ),
                                ]),
                            ),
                            ("relation", Json::str(relation)),
                        ]));
                    }
                }
            }
            if let Some(parent) = &manifest.parent_run_id {
                if let Some(src) = self.runs.get(parent) {
                    let src_leaves: Vec<String> =
                        src.events.iter().map(|e| e.hash.clone()).collect();
                    anchors.push(Json::obj([
                        ("other_run", Json::str(parent)),
                        (
                            "other_head",
                            Json::obj([
                                ("tree_size", Json::Int(src_leaves.len() as i64)),
                                ("tree_head", Json::str(crate::tree::mth(&src_leaves))),
                            ]),
                        ),
                        ("relation", Json::str("parent")),
                    ]));
                }
            }
        }
        for e in state.events.iter().filter(|e| e.seq >= since_seq) {
            let relation = match e.class.as_str() {
                "lifecycle.run.forked" => Some("fork"),
                "control.subagent.spawned" => Some("subagent"),
                _ => None,
            };
            let Some(relation) = relation else { continue };
            let Some(child) = e.scope.child_run_id.as_deref() else {
                continue;
            };
            if let Some(src) = self.runs.get(child) {
                let src_leaves: Vec<String> = src.events.iter().map(|e| e.hash.clone()).collect();
                anchors.push(Json::obj([
                    ("other_run", Json::str(child)),
                    (
                        "other_head",
                        Json::obj([
                            ("tree_size", Json::Int(src_leaves.len() as i64)),
                            ("tree_head", Json::str(crate::tree::mth(&src_leaves))),
                        ]),
                    ),
                    ("relation", Json::str(relation)),
                ]));
            }
        }
        // Build the unsigned claim, then the signature, then the idp.
        let mut members = BTreeMap::new();
        members.insert("kind".to_string(), Json::str(kind.as_str()));
        members.insert("origin".to_string(), Json::str(KERNEL_LEDGER));
        members.insert("tree_size".to_string(), Json::Int(tree_size as i64));
        members.insert("tree_head".to_string(), Json::str(&tree_head));
        members.insert("chain_hash".to_string(), Json::str(&chain_hash));
        members.insert("prev_checkpoint".to_string(), prev_link);
        members.insert("cross_run_anchors".to_string(), Json::Arr(anchors));
        if let Some(apr) = &manifest.audit_policy_ref {
            members.insert("audit_policy_ref".to_string(), Json::str(apr));
        }
        let unsigned = Json::Obj(members);
        let preimage = crate::tree::checkpoint_sig_preimage(&unsigned);
        let sig_bytes = signer
            .sign(&preimage)
            .map_err(|e| LedgerError::SignerUnavailable {
                run_id: run_id.to_string(),
                detail: format!("signer {}: {e}", signer.key_id()),
            })?;
        let mut members = match unsigned {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        members.insert(
            "signatures".to_string(),
            Json::Arr(vec![Json::obj([
                ("key_id", Json::str(signer.key_id())),
                ("alg_ref", Json::str(CHECKPOINT_ALG)),
                ("sig", Json::str(crate::audit::render_sig(&sig_bytes))),
            ])]),
        );
        let idp = crate::tree::checkpoint_idp(&Json::Obj(members.clone()));
        members.insert("idp".to_string(), Json::str(idp));
        let payload = Json::Obj(members);
        let state = self.runs.get_mut(run_id).unwrap();
        let seq = state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0);
        let prev_hash = state
            .head
            .as_ref()
            .map(|h| h.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let env = system_event(
            &*self.ids,
            &*self.clock,
            state,
            seq,
            "security.audit.checkpoint",
            payload,
            &prev_hash,
            rec.generation,
        );
        commit_envelopes(
            state,
            vec![Staged::Durable(env.clone())],
            self.clock.now_ms(),
        )?;
        Ok(env)
    }

    /// `prove_inclusion(run, seq, at?)` — the Merkle audit path for the event
    /// at `seq` under the tree of size `at` (default: current head).
    pub fn prove_inclusion(
        &self,
        run_id: &str,
        seq: u64,
        at: Option<u64>,
    ) -> Result<crate::tree::InclusionProof, LedgerError> {
        let state = self.run(run_id)?;
        let leaves: Vec<String> = state.events.iter().map(|e| e.hash.clone()).collect();
        let size = at.unwrap_or(leaves.len() as u64) as usize;
        crate::tree::prove_inclusion(&leaves, seq as usize, size).ok_or_else(|| {
            LedgerError::UnknownCursor {
                detail: format!(
                    "prove_inclusion: seq {seq} not in tree of size {size} \
                     (head {})",
                    leaves.len()
                ),
            }
        })
    }

    /// `prove_consistency(run, from_size, to_size)` — the append-only
    /// consistency proof between two tree sizes of this run.
    pub fn prove_consistency(
        &self,
        run_id: &str,
        from_size: u64,
        to_size: u64,
    ) -> Result<crate::tree::ConsistencyProof, LedgerError> {
        let state = self.run(run_id)?;
        let leaves: Vec<String> = state.events.iter().map(|e| e.hash.clone()).collect();
        crate::tree::prove_consistency(&leaves, from_size as usize, to_size as usize).ok_or_else(
            || LedgerError::UnknownCursor {
                detail: format!(
                    "prove_consistency: no proof from {from_size} to {to_size} \
                     (head {})",
                    leaves.len()
                ),
            },
        )
    }

    /// `audit_with(run, auditor, keys?)` — replay the durable prefix through
    /// an [`Auditor`]'s own record (the independent-head-holder check). The
    /// first `AuditFault` aborts; `Ok` means every frame — and every signed
    /// claim — agreed with the auditor's fold.
    pub fn audit_with(
        &self,
        run_id: &str,
        auditor: &mut Auditor,
        keys: Option<&dyn AuditKeyResolver>,
    ) -> Result<(), AuditFault> {
        let state = match self.runs.get(run_id) {
            Some(s) => s,
            None => {
                return Err(AuditFault::Malformed {
                    at_seq: 0,
                    detail: format!("unknown run {run_id}"),
                })
            }
        };
        for e in &state.events {
            auditor.observe(
                &EventFrame::Durable {
                    seq: e.seq,
                    hash: e.hash.clone(),
                    event: Box::new(e.clone()),
                },
                keys,
            )?;
        }
        Ok(())
    }

    /// The tombstone-aware blob lookup `audit_view` consumes — present /
    /// tombstoned-with-reason / missing-with-no-audit-row.
    fn blob_status(&self, address: &str) -> BlobStatus {
        let Ok(addr) = hh_identity::idp::parse_id(address) else {
            return BlobStatus::Missing;
        };
        if self.root.join("blobs").join(&addr.digest_hex).exists() {
            return BlobStatus::Present;
        }
        match self.tombstones.get(address) {
            Some(r) => BlobStatus::Tombstoned(*r),
            None => BlobStatus::Missing,
        }
    }

    /// Why an address is pinned against `gc`/`redact`, or `None` — the pin
    /// set (§5g.6 GC contract): another run's `refs`/`content_refs` still
    /// names it (a live fork prefix member), or it sits in an `audit_fields`
    /// member (structural audit data is never collectable). Own-run
    /// `content_refs` are the very thing `redact`/`gc` retire — not a pin.
    fn pin_reason(&self, address: &str, excluding_run: &str) -> Option<String> {
        for (rid, state) in &self.runs {
            if rid == excluding_run {
                continue;
            }
            for env in &state.events {
                if env
                    .refs
                    .iter()
                    .any(|r| format!("{}:{}", r.algorithm, r.digest) == address)
                {
                    return Some(format!("referenced_by_run:{rid}"));
                }
                if let Some(spec) = classes::lookup(&env.class) {
                    if let Json::Obj(m) = &env.payload {
                        for (name, v) in m {
                            let is_af = spec
                                .audit_fields
                                .iter()
                                .any(|f| f.name == "*" || f.name == name.as_str());
                            let is_cr = spec.content_refs.contains(&name.as_str());
                            if !is_af && !is_cr {
                                continue;
                            }
                            let hit = match v {
                                Json::Str(s) => s == address,
                                Json::Arr(items) => {
                                    items.iter().filter_map(Json::as_str).any(|s| s == address)
                                }
                                _ => false,
                            };
                            if hit {
                                return Some(if is_af {
                                    format!("audit_field:{}.{name}", env.class)
                                } else {
                                    format!("referenced_by_run:{rid}")
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// `gc(run, lease, addresses, policy_ref, tier, retained_until?)` — the
    /// retention op (§5g.6 GC contract). **Durable before delete**: the
    /// `lifecycle.ledger.gc` row commits (WAL synced) before any byte leaves
    /// the blob pool, so a crash between the two still leaves the audit fact.
    /// Refusals are typed: non-`idp/1` addresses → `SchemaViolation`; a
    /// pinned address → `Pinned{reason}` (the whole op is atomic). GC never
    /// targets events — only blob bytes.
    pub fn gc(
        &mut self,
        run_id: &str,
        lease: &Lease,
        addresses: Vec<String>,
        policy_ref: &str,
        tier: &str,
        retained_until: Option<u64>,
    ) -> Result<EventEnvelope, LedgerError> {
        let rec = self.active_lease(run_id, lease)?;
        for a in &addresses {
            if !is_pinned_id(a) {
                return Err(LedgerError::SchemaViolation {
                    detail: format!("gc address {a} is not an idp/1 id"),
                });
            }
            if let Some(reason) = self.pin_reason(a, run_id) {
                return Err(LedgerError::Pinned {
                    address: a.clone(),
                    reason,
                });
            }
        }
        // The durable record first.
        let state = self.runs.get_mut(run_id).unwrap();
        let seq = state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0);
        let prev_hash = state
            .head
            .as_ref()
            .map(|h| h.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let mut members = BTreeMap::new();
        members.insert(
            "addresses".to_string(),
            Json::Arr(addresses.iter().map(Json::str).collect()),
        );
        members.insert("policy_ref".to_string(), Json::str(policy_ref));
        members.insert("tier".to_string(), Json::str(tier));
        if let Some(t) = retained_until {
            members.insert("retained_until".to_string(), Json::Int(t as i64));
        }
        let env = system_event(
            &*self.ids,
            &*self.clock,
            state,
            seq,
            "lifecycle.ledger.gc",
            Json::Obj(members),
            &prev_hash,
            rec.generation,
        );
        commit_envelopes(
            state,
            vec![Staged::Durable(env.clone())],
            self.clock.now_ms(),
        )?;
        // Only now delete bytes — the audit fact is durable.
        for a in &addresses {
            if let Ok(addr) = hh_identity::idp::parse_id(a) {
                let path = self.root.join("blobs").join(&addr.digest_hex);
                if path.exists() {
                    fs::remove_file(&path).map_err(|e| LedgerError::Io {
                        detail: format!("gc delete {a}: {e}"),
                    })?;
                    sync_dir(&self.root.join("blobs")).map_err(io_err)?;
                }
            }
            self.tombstones.insert(a.clone(), MissingReason::Gc);
        }
        Ok(env)
    }

    /// `redact(run, lease, targets, reason_code, endorser, basis)` — the
    /// endorsement-gated tombstone op (§5g.6 redaction). `basis ∈
    /// {approval, policy_rule}` — `approval` requires an endorser at
    /// `principal` or above; `policy_rule` is the kernel scanner's own
    /// channel (authority `kernel`). Anything else is an
    /// `IllegitimateEndorsement`. Targets: `Address(a)` tombstones the blob;
    /// `Field{event_id, field}` names a *declared `content_refs` member* —
    /// an `audit_fields` member answers `NotRedactable` (AC-R-2.8.6-2: audit
    /// fields are never redactable).
    pub fn redact(
        &mut self,
        run_id: &str,
        lease: &Lease,
        targets: Vec<RedactTarget>,
        reason_code: &str,
        endorser: &ProvenanceRecord,
        basis: &str,
    ) -> Result<EventEnvelope, LedgerError> {
        let rec = self.active_lease(run_id, lease)?;
        match basis {
            "approval" => {
                if endorser.authority < hh_provenance::AuthorityClass::Principal {
                    return Err(LedgerError::IllegitimateEndorsement {
                        detail: format!(
                            "redaction basis approval requires principal-or-above \
                             endorser, got {:?}",
                            endorser.authority
                        ),
                    });
                }
            }
            "policy_rule" => {
                if endorser.authority != hh_provenance::AuthorityClass::Kernel {
                    return Err(LedgerError::IllegitimateEndorsement {
                        detail: format!(
                            "redaction basis policy_rule is the kernel scanner's \
                             channel, got {:?}",
                            endorser.authority
                        ),
                    });
                }
            }
            other => {
                return Err(LedgerError::IllegitimateEndorsement {
                    detail: format!("redaction basis {other} is not {{approval, policy_rule}}"),
                })
            }
        }
        // Resolve every target to its addresses before touching anything —
        // the op is atomic.
        let mut resolved: Vec<String> = Vec::new();
        let mut rendered: Vec<String> = Vec::new();
        for t in &targets {
            match t {
                RedactTarget::Address(a) => {
                    if !is_pinned_id(a) {
                        return Err(LedgerError::SchemaViolation {
                            detail: format!("redact target {a} is not an idp/1 id"),
                        });
                    }
                    if let Some(reason) = self.pin_reason(a, run_id) {
                        return Err(LedgerError::Pinned {
                            address: a.clone(),
                            reason,
                        });
                    }
                    rendered.push(a.clone());
                    resolved.push(a.clone());
                }
                RedactTarget::Field { event_id, field } => {
                    let state = self.run(run_id)?;
                    let env = state
                        .events
                        .iter()
                        .find(|e| &e.event_id == event_id)
                        .ok_or_else(|| LedgerError::NotRedactable {
                            target: format!("{event_id}.{field}"),
                            reason: "event_id not committed in this run".into(),
                        })?;
                    let spec =
                        classes::lookup(&env.class).ok_or_else(|| LedgerError::NotRedactable {
                            target: format!("{event_id}.{field}"),
                            reason: format!("class {} unknown", env.class),
                        })?;
                    if spec
                        .audit_fields
                        .iter()
                        .any(|f| f.name == "*" || f.name == field.as_str())
                    {
                        return Err(LedgerError::NotRedactable {
                            target: format!("{event_id}.{field}"),
                            reason: format!(
                                "{}.{field} is an audit_fields member — structural \
                                 audit data is never redactable",
                                env.class
                            ),
                        });
                    }
                    if !spec.content_refs.contains(&field.as_str()) {
                        return Err(LedgerError::NotRedactable {
                            target: format!("{event_id}.{field}"),
                            reason: format!(
                                "{}.{field} is not a declared content_refs member",
                                env.class
                            ),
                        });
                    }
                    let mut addrs: Vec<String> = match env.payload.get(field) {
                        Some(Json::Str(s)) => vec![s.clone()],
                        Some(Json::Arr(items)) => items
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    };
                    for a in &addrs {
                        if let Some(reason) = self.pin_reason(a, run_id) {
                            return Err(LedgerError::Pinned {
                                address: a.clone(),
                                reason,
                            });
                        }
                    }
                    rendered.push(format!("{event_id}.{field}"));
                    resolved.append(&mut addrs);
                }
            }
        }
        // The durable tombstone row — committed before any byte leaves.
        let state = self.runs.get_mut(run_id).unwrap();
        let seq = state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0);
        let prev_hash = state
            .head
            .as_ref()
            .map(|h| h.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let mut members = BTreeMap::new();
        members.insert(
            "targets".to_string(),
            Json::Arr(rendered.iter().map(Json::str).collect()),
        );
        members.insert("reason_code".to_string(), Json::str(reason_code));
        members.insert("endorser".to_string(), endorser.to_json());
        members.insert("basis".to_string(), Json::str(basis));
        // OQ-151's content fingerprints are not yet specified — the member is
        // declared but lands empty until its scheme ratifies (never fabricated).
        members.insert("content_fingerprints".to_string(), Json::Arr(vec![]));
        let env = system_event(
            &*self.ids,
            &*self.clock,
            state,
            seq,
            "lifecycle.ledger.redacted",
            Json::Obj(members),
            &prev_hash,
            rec.generation,
        );
        commit_envelopes(
            state,
            vec![Staged::Durable(env.clone())],
            self.clock.now_ms(),
        )?;
        for a in &resolved {
            if let Ok(addr) = hh_identity::idp::parse_id(a) {
                let path = self.root.join("blobs").join(&addr.digest_hex);
                if path.exists() {
                    fs::remove_file(&path).map_err(|e| LedgerError::Io {
                        detail: format!("redact delete {a}: {e}"),
                    })?;
                    sync_dir(&self.root.join("blobs")).map_err(io_err)?;
                }
            }
            self.tombstones.insert(a.clone(), MissingReason::Redacted);
        }
        Ok(env)
    }
    /// `project(run, view_kind, until?)` — a pure fold over the durable prefix,
    /// stamped `(run_id, seq)` watermark + `view_policy_version` + `view_hash`.
    pub fn project(
        &self,
        run_id: &str,
        kind: ViewKind,
        until: Option<u64>,
    ) -> Result<View, LedgerError> {
        let state = self.run(run_id)?;
        Ok(match kind {
            ViewKind::ContextView => views::context_view(run_id, &state.events, until),
            ViewKind::Checkpoint => views::checkpoint(run_id, &state.events, until),
            ViewKind::EffectLedger => views::effect_ledger(run_id, &state.events, until),
            // `audit_view` is folded by the ledger itself — the audit trail *is*
            // the run ledger (ADR-0066 D1: no second store, no audit-only path).
            // The cross-run lookup recomputes the named run's head at the
            // claimed size — a held-but-shorter run contradicts the claim
            // (`failed`); an absent run is `unresolvable`.
            ViewKind::AuditView => crate::audit::audit_view(
                run_id,
                &state.events,
                &state.open_scopes,
                state.finished,
                |addr| self.blob_status(addr),
                until,
                &state.manifest,
                |other, size| {
                    self.runs.get(other).map(|s| {
                        let leaves: Vec<String> = s.events.iter().map(|e| e.hash.clone()).collect();
                        (size, crate::tree::mth_prefix(&leaves, size as usize))
                    })
                },
                self.audit_keys.as_deref(),
            ),
            ViewKind::RunSummary => views::run_summary(
                state.manifest.run_kind.as_str(),
                state.manifest.participant_class.as_str(),
                &state.manifest.observability_level,
                &state.events,
                &state.open_scopes.keys().cloned().collect::<BTreeSet<_>>(),
                run_id,
                until,
            ),
            // `trace_view`/`cost_view`/`metric_view` fold in the owning crate
            // (`hh-telemetry`) over `read` output — the ledger cannot depend
            // on its consumer (ADR-0042 D2).
            // `lexical_index`/`memory_stale_index`/`memory_usage` fold in
            // `hh-context` over the `MemoryStore` (§5c.3 — the store, not
            // the run log, is their input; same owner-projection rule).
            ViewKind::TraceView
            | ViewKind::CostView
            | ViewKind::MetricView
            | ViewKind::LexicalIndex
            | ViewKind::MemoryStaleIndex
            | ViewKind::MemoryUsage => {
                return Err(LedgerError::OwnerProjected {
                    kind: kind.as_str(),
                });
            }
        })
    }

    /// `lineage(run) → [{run_id, up_to_seq, head_hash}]` — resolves
    /// `forked_from`/`continued_from` **root-first**, ending with the run itself
    /// (ADR-0027 §6; ADR-0131 §5).
    pub fn lineage(&self, run_id: &str) -> Result<Vec<LineageEntry>, LedgerError> {
        if !self.runs.contains_key(run_id) {
            return Err(LedgerError::UnknownRun {
                run_id: run_id.to_string(),
            });
        }
        let mut chain: Vec<LineageEntry> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut cur = run_id.to_string();
        loop {
            if !seen.insert(cur.clone()) {
                break; // defensive: a lineage cycle cannot form, but never loop.
            }
            let state = &self.runs[&cur];
            let link = state
                .manifest
                .continued_from
                .as_ref()
                .or(state.manifest.forked_from.as_ref());
            match link {
                Some(l) => {
                    chain.push(LineageEntry {
                        run_id: l.run_id.clone(),
                        up_to_seq: l.at_seq,
                        head_hash: l.head_hash.clone(),
                    });
                    cur = l.run_id.clone();
                }
                None => break,
            }
        }
        chain.reverse(); // root-first
        let head = self.runs[run_id].head.clone().unwrap_or(Head {
            seq: 0,
            event_id: String::new(),
            hash: String::new(),
        });
        chain.push(LineageEntry {
            run_id: run_id.to_string(),
            up_to_seq: head.seq,
            head_hash: head.hash,
        });
        Ok(chain)
    }

    /// Every committed event of a run (the durable prefix) — a read helper.
    pub fn events(&self, run_id: &str) -> Result<&[EventEnvelope], LedgerError> {
        Ok(&self.run(run_id)?.events)
    }

    /// The run's manifest — the immutable seq-0 record (§5g.6's
    /// `audit_policy_ref`/`signer_key_ids` live here).
    pub fn manifest(&self, run_id: &str) -> Result<&RunManifest, LedgerError> {
        Ok(&self.run(run_id)?.manifest)
    }

    /// The run's currently-open scopes (`id → kind`) — `audit_view`'s
    /// `scopes_unclosed` read.
    pub fn open_scopes(&self, run_id: &str) -> Result<Vec<(String, ScopeKind)>, LedgerError> {
        Ok(self
            .run(run_id)?
            .open_scopes
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }

    /// Whether a blob exists at the address — `audit_view`'s `content_refs`
    /// presence accounting (a missing blob is `missing`, never `tampered` —
    /// ADR-0068 R3).
    pub fn blob_present(&self, address: &str) -> bool {
        hh_identity::idp::parse_id(address)
            .map(|p| self.root.join("blobs").join(&p.digest_hex).exists())
            .unwrap_or(false)
    }

    /// Every `(effect_id, fold)` in id order — the reconciliation sweep's
    /// enumeration (e.g. `reconcile_detached`; `effect_fold` is the
    /// single-id form, `effects_in_state` the phase-indexed one).
    pub fn effect_folds(&self, run_id: &str) -> Result<Vec<(String, EffectFold)>, LedgerError> {
        Ok(self
            .run(run_id)?
            .effects
            .iter()
            .map(|(id, f)| (id.clone(), f.clone()))
            .collect())
    }

    /// `effect_id = f(run_id, model_call_id, tool_call_id, ordinal)` — the derived
    /// id (ADR-0027 §2).
    pub fn effect_id(
        run_id: &str,
        model_call_id: &str,
        tool_call_id: &str,
        ordinal: u64,
    ) -> String {
        derive_effect_id(run_id, model_call_id, tool_call_id, ordinal)
    }

    /// A test/deterministic constructor: fresh store at `root` with [`SeqIds`] +
    /// [`crate::ids::ManualClock`] frozen at `ms`.
    pub fn open_test(root: impl AsRef<Path>, ms: u64) -> Result<Store, LedgerError> {
        Store::open_with(
            root,
            Box::new(crate::ids::ManualClock::at(ms)),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// System rows + commit — free functions so `state` (a `&mut` borrow of
// `self.runs[..]`) can be held across them alongside disjoint `self` fields.
// ─────────────────────────────────────────────────────────────────────────────

/// Rule C (§5g.6 I-A1; ADR-0066 D3): an audit-grade payload is an object whose
/// every member is either a declared `audit_fields` member — inline, canonical,
/// hashed, never offloaded, never redactable, bounded — or a declared
/// `content_refs` member naming content addresses. Any violation is a schema
/// error, never an offload opportunity (AC-R-2.8.6-12).
fn check_audit_partition(
    class: &str,
    payload: &Json,
    spec: &classes::ClassSpec,
) -> Result<(), LedgerError> {
    let members = match payload {
        Json::Obj(m) => m,
        _ => {
            return Err(LedgerError::SchemaViolation {
                detail: format!(
                    "audit-grade {class} payload must be an object — every member \
                     partitions into audit_fields | content_refs"
                ),
            })
        }
    };
    let open = spec.audit_fields.iter().any(|f| f.name == "*");
    let mut audit_bytes = 0usize;
    for (name, value) in members {
        if spec.content_refs.contains(&name.as_str()) {
            // A content ref is a ContentAddress — a string (or array of strings)
            // pinned under `idp/1`. Free text here is a schema error, not an
            // offload.
            let ok = match value {
                Json::Null => true,
                Json::Str(s) => crate::ids::is_pinned_id(s),
                Json::Arr(items) => items
                    .iter()
                    .all(|i| i.as_str().map(crate::ids::is_pinned_id).unwrap_or(false)),
                _ => false,
            };
            if !ok {
                return Err(LedgerError::SchemaViolation {
                    detail: format!(
                        "audit-grade {class}.{name} is a content_refs member but \
                         is not a content address"
                    ),
                });
            }
            continue;
        }
        let bound = if open {
            classes::AUDIT_FIELD_MAX_BYTES
        } else {
            match spec.audit_fields.iter().find(|f| f.name == name.as_str()) {
                Some(f) => f.max_bytes,
                None => {
                    return Err(LedgerError::SchemaViolation {
                        detail: format!(
                            "audit-grade {class}.{name} is not a declared \
                             audit_fields or content_refs member"
                        ),
                    })
                }
            }
        };
        let bytes = value.to_canonical_string().len();
        audit_bytes += bytes;
        if bytes > bound {
            return Err(LedgerError::AuditFieldsTooLarge {
                class: class.to_string(),
                bytes,
                max: bound,
            });
        }
    }
    // AC-R-2.8.6-12: the partition's bound *is* the class's offload threshold —
    // audit fields past it are a schema error, never an offload opportunity.
    if audit_bytes > spec.offload_threshold {
        return Err(LedgerError::AuditFieldsTooLarge {
            class: class.to_string(),
            bytes: audit_bytes,
            max: spec.offload_threshold,
        });
    }
    Ok(())
}

/// Append one `lifecycle.lease.*` row as the ledger's own audit fact — written under
/// the *current* generation, never through the client-lease path.
fn emit_lease_row(
    ids: &dyn IdSource,
    clock: &dyn Clock,
    state: &mut RunState,
    class: &str,
    rec: &LeaseRecord,
    extra: Json,
) -> Result<(), LedgerError> {
    let mut payload = BTreeMap::new();
    payload.insert("scope".to_string(), Json::str("writer"));
    payload.insert("lease_id".to_string(), Json::str(&rec.lease_id));
    payload.insert("holder".to_string(), Json::str(&rec.holder));
    payload.insert("generation".to_string(), Json::Int(rec.generation as i64));
    if let Json::Obj(m) = extra {
        payload.extend(m);
    }
    let prev = state
        .head
        .as_ref()
        .map(|h| h.hash.clone())
        .unwrap_or_else(|| GENESIS_HASH.to_string());
    let env = system_event(
        ids,
        clock,
        state,
        state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0),
        class,
        Json::Obj(payload),
        &prev,
        rec.generation,
    );
    commit_envelopes(state, vec![Staged::Durable(env)], clock.now_ms())?;
    Ok(())
}

/// Build a kernel-stamped envelope (system rows — the ledger's own audit facts).
#[allow(clippy::too_many_arguments)] // the stamp needs every field it takes.
fn system_event(
    ids: &dyn IdSource,
    clock: &dyn Clock,
    state: &RunState,
    seq: u64,
    class: &str,
    payload: Json,
    prev_hash: &str,
    generation: u64,
) -> EventEnvelope {
    let parent = state
        .head
        .as_ref()
        .map(|h| h.event_id.clone())
        .unwrap_or_else(|| ROOT_EVENT.to_string());
    let mut observability = state.manifest.observability_level.clone();
    if state.manifest.participant_class == ParticipantClass::Native {
        observability.insert(ObservabilityLevel::Ledger);
    }
    let mut env = EventEnvelope {
        event_id: ids.alloc("evt"),
        run_id: state.run_id.clone(),
        seq,
        ts: rfc3339_ms(clock.now_ms()),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Lifecycle),
        class: class.to_string(),
        schema_version: SCHEMA_VERSION,
        producer: Producer::kernel(KERNEL_LEDGER),
        participant_class: state.manifest.participant_class,
        observability_level: observability,
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: generation,
        parent_event_id: parent,
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(KERNEL_LEDGER, seq)),
        prev_hash: prev_hash.to_string(),
        payload,
        hash: String::new(),
    };
    env.hash = env.recompute_hash();
    env
}

/// Stamp a caller `Event` into an ephemeral record — no seq, no hashes, durable only
/// inside a subscriber's buffers.
fn stamp_ephemeral(state: &RunState, ev: &Event, generation: u64) -> EphemeralRecord {
    let mut observability = state.manifest.observability_level.clone();
    if state.manifest.participant_class == ParticipantClass::Native {
        observability.insert(ObservabilityLevel::Ledger);
    }
    EphemeralRecord {
        event_id: ev.event_id.clone(),
        run_id: state.run_id.clone(),
        ts: ev.ts.clone(),
        hlc: ev.hlc.clone(),
        plane: EventPlane::of_class(&ev.class).unwrap_or(EventPlane::Action),
        class: ev.class.clone(),
        schema_version: SCHEMA_VERSION,
        producer: ev.producer.clone(),
        participant_class: state.manifest.participant_class,
        observability_level: observability,
        scope: ev.scope.clone(),
        lease_generation: generation,
        parent_event_id: ev.parent_event_id.clone(),
        causes: ev.causes.clone(),
        refs: ev.refs.clone(),
        ir_refs: ev.ir_refs.clone(),
        surface_ids: ev.surface_ids.clone(),
        provenance: ev.provenance.clone(),
        payload: ev.payload.clone(),
        durable_event_seq: None,
    }
}

/// A staged record — durable envelopes and ephemeral records in batch order, so the
/// subscriber stream preserves submission order across a mixed batch.
enum Staged {
    /// A durable envelope (written, synced, then visible).
    Durable(EventEnvelope),
    /// An ephemeral record (visible to subscribers only).
    Eph(EphemeralRecord),
}

/// Write + sync the batch's durable lines, write + sync the commit marker, then make
/// everything visible — and only then notify subscribers, in submission order
/// (durable-before-visible).
fn commit_envelopes(
    state: &mut RunState,
    mut staged: Vec<Staged>,
    now_ms: u64,
) -> Result<SeqRange, LedgerError> {
    // ── HLC stamp (R-2.2.3⁰ᵇ; ADR-0131 §5) ────────────────────────────────
    // Continuation/child runs stamp `hlc` on every durable event — set it
    // here, then re-hash so the stamp rides the chain (plain runs carry no
    // `hlc` — byte-goldens never move, CC8).
    if let Some(node) = state.hlc_node.clone() {
        for s in staged.iter_mut() {
            if let Staged::Durable(env) = s {
                let h = state
                    .hlc_last
                    .take()
                    .map(|prev| prev.tick(now_ms))
                    .unwrap_or_else(|| crate::hlc::Hlc::seed(now_ms, None, &node));
                env.hlc = Some(h.render());
                env.hash = env.recompute_hash();
                state.hlc_last = Some(h);
            }
        }
    }
    let durable: Vec<&EventEnvelope> = staged
        .iter()
        .filter_map(|s| match s {
            Staged::Durable(e) => Some(e),
            _ => None,
        })
        .collect();
    let wal = state.dir.join("events.wal");
    // ── durable write ───────────────────────────────────────────────────
    if let (Some(first), Some(last)) = (durable.first(), durable.last()) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal)
            .map_err(io_err)?;
        let mut buf = Vec::new();
        for env in &durable {
            buf.extend_from_slice(b"{\"k\":\"e\",\"v\":");
            buf.extend_from_slice(env.canonical_bytes().as_slice());
            buf.push(b'}');
            buf.push(b'\n');
        }
        f.write_all(&buf).map_err(io_err)?;
        f.sync_all().map_err(io_err)?;
        let marker = format!("{{\"k\":\"c\",\"n\":{}}}\n", last.seq);
        f.write_all(marker.as_bytes()).map_err(io_err)?;
        f.sync_all().map_err(io_err)?;
        sync_dir(&state.dir).map_err(io_err)?;
        let _ = first;
    }
    // ── visible ─────────────────────────────────────────────────────────
    let mut range = SeqRange {
        first: 0,
        last: 0,
        count: 0,
    };
    for s in staged {
        match s {
            Staged::Durable(env) => {
                state.by_event_id.insert(env.event_id.clone(), env.seq);
                for r in &env.ir_refs {
                    state
                        .ir_index
                        .entry(r.version_id.clone())
                        .or_default()
                        .push(env.seq);
                }
                apply_scope_marks(&mut state.open_scopes, &state.effects, &env);
                effect::fold_event(&mut state.effects, &env);
                effect::apply_decision(&mut state.decisions, &state.effects, &env);
                crate::leases::fold_lease_row(&mut state.scoped_leases, &env);
                crate::wakeup::fold_wakeup_row(&mut state.wakeups, &env);
                match env.class.as_str() {
                    "lifecycle.run.finished" => state.finished = true,
                    "lifecycle.run.suspended" => state.suspended = true,
                    "lifecycle.run.resumed" => state.suspended = false,
                    _ => {}
                }
                state.head = Some(Head {
                    seq: env.seq,
                    event_id: env.event_id.clone(),
                    hash: env.hash.clone(),
                });
                state.tree.push(env.hash.clone());
                if range.count == 0 {
                    range.first = env.seq;
                }
                range.last = env.seq;
                range.count += 1;
                let frame = EventFrame::Durable {
                    seq: env.seq,
                    hash: env.hash.clone(),
                    event: Box::new(env.clone()),
                };
                state.events.push(env);
                notify(state, frame);
            }
            Staged::Eph(eph) => notify(
                state,
                EventFrame::Ephemeral {
                    event: Box::new(eph),
                },
            ),
        }
    }
    if state.finished {
        notify(
            state,
            EventFrame::Closed {
                reason: crate::event::CloseReason::RunFinished,
            },
        );
    }
    Ok(range)
}

// ─────────────────────────────────────────────────────────────────────────────
// Scope validation
// ─────────────────────────────────────────────────────────────────────────────

/// Apply one committed envelope's scope opens/closes — the single fold both
/// `rebuild` (replay) and `commit_envelopes` (live) run so the two paths can
/// never disagree (CC1). Honors `effect::scope_close_fires` — `observed{partial}`,
/// `probed{undeterminable}` and a retryable `…{not_applied}` leave the effect
/// scope open (ADR-0238 §1).
fn apply_scope_marks(
    open: &mut BTreeMap<String, ScopeKind>,
    effects: &BTreeMap<String, effect::EffectFold>,
    env: &EventEnvelope,
) {
    if let Some(spec) = classes::lookup(&env.class) {
        if let Some(kind) = spec.opens_scope {
            if let Some(id) = env.scope.get(kind) {
                open.insert(id.to_string(), kind);
            }
        }
        if let Some(kind) = spec.closes_scope {
            if let Some(id) = env.scope.get(kind) {
                let fold = if kind == ScopeKind::Effect {
                    effects.get(id)
                } else {
                    None
                };
                if effect::scope_close_fires(&env.class, &env.payload, fold) {
                    open.remove(id);
                }
            }
        }
    }
}

/// The scope rules (§5a.1 §3): the chain `run ⊃ turn ⊃ model_call ⊃ tool_call` is
/// enforced (a scope id may only be set when its parent scope is set); every scope id
/// must name an *opened* scope — except the class's own `opens_scope` field (it opens)
/// and `closes_scope` field (it must be open, then closes).
fn check_scopes(
    ev: &Event,
    spec: &classes::ClassSpec,
    open: &mut BTreeMap<String, ScopeKind>,
    effects: &BTreeMap<String, effect::EffectFold>,
) -> Result<(), LedgerError> {
    let bad = |d: String| LedgerError::SchemaViolation { detail: d };
    // The chain: model_call ⇒ turn; tool_call ⇒ model_call.
    if ev.scope.model_call_id.is_some() && ev.scope.turn_id.is_none() {
        return Err(bad(format!(
            "{} carries model_call_id without turn_id",
            ev.class
        )));
    }
    if ev.scope.tool_call_id.is_some() && ev.scope.model_call_id.is_none() {
        return Err(bad(format!(
            "{} carries tool_call_id without model_call_id",
            ev.class
        )));
    }
    for (kind, value) in [
        (ScopeKind::Turn, &ev.scope.turn_id),
        (ScopeKind::ModelCall, &ev.scope.model_call_id),
        (ScopeKind::ToolCall, &ev.scope.tool_call_id),
        (ScopeKind::Effect, &ev.scope.effect_id),
        (ScopeKind::ChildRun, &ev.scope.child_run_id),
        (ScopeKind::Branch, &ev.scope.branch_id),
    ] {
        let Some(id) = value else { continue };
        if spec.opens_scope == Some(kind) {
            // This event opens the scope — the id must be fresh.
            if open.contains_key(id) {
                return Err(bad(format!("scope {} {id} already open", kind.field())));
            }
            open.insert(id.clone(), kind);
        } else if spec.closes_scope == Some(kind) {
            if !open.contains_key(id) {
                return Err(LedgerError::ScopeNotOpen {
                    scope: kind.field(),
                    id: id.clone(),
                });
            }
            // §5a.2 conditional closers (`effect::scope_close_fires` is the one
            // predicate — CC1): `observed{partial}`, `probed{undeterminable}`
            // and a retryable `…{not_applied}` leave the scope open.
            let fold = if kind == ScopeKind::Effect {
                effects.get(id)
            } else {
                None
            };
            if effect::scope_close_fires(&ev.class, &ev.payload, fold) {
                open.remove(id);
            }
        } else if !open.contains_key(id) {
            return Err(LedgerError::ScopeNotOpen {
                scope: kind.field(),
                id: id.clone(),
            });
        }
    }
    Ok(())
}

/// The ADR-0035 §3 write ceilings, applied to the scopes *above* the run boundary:
/// `definition` only via `seal`; `user` needs `principal` + approval evidence (an
/// attestation at this stage — a `permission`-basis endorsement is Stage 2);
/// `project` needs `principal`; `session` needs `delegate`.
///
/// `run`/`turn` scope is exempt: it is where external and imported content
/// legitimately lives (an `origin=import` record mints `unverified` — refusing it at
/// run scope would make the import path unwritable). The §3 floor for
/// `session`/`run`/`turn` is enforced at `session` and above; tightening `run`/`turn`
/// to `delegate` is the memory-promotion (`check_persist`) concern, which lands with
/// the context/memory slices — interim ruling recorded in ADR-0234.
fn check_scope_ceiling(p: &ProvenanceRecord) -> Result<(), LedgerError> {
    use hh_provenance::{AttestationKind, AuthorityClass};
    let err = |scope| LedgerError::ScopeCeilingExceeded {
        scope,
        authority: p.authority,
    };
    match p.scope {
        hh_provenance::PersistenceScope::Definition => {
            let sealed = p
                .attestation
                .as_ref()
                .is_some_and(|a| a.kind == AttestationKind::Seal);
            if !sealed {
                return Err(err(p.scope));
            }
        }
        hh_provenance::PersistenceScope::User => {
            if p.authority < AuthorityClass::Principal || p.attestation.is_none() {
                return Err(err(p.scope));
            }
        }
        hh_provenance::PersistenceScope::Project => {
            if p.authority < AuthorityClass::Principal {
                return Err(err(p.scope));
            }
        }
        hh_provenance::PersistenceScope::Session => {
            if p.authority < AuthorityClass::Delegate {
                return Err(err(p.scope));
            }
        }
        hh_provenance::PersistenceScope::Run | hh_provenance::PersistenceScope::Turn => {}
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// WAL mechanics
// ─────────────────────────────────────────────────────────────────────────────

fn io_err(e: std::io::Error) -> LedgerError {
    LedgerError::Durability {
        detail: e.to_string(),
    }
}

fn sync_dir(dir: &Path) -> std::io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// The committed-prefix replay, with the raw line bytes retained — `verify`
/// diffs each line against its canonical re-rendering (byte-exact, ADR-0067 D5).
/// `tail_corrupt` reports an unparseable line that is **not** the last
/// non-empty line: a torn tail is a crash (legit — the uncommitted suffix is
/// dropped); unparseable bytes *inside* the file are an edit.
struct WalReplay {
    /// The committed envelopes and their stored line bytes, in commit order.
    committed: Vec<(Vec<u8>, EventEnvelope)>,
    /// A parseable line whose bytes differ from its canonical form, or an
    /// unparseable line followed by more content — mid-file corruption, never a
    /// torn tail. Carries the count of committed events collected so far.
    noncanonical_at: Option<usize>,
}

/// Replay a WAL: only envelopes covered by a subsequent commit marker are durable;
/// a trailing run of event lines without a marker is the torn tail — dropped,
/// never surfaced.
fn replay_committed(wal: &Path) -> std::io::Result<Vec<EventEnvelope>> {
    Ok(replay_committed_raw(wal)?
        .committed
        .into_iter()
        .map(|(_, e)| e)
        .collect())
}

fn replay_committed_raw(wal: &Path) -> std::io::Result<WalReplay> {
    let text = match fs::read_to_string(wal) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WalReplay {
                committed: Vec::new(),
                noncanonical_at: None,
            })
        }
        Err(e) => return Err(e),
    };
    let mut committed: Vec<(Vec<u8>, EventEnvelope)> = Vec::new();
    let mut pending: Vec<(Vec<u8>, EventEnvelope)> = Vec::new();
    let mut noncanonical_at = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut first_bad: Option<usize> = None;
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(j) = json::parse(line) else {
            // An unparseable line mid-file is a torn write — the rest of the file is
            // not durable. If non-empty content follows it, the file was edited.
            first_bad = Some(i);
            break;
        };
        // Byte-exactness: a parseable line must re-render to its stored bytes
        // (untrimmed — a whitespace edit is a rewrite).
        if noncanonical_at.is_none() && j.to_canonical_string().as_bytes() != raw.as_bytes() {
            noncanonical_at = Some(committed.len() + pending.len());
        }
        match j.get("k").and_then(Json::as_str) {
            Some("e") => {
                let Some(v) = j.get("v") else {
                    first_bad = Some(i);
                    break;
                };
                let Ok(env) = EventEnvelope::from_json(v) else {
                    first_bad = Some(i);
                    break;
                };
                pending.push((raw.as_bytes().to_vec(), env));
            }
            Some("c") => {
                let Some(n) = j.get("n").and_then(Json::as_int) else {
                    first_bad = Some(i);
                    break;
                };
                let n = n as u64;
                // Commit everything pending with seq ≤ n, in file order.
                for (raw, env) in pending.drain(..) {
                    if env.seq <= n {
                        committed.push((raw, env));
                    }
                }
            }
            _ => {}
        }
    }
    // Content after the first unparseable line is mid-file corruption — a torn
    // tail is always the last line.
    if let Some(i) = first_bad {
        if lines[i + 1..].iter().any(|l| !l.trim().is_empty()) {
            noncanonical_at = Some(committed.len() + pending.len());
        }
    }
    Ok(WalReplay {
        committed,
        noncanonical_at,
    })
}

fn read_lease_file(path: &Path) -> Result<Option<LeaseRecord>, LedgerError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(lease_record_from_json(&json::parse(&text).map_err(
            |e| LedgerError::Io {
                detail: format!("lease file unreadable: {e}"),
            },
        )?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(LedgerError::Io {
            detail: format!("read lease file: {e}"),
        }),
    }
}

fn write_lease_file(dir: &Path, rec: &LeaseRecord) -> Result<(), LedgerError> {
    let tmp = dir.join("lease.json.tmp");
    let path = dir.join("lease.json");
    fs::write(&tmp, lease_record_json(rec).to_canonical_string()).map_err(|e| LedgerError::Io {
        detail: format!("write lease tmp: {e}"),
    })?;
    fs::rename(&tmp, &path).map_err(|e| LedgerError::Io {
        detail: format!("rename lease: {e}"),
    })?;
    sync_dir(dir).map_err(io_err)?;
    Ok(())
}

fn notify(state: &mut RunState, frame: EventFrame) {
    let mut drop_idx: Vec<usize> = Vec::new();
    for (i, sub) in state.subscribers.iter_mut().enumerate() {
        let seq = match &frame {
            EventFrame::Durable { seq, .. } => Some(*seq),
            _ => None,
        };
        if sub.lagged_from.is_some() {
            // Still behind — try to deliver the lagged marker first.
            let lag = EventFrame::Lagged {
                missed_from_seq: sub.lagged_from.unwrap(),
            };
            match sub.tx.try_send(lag) {
                Ok(()) => sub.lagged_from = None,
                Err(_) => {
                    if let Some(s) = seq {
                        sub.lagged_from = Some(sub.lagged_from.unwrap().min(s));
                    }
                    continue;
                }
            }
        }
        match sub.tx.try_send(frame.clone()) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                // A non-durable frame carries no seq — the subscriber resumes from the
                // next durable seq (ephemeral loss needs no durable resync point).
                let from =
                    seq.unwrap_or_else(|| state.head.as_ref().map(|h| h.seq + 1).unwrap_or(0));
                sub.lagged_from = Some(sub.lagged_from.map_or(from, |l| l.min(from)));
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => drop_idx.push(i),
        }
    }
    for i in drop_idx.into_iter().rev() {
        state.subscribers.remove(i);
    }
}

/// Is this manifest the child of a continuation/fork/spawn? — the `R-2.2.3⁰ᵇ`
/// HLC-stamp gate (`hlc_node = run_id` on lineage-bearing runs only).
fn manifest_has_lineage(m: &RunManifest) -> bool {
    m.continued_from.is_some() || m.forked_from.is_some() || m.parent_run_id.is_some()
}

/// RFC 3339 UTC ms — `YYYY-MM-DDTHH:MM:SS.mmmZ` (the civil-from-days algorithm).
pub fn rfc3339_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let millis = ms % 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    if mo <= 2 {
        y += 1;
    }
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_formats_utc_ms() {
        assert_eq!(rfc3339_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339_ms(1_789_200_000_123), "2026-09-12T08:00:00.123Z");
        assert!(valid_ts(&rfc3339_ms(1_789_200_000_123)));
    }
}
