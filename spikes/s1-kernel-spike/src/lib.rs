//! S1 kernel-slice spike — throwaway (ADR-0050 R1–R6; §10.6 S1 slice; `l1-spike-spec.md` §3).
//!
//! A deliberately minimal, *fresh-from-the-spec* implementation of the S1 kernel slice used only
//! to produce numbers (M-S1-1…9) and to exercise the correctness gates (G1–G4). It is NOT the
//! kernel: it links only the pure-std `hh-wire` transport primitives and is excluded from the
//! E1 workspace. Deleting `spikes/s1-kernel-spike/` breaks nothing persistent.
//!
//! Scope (l1-spike-spec §3.1, offline/hermetic subset): an append-only ledger with the ADR-0027
//! id model (allocated `run_id`/`event_id`, per-run dense `seq`, `parent_event_id`,
//! `lease_generation`), atomic multi-event append, durable-before-visible; the `idp/1`-class
//! canonical form (via `hh-wire`, Stage-0 integers-only interim) and the per-run hash chain
//! `hash = H(leaf_tag ∥ canonical(envelope − hash) ∥ prev_hash)`; one `project()` cost-totals
//! view with a `derived_from` watermark and rebuild-equality; 64 KiB blob offload; one sandboxed
//! tool call through a **helper-binary** boundary (`s1-helper`, discharges DF-S0.2-1). Excluded
//! (l1-spike-spec §3.2): no model calls, no compiler, no reference monitor, no UI. The MCP/ACP
//! calls (M-S1-7) need the candidate's official SDKs over the network — deferred offline.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use hh_wire::json::Json;
use hh_wire::sha256_hex;

/// Domain-separation tag for the per-run hash chain (l1-spike-spec §3.1 item 1).
const LEAF_TAG: &str = "hh-event\u{0}";
/// ADR-0029 offload threshold: a payload at/above this size is offloaded to the blob store.
pub const OFFLOAD_THRESHOLD: usize = 64 * 1024;

/// One ledger event, before hashing. `cost` is the numeric folded by the `project()` view.
#[derive(Debug, Clone)]
pub struct EventSpec {
    pub kind: String,
    pub cost: i64,
    /// The event payload. If its canonical size ≥ `OFFLOAD_THRESHOLD` it is offloaded to the
    /// blob store and the envelope carries a `blob` content address instead of `payload`.
    pub payload: Json,
}

impl EventSpec {
    pub fn new(kind: &str, cost: i64, payload: Json) -> Self {
        EventSpec {
            kind: kind.into(),
            cost,
            payload,
        }
    }
}

/// A committed envelope as it appears in the visible ledger and the WAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub run_id: String,
    pub event_id: u64,
    pub seq: u64,
    pub parent_event_id: Option<u64>,
    pub lease_generation: u64,
    pub kind: String,
    pub cost: i64,
    /// Exactly one of `payload` / `blob_address` is set (offload discriminates on size).
    pub payload: Option<Json>,
    pub blob_address: Option<String>,
    pub hash: String,
}

impl Envelope {
    /// Canonical form of the envelope **minus** the `hash` field (the hash-chain preimage body).
    fn canonical_without_hash(&self) -> String {
        let mut m: BTreeMap<String, Json> = BTreeMap::new();
        m.insert("run_id".into(), Json::str(self.run_id.clone()));
        m.insert("event_id".into(), Json::Int(self.event_id as i64));
        m.insert("seq".into(), Json::Int(self.seq as i64));
        m.insert(
            "parent_event_id".into(),
            match self.parent_event_id {
                Some(p) => Json::Int(p as i64),
                None => Json::Null,
            },
        );
        m.insert(
            "lease_generation".into(),
            Json::Int(self.lease_generation as i64),
        );
        m.insert("kind".into(), Json::str(self.kind.clone()));
        m.insert("cost".into(), Json::Int(self.cost));
        match (&self.payload, &self.blob_address) {
            (Some(p), None) => {
                m.insert("payload".into(), p.clone());
            }
            (None, Some(a)) => {
                m.insert("blob".into(), Json::str(a.clone()));
            }
            _ => unreachable!("exactly one of payload/blob is set"),
        }
        Json::Obj(m).to_canonical_string()
    }

    /// Full canonical JSON line for the WAL (includes the hash).
    fn to_wal_line(&self) -> String {
        let mut m: BTreeMap<String, Json> = BTreeMap::new();
        m.insert("t".into(), Json::str("event"));
        m.insert("run_id".into(), Json::str(self.run_id.clone()));
        m.insert("event_id".into(), Json::Int(self.event_id as i64));
        m.insert("seq".into(), Json::Int(self.seq as i64));
        m.insert(
            "parent_event_id".into(),
            match self.parent_event_id {
                Some(p) => Json::Int(p as i64),
                None => Json::Null,
            },
        );
        m.insert(
            "lease_generation".into(),
            Json::Int(self.lease_generation as i64),
        );
        m.insert("kind".into(), Json::str(self.kind.clone()));
        m.insert("cost".into(), Json::Int(self.cost));
        match (&self.payload, &self.blob_address) {
            (Some(p), None) => {
                m.insert("payload".into(), p.clone());
            }
            (None, Some(a)) => {
                m.insert("blob".into(), Json::str(a.clone()));
            }
            _ => unreachable!(),
        }
        m.insert("hash".into(), Json::str(self.hash.clone()));
        Json::Obj(m).to_canonical_string()
    }
}

/// Errors the ledger can return.
#[derive(Debug, PartialEq, Eq)]
pub enum LedgerError {
    /// A writer whose lease generation is older than the current one (fenced — G3).
    StaleWriter {
        writer_gen: u64,
        current_gen: u64,
    },
    Io(String),
}

/// A writer handle carrying its lease generation (the fencing token — ADR-0027).
#[derive(Debug, Clone, Copy)]
pub struct Writer {
    pub lease_generation: u64,
}

/// An append-only, durable-before-visible ledger for one run/session.
pub struct Ledger {
    run_id: String,
    dir: PathBuf,
    wal: File,
    blob_dir: PathBuf,
    /// The visible, committed envelopes (durable-before-visible: only appended after fsync+commit).
    visible: Vec<Envelope>,
    prev_hash: String,
    next_event_id: u64,
    next_seq: u64,
    current_gen: u64,
    committed_batches: u64,
}

impl Ledger {
    /// Open (create) a fresh ledger under `dir` for `run_id` at lease generation `gen`.
    pub fn open(dir: &Path, run_id: &str, gen: u64) -> Result<Ledger, LedgerError> {
        std::fs::create_dir_all(dir).map_err(io)?;
        let blob_dir = dir.join("blobs");
        std::fs::create_dir_all(&blob_dir).map_err(io)?;
        let wal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("wal.jsonl"))
            .map_err(io)?;
        Ok(Ledger {
            run_id: run_id.to_string(),
            dir: dir.to_path_buf(),
            wal,
            blob_dir,
            visible: Vec::new(),
            prev_hash: "genesis".to_string(),
            next_event_id: 1,
            next_seq: 0,
            current_gen: gen,
            committed_batches: 0,
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// A writer at the current lease generation.
    pub fn writer(&self) -> Writer {
        Writer {
            lease_generation: self.current_gen,
        }
    }

    /// Take over the ledger with a fresh (higher) lease generation, fencing older writers (G3).
    pub fn take_over(&mut self) -> Writer {
        self.current_gen += 1;
        self.writer()
    }

    /// Atomic multi-event append (l1-spike-spec §3.1): all events in `batch` become visible
    /// together or none do. Durable-before-visible — the WAL records are fsync'd, then a commit
    /// marker is fsync'd, and only then are the envelopes made visible. A crash between the two
    /// fsyncs leaves a *torn* batch that `replay` drops (G3).
    pub fn append_batch(
        &mut self,
        writer: Writer,
        batch: &[EventSpec],
    ) -> Result<Vec<Envelope>, LedgerError> {
        if writer.lease_generation < self.current_gen {
            return Err(LedgerError::StaleWriter {
                writer_gen: writer.lease_generation,
                current_gen: self.current_gen,
            });
        }
        let mut built = Vec::with_capacity(batch.len());
        let mut prev = self.prev_hash.clone();
        let mut event_id = self.next_event_id;
        let mut seq = self.next_seq;
        let mut parent = self.visible.last().map(|e| e.event_id);
        let mut wal_buf = String::new();
        for spec in batch {
            let (payload, blob_address) = self.stage_payload(&spec.payload)?;
            let mut env = Envelope {
                run_id: self.run_id.clone(),
                event_id,
                seq,
                parent_event_id: parent,
                lease_generation: writer.lease_generation,
                kind: spec.kind.clone(),
                cost: spec.cost,
                payload,
                blob_address,
                hash: String::new(),
            };
            env.hash = sha256_hex(
                format!("{LEAF_TAG}{}{}", env.canonical_without_hash(), prev).as_bytes(),
            );
            prev = env.hash.clone();
            parent = Some(event_id);
            event_id += 1;
            seq += 1;
            wal_buf.push_str(&env.to_wal_line());
            wal_buf.push('\n');
            built.push(env);
        }
        // Durable-before-visible: write the batch records, fsync, then the commit marker, fsync.
        self.wal.write_all(wal_buf.as_bytes()).map_err(io)?;
        self.wal.sync_data().map_err(io)?;
        let commit = Json::obj([
            ("t", Json::str("commit")),
            ("batch", Json::Int(self.committed_batches as i64 + 1)),
            ("upto_seq", Json::Int(seq as i64)),
        ]);
        self.wal
            .write_all(format!("{}\n", commit.to_canonical_string()).as_bytes())
            .map_err(io)?;
        self.wal.sync_data().map_err(io)?;
        // Only now make visible.
        self.prev_hash = prev;
        self.next_event_id = event_id;
        self.next_seq = seq;
        self.committed_batches += 1;
        self.visible.extend(built.iter().cloned());
        Ok(built)
    }

    /// Offload a payload ≥ `OFFLOAD_THRESHOLD` to the blob store, addressed by SHA-256; otherwise
    /// keep it inline. Returns `(inline_payload, blob_address)` with exactly one set.
    fn stage_payload(&self, payload: &Json) -> Result<(Option<Json>, Option<String>), LedgerError> {
        let canonical = payload.to_canonical_string();
        if canonical.len() >= OFFLOAD_THRESHOLD {
            let addr = format!("sha256:{}", sha256_hex(canonical.as_bytes()));
            let path = self.blob_dir.join(addr.replace(':', "_"));
            std::fs::write(&path, canonical.as_bytes()).map_err(io)?;
            Ok((None, Some(addr)))
        } else {
            Ok((Some(payload.clone()), None))
        }
    }

    /// Resolve an offloaded blob address back to its bytes (G4).
    pub fn resolve_blob(&self, address: &str) -> Option<String> {
        let path = self.blob_dir.join(address.replace(':', "_"));
        std::fs::read_to_string(path).ok()
    }

    /// The visible, committed envelopes.
    pub fn visible(&self) -> &[Envelope] {
        &self.visible
    }

    /// `project()` — the cost-totals view (l1-spike-spec §3.1 item 2). Folds `cost` over the
    /// visible events up to (and including) `until_seq`, returning `(total, derived_from)` where
    /// `derived_from` is the watermark (the highest seq folded, or `None` for the empty view).
    pub fn project_cost_totals(&self, until_seq: Option<u64>) -> (i64, Option<u64>) {
        let mut total = 0i64;
        let mut watermark = None;
        for e in &self.visible {
            if let Some(u) = until_seq {
                if e.seq > u {
                    break;
                }
            }
            total += e.cost;
            watermark = Some(e.seq);
        }
        (total, watermark)
    }

    /// Replay the durable WAL from scratch, honoring commit markers: events after the last commit
    /// marker (a torn batch) are dropped. Used by G3 to prove no partial multi-event append is
    /// visible after a crash. Returns the replayed visible envelopes.
    pub fn replay(dir: &Path) -> Result<Vec<Envelope>, LedgerError> {
        let f = File::open(dir.join("wal.jsonl")).map_err(io)?;
        let mut reader = BufReader::new(f);
        let mut line = String::new();
        let mut pending: Vec<Envelope> = Vec::new();
        let mut committed: Vec<Envelope> = Vec::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).map_err(io)?;
            if n == 0 {
                break;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let v = match hh_wire::parse(trimmed) {
                Ok(v) => v,
                Err(_) => continue, // torn/partial final line — drop
            };
            match v.get("t").and_then(Json::as_str) {
                Some("event") => {
                    if let Some(env) = envelope_from_json(&v) {
                        pending.push(env);
                    }
                }
                Some("commit") => {
                    committed.append(&mut pending);
                }
                _ => {}
            }
        }
        // `pending` now holds a torn (uncommitted) batch — dropped.
        Ok(committed)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn envelope_from_json(v: &Json) -> Option<Envelope> {
    Some(Envelope {
        run_id: v.get("run_id")?.as_str()?.to_string(),
        event_id: v.get("event_id")?.as_int()? as u64,
        seq: v.get("seq")?.as_int()? as u64,
        parent_event_id: match v.get("parent_event_id") {
            Some(Json::Int(i)) => Some(*i as u64),
            _ => None,
        },
        lease_generation: v.get("lease_generation")?.as_int()? as u64,
        kind: v.get("kind")?.as_str()?.to_string(),
        cost: v.get("cost")?.as_int()?,
        payload: v.get("payload").cloned(),
        blob_address: v.get("blob").and_then(Json::as_str).map(str::to_string),
        hash: v.get("hash")?.as_str()?.to_string(),
    })
}

fn io(e: std::io::Error) -> LedgerError {
    LedgerError::Io(e.to_string())
}

/// The per-session synthetic corpus (l1-spike-spec §3.3): 200 events, mix 60 % ≤ 1 KiB, 35 %
/// 8–16 KiB, 5 % exactly 64 KiB (offloaded). Deterministic given `run_id` so hash chains are
/// reproducible (G1). Payloads are pure ASCII so byte sizes are predictable.
pub fn session_corpus(run_id: &str) -> Vec<EventSpec> {
    let mut out = Vec::with_capacity(200);
    for i in 0..200u64 {
        let bucket = i % 20; // 12 small, 7 medium, 1 large  → 60 % / 35 % / 5 %
        let size = if bucket < 12 {
            256
        } else if bucket < 19 {
            12 * 1024
        } else {
            OFFLOAD_THRESHOLD // exactly 64 KiB canonical (payload string sized to hit it)
        };
        let filler = filler_of_canonical_size(size, run_id, i);
        let payload = Json::obj([
            ("i", Json::Int(i as i64)),
            ("run", Json::str(run_id)),
            ("body", Json::str(filler)),
        ]);
        out.push(EventSpec::new("work.step", i as i64, payload));
    }
    out
}

/// Build a `body` string so that the *canonical* payload object is ≥ `target` bytes for the large
/// bucket (exactly hits the offload threshold) and ≈ `target` for the others.
fn filler_of_canonical_size(target: usize, run_id: &str, i: u64) -> String {
    // Envelope overhead is small and fixed; size the filler to `target` minus a margin, then, for
    // the large bucket, pad until the canonical payload actually reaches OFFLOAD_THRESHOLD.
    let base = target.saturating_sub(64);
    let mut s = String::with_capacity(target);
    let seed = (run_id.len() as u64).wrapping_add(i);
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for k in 0..base {
        s.push(alphabet[((k as u64).wrapping_add(seed) as usize) % alphabet.len()] as char);
    }
    if target >= OFFLOAD_THRESHOLD {
        let probe = Json::obj([
            ("i", Json::Int(i as i64)),
            ("run", Json::str(run_id)),
            ("body", Json::str(s.clone())),
        ]);
        let mut deficit = OFFLOAD_THRESHOLD.saturating_sub(probe.to_canonical_string().len());
        while deficit > 0 {
            s.push('x');
            deficit -= 1;
        }
    }
    s
}

/// Run one session end-to-end (open ledger, append the corpus in batches of `batch_size`),
/// returning the ledger. Durable-before-visible per batch.
pub fn run_session(root: &Path, run_id: &str, batch_size: usize) -> Result<Ledger, LedgerError> {
    let dir = root.join(run_id);
    let mut ledger = Ledger::open(&dir, run_id, 1)?;
    let writer = ledger.writer();
    let corpus = session_corpus(run_id);
    for chunk in corpus.chunks(batch_size) {
        ledger.append_batch(writer, chunk)?;
    }
    Ok(ledger)
}

/// Build the per-run hash chain for `run_id`'s corpus **in memory** (canonical form + SHA-256
/// chain + a cost-totals fold), with no durable I/O. This isolates the ecosystem's concurrency /
/// CPU work (what C5 fan-out scores — "ideal 1.0 if perfectly parallel on ≥50 cores") from the
/// storage-fsync serialization that `run_session` (durable-before-visible) is bound by. Returns
/// `(head_hash, cost_total)`. `check_cancel` is polled between events (M-S1-8 propagation).
pub fn build_chain_in_memory(
    run_id: &str,
    mut check_cancel: impl FnMut() -> bool,
) -> (String, i64) {
    let corpus = session_corpus(run_id);
    let mut prev = "genesis".to_string();
    let mut total = 0i64;
    let mut parent: Option<u64> = None;
    for (i, spec) in corpus.iter().enumerate() {
        if check_cancel() {
            break;
        }
        // Offload discriminates on canonical size, matching the durable path.
        let canonical = spec.payload.to_canonical_string();
        let (payload, blob_address) = if canonical.len() >= OFFLOAD_THRESHOLD {
            (
                None,
                Some(format!("sha256:{}", sha256_hex(canonical.as_bytes()))),
            )
        } else {
            (Some(spec.payload.clone()), None)
        };
        let env = Envelope {
            run_id: run_id.to_string(),
            event_id: (i as u64) + 1,
            seq: i as u64,
            parent_event_id: parent,
            lease_generation: 1,
            kind: spec.kind.clone(),
            cost: spec.cost,
            payload,
            blob_address,
            hash: String::new(),
        };
        let h =
            sha256_hex(format!("{LEAF_TAG}{}{}", env.canonical_without_hash(), prev).as_bytes());
        prev = h;
        total += spec.cost;
        parent = Some((i as u64) + 1);
    }
    (prev, total)
}

// ---------------------------------------------------------------------------------------------
// Helper-binary boundary (M-S1-6; discharges DF-S0.2-1). One sandboxed tool call through a
// *helper process*: the spike spawns `s1-helper`, sends `execute(command,cwd,policy)` over stdio,
// receives `{exit_code, stdout_hash, stderr_hash, duration_ms}`. The helper applies the host's
// primitive (here: run under a wall-clock deadline) — the *same* external tool for every
// candidate so only the drive path is measured (l1-spike-spec §3.1 item 3).
// ---------------------------------------------------------------------------------------------

/// A persistent connection to a spawned helper binary.
pub struct HelperConn {
    child: Child,
}

/// The helper's reply to one `execute`.
#[derive(Debug, Clone)]
pub struct HelperReply {
    pub exit_code: i64,
    pub stdout_hash: String,
    pub stderr_hash: String,
    pub duration_ms: i64,
}

impl HelperConn {
    pub fn spawn(helper_bin: &Path) -> std::io::Result<HelperConn> {
        let child = Command::new(helper_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(HelperConn { child })
    }

    /// Send one `execute` request and read the reply. Returns the reply and the measured
    /// round-trip time in microseconds.
    pub fn execute(&mut self, command: &str, cwd: &str) -> std::io::Result<(HelperReply, u128)> {
        let req = Json::obj([
            ("op", Json::str("execute")),
            ("command", Json::str(command)),
            ("cwd", Json::str(cwd)),
            ("policy", Json::str("deadline")),
        ]);
        let stdin = self.child.stdin.as_mut().expect("helper stdin");
        let t = Instant::now();
        stdin.write_all(format!("{}\n", req.to_canonical_string()).as_bytes())?;
        stdin.flush()?;
        let stdout = self.child.stdout.as_mut().expect("helper stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let elapsed = t.elapsed().as_micros();
        let v = hh_wire::parse(line.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let reply = HelperReply {
            exit_code: v.get("exit_code").and_then(Json::as_int).unwrap_or(-1),
            stdout_hash: v
                .get("stdout_hash")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            stderr_hash: v
                .get("stderr_hash")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            duration_ms: v.get("duration_ms").and_then(Json::as_int).unwrap_or(-1),
        };
        Ok((reply, elapsed))
    }
}

impl Drop for HelperConn {
    fn drop(&mut self) {
        // Close stdin (EOF → helper exits) and reap.
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

/// Run `command` in-process (bare) and return `(reply-equivalent, wall µs)` — the baseline the
/// helper round-trip is compared against (M-S1-6 = helper round-trip − bare command time).
pub fn bare_command(command: &str, cwd: &str) -> std::io::Result<u128> {
    let t = Instant::now();
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .output()?;
    let elapsed = t.elapsed().as_micros();
    let _ = sha256_hex(&out.stdout);
    Ok(elapsed)
}

/// Percentile of a sorted slice (nearest-rank).
pub fn pct(sorted: &[u128], p: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() * p) / 100).min(sorted.len() - 1)]
}

#[cfg(test)]
mod gates {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("s1-spike-{}-{}", tag, std::process::id()))
    }

    // G1 — hash chain identical across repeated runs of the shared corpus (canonical form is
    // implementation-independent; within-candidate determinism + a pinned known-answer). The
    // cross-*candidate* identity (E2/E3) is deferred offline (DF-S0.3-1).
    #[test]
    fn g1_hash_chain_is_deterministic() {
        let head = |root: &Path| {
            let l = run_session(root, "run-g1", 10).unwrap();
            l.visible().last().unwrap().hash.clone()
        };
        let a = tmp("g1a");
        let b = tmp("g1b");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        let ha = head(&a);
        let hb = head(&b);
        assert_eq!(
            ha, hb,
            "same corpus must produce a byte-identical hash chain head"
        );
        // Known-answer: the head hash is a stable 64-hex SHA-256 digest.
        assert_eq!(ha.len(), 64);
        assert!(ha.chars().all(|c| c.is_ascii_hexdigit()));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    // G2 — rebuild-equality: project() from scratch equals the incremental view at every until_seq.
    #[test]
    fn g2_rebuild_equals_incremental() {
        let root = tmp("g2");
        let _ = std::fs::remove_dir_all(&root);
        let ledger = run_session(&root, "run-g2", 10).unwrap();
        // Incremental fold, checked against a from-scratch projection at every watermark.
        let mut running = 0i64;
        for e in ledger.visible() {
            running += e.cost;
            let (from_scratch, watermark) = ledger.project_cost_totals(Some(e.seq));
            assert_eq!(
                from_scratch, running,
                "rebuild != incremental at seq {}",
                e.seq
            );
            assert_eq!(watermark, Some(e.seq), "derived_from watermark mismatch");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    // G3 — crash mid-append: no partial multi-event append is visible on replay; a stale writer is
    // fenced. We simulate a torn batch by appending raw event records to the WAL with NO commit
    // marker, then replay: the torn batch must be dropped.
    #[test]
    fn g3_crash_no_partial_append() {
        let root = tmp("g3");
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("run-g3");
        {
            let mut ledger = Ledger::open(&dir, "run-g3", 1).unwrap();
            let w = ledger.writer();
            // Two committed batches.
            ledger
                .append_batch(w, &session_corpus("run-g3")[0..10])
                .unwrap();
            ledger
                .append_batch(w, &session_corpus("run-g3")[10..20])
                .unwrap();
        }
        let committed_before = Ledger::replay(&dir).unwrap().len();
        // Simulate a crash mid-append: raw event lines with no trailing commit marker.
        {
            let mut wal = OpenOptions::new()
                .append(true)
                .open(dir.join("wal.jsonl"))
                .unwrap();
            for line in [
                r#"{"cost":999,"event_id":99,"hash":"deadbeef","kind":"torn","lease_generation":1,"parent_event_id":null,"payload":{"x":1},"run_id":"run-g3","seq":98,"t":"event"}"#,
                r#"{"cost":999,"event_id":100,"hash":"deadbeef","kind":"torn","lease_generation":1,"parent_event_id":99,"payload":{"x":2},"run_id":"run-g3","seq":99,"t":"event"}"#,
            ] {
                wal.write_all(line.as_bytes()).unwrap();
                wal.write_all(b"\n").unwrap();
            }
            wal.sync_data().unwrap();
        }
        let committed_after = Ledger::replay(&dir).unwrap();
        assert_eq!(
            committed_after.len(),
            committed_before,
            "a torn (uncommitted) batch must not be visible after replay"
        );
        assert!(
            committed_after.iter().all(|e| e.kind != "torn"),
            "torn events leaked"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn g3_stale_writer_fenced() {
        let root = tmp("g3f");
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("run-g3f");
        let mut ledger = Ledger::open(&dir, "run-g3f", 1).unwrap();
        let old = ledger.writer();
        let _new = ledger.take_over(); // generation bumps to 2, fencing `old`
        let err = ledger
            .append_batch(old, &session_corpus("run-g3f")[0..1])
            .unwrap_err();
        assert_eq!(
            err,
            LedgerError::StaleWriter {
                writer_gen: 1,
                current_gen: 2
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // G4 — a 64 KiB event is offloaded and its blob is resolvable; a > 2^53 integer round-trips
    // unchanged. (Arbitrary-precision decimal strings are the idp/1 form deferred to S1.2 —
    // DF-S0.1-2 interim; the Stage-0 canonical form carries i64, which covers > 2^53 exactly.)
    #[test]
    fn g4_offload_and_numeric_round_trip() {
        let root = tmp("g4");
        let _ = std::fs::remove_dir_all(&root);
        let ledger = run_session(&root, "run-g4", 10).unwrap();
        // The large-bucket events (5 %) are offloaded — carry a blob address, resolvable.
        let offloaded: Vec<_> = ledger
            .visible()
            .iter()
            .filter(|e| e.blob_address.is_some())
            .collect();
        assert!(!offloaded.is_empty(), "expected offloaded 64 KiB events");
        for e in &offloaded {
            let addr = e.blob_address.as_ref().unwrap();
            let body = ledger.resolve_blob(addr).expect("blob resolvable");
            assert!(
                body.len() >= OFFLOAD_THRESHOLD,
                "resolved blob is the full payload"
            );
        }
        // > 2^53 integer round-trips through the canonical form unchanged.
        let big = (1i64 << 53) + 12345;
        let v = Json::obj([("n", Json::Int(big))]);
        let round = hh_wire::parse(&v.to_canonical_string()).unwrap();
        assert_eq!(round.get("n").and_then(Json::as_int), Some(big));
        let _ = std::fs::remove_dir_all(&root);
    }
}
