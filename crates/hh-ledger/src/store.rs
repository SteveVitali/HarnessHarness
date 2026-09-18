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

use crate::classes::{self, Durability, ScopeKind};
use crate::effect::{self, EffectCtx, EffectFold, EffectPhase};
use crate::errors::{LedgerError, MissingReason, Tampered, TamperedKind};
use crate::event::{
    Cursor, Direction, EphemeralRecord, Event, EventEnvelope, EventFrame, EventPlane, Head, Page,
    Producer, ReadFilter, Scope, SeqRange,
};
use crate::ids::{
    effect_id as derive_effect_id, valid_ts, Clock, IdSource, SeqIds, SystemClock, TimeIds,
    GENESIS_HASH, ROOT_EVENT,
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
    head: Option<Head>,
    pub(crate) finished: bool,
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
        };
        store.load_all()?;
        Ok(store)
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
            subscribers: Vec::new(),
        };
        for env in committed {
            if env.seq == 0 && env.class == "lifecycle.run.created" {
                state.manifest = RunManifest::from_json(&env.payload)?;
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
            if env.class == "lifecycle.run.finished" {
                state.finished = true;
            }
            state.head = Some(Head {
                seq: env.seq,
                event_id: env.event_id.clone(),
                hash: env.hash.clone(),
            });
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
            subscribers: Vec::new(),
        };
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
        commit_envelopes(&mut state, vec![Staged::Durable(created)])?;
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
        commit_envelopes(state, staged)
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
                return Err(LedgerError::Missing {
                    address: id,
                    reason: MissingReason::Gc,
                })
            }
        };
        if hh_identity::idp::idp_digest("blob", &bytes) != address.digest {
            return Err(LedgerError::BlobCorrupt { address: id });
        }
        Ok(bytes)
    }

    // ── verify / project / lineage ───────────────────────────────────────

    /// `verify(run, from?, to?) → ok | Tampered{at_seq, kind}` — re-reads the WAL
    /// from disk (the durable truth), replays the committed prefix and checks it
    /// **byte-exact**: every stored line must re-render to its canonical bytes
    /// (`NonCanonicalBytes` — a rewritten line, even a semantically equal one),
    /// every hash recomputes (`ContentModified`), `prev_hash` chains
    /// (`ChainBroken`), seqs are dense and ordered (`SeqGap`/`Reordered`/
    /// `DuplicateSeq`/`DuplicateEventId`), parents resolve (`DanglingParent`),
    /// and audit-grade rows carry a producer from the class's declared set with
    /// `authority = kernel` provenance (`Producer` — §5g.6's `producer` kind).
    /// Tail-truncation detection is the signed-checkpoint half (Stage 2,
    /// `security.audit.checkpoint`); within the committed prefix every byte
    /// change is caught.
    pub fn verify(&self, run_id: &str) -> Result<(), LedgerError> {
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
        Ok(())
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
            ViewKind::AuditView => crate::audit::audit_view(
                run_id,
                &state.events,
                &state.open_scopes,
                state.finished,
                |addr| self.blob_present(addr),
                until,
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
    commit_envelopes(state, vec![Staged::Durable(env)])?;
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
fn commit_envelopes(state: &mut RunState, staged: Vec<Staged>) -> Result<SeqRange, LedgerError> {
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
                if env.class == "lifecycle.run.finished" {
                    state.finished = true;
                }
                state.head = Some(Head {
                    seq: env.seq,
                    event_id: env.event_id.clone(),
                    hash: env.hash.clone(),
                });
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
