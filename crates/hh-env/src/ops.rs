//! The confined observer apparatus (R5.32; evidence spec §5.2 / D3-F3): an
//! **independently surviving supervisor**, a durable **marker sink**, and a
//! denied/read-only **sentinel** for the O1–O8 operational cells — plus the
//! declared Kp4 commit-before-entry evidence fold and the honest
//! probe/unknown outcome vocabulary.
//!
//! Read-only by definition: this module observes and folds; it never
//! appends to the run `Store`, never dispatches, and never authorizes a
//! retry or a settlement — `probe_authorization` is the one refusal
//! surface, and an external sink-zero read is *not* a probe. Kp4 proves
//! classification, not completion: `Kp4Outcome` retains `Unknown*`, never
//! `pass`; `defect_detected` is a distinct label from `pass`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use hharness_identity::idp::idp_id;
use hharness_wire::json::{parse, Json};

use crate::executor::ProbeVerdict;

fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()
}

fn read_rows(path: &Path) -> std::io::Result<Vec<Json>> {
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(parse(line).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ops record malformed at line {}", i + 1),
            )
        })?);
    }
    Ok(out)
}

// ── marker sink ──────────────────────────────────────────────────────────────

/// `TrialMarker` — the unique trial/effect marker: the cell trial, the
/// effect coordinate and the append ordinal (a defecting executor's second
/// append carries `ordinal = 1`); `marker_id` derives
/// `<trial>/<effect>/<ordinal>` — no invented dedup keys.
#[derive(Debug, Clone, PartialEq)]
pub struct TrialMarker {
    /// The cell/trial coordinate (fresh identity per repetition).
    pub trial_id: String,
    /// The dispatched effect this marker belongs to.
    pub effect_id: String,
    /// Append ordinal within the effect — an honest execution writes `0`.
    pub ordinal: u64,
    /// The marker's unique identity.
    pub marker_id: String,
}

impl TrialMarker {
    /// `new(trial_id, effect_id, ordinal)`.
    pub fn new(trial_id: &str, effect_id: &str, ordinal: u64) -> Self {
        TrialMarker {
            trial_id: trial_id.to_string(),
            effect_id: effect_id.to_string(),
            ordinal,
            marker_id: format!("{trial_id}/{effect_id}/{ordinal}"),
        }
    }

    /// The marker's content digest — O1's "matching observed digest"
    /// (`sha256:` over the canonical record bytes).
    pub fn digest(&self) -> String {
        idp_id(
            "ops_marker",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    fn to_json(&self) -> Json {
        Json::obj([
            ("effect_id", Json::str(&self.effect_id)),
            ("marker_id", Json::str(&self.marker_id)),
            ("ordinal", Json::Int(self.ordinal as i64)),
            ("trial_id", Json::str(&self.trial_id)),
        ])
    }

    fn from_json(j: &Json) -> Option<Self> {
        Some(TrialMarker {
            trial_id: j.get("trial_id")?.as_str()?.to_string(),
            effect_id: j.get("effect_id")?.as_str()?.to_string(),
            ordinal: j.get("ordinal")?.as_int()? as u64,
            marker_id: j.get("marker_id")?.as_str()?.to_string(),
        })
    }
}

/// `MarkerSink` — the durable append-only marker log (D3-F3's "real
/// sink"): the external world the runtime cannot roll back. `markers()`
/// re-reads the file — the world, never runtime memory.
pub struct MarkerSink {
    path: PathBuf,
}

impl MarkerSink {
    /// `open(dir)` — bind the sink at `dir/marker.log` (created on first
    /// append). One fresh sink per repetition.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(MarkerSink {
            path: dir.join("marker.log"),
        })
    }

    /// `append(marker)` — the fixed non-idempotent append: every call
    /// lands one durable line (flush + fsync). There is deliberately no
    /// `append_once` — deduplication here would hide the O8 defect.
    pub fn append(&mut self, marker: &TrialMarker) -> std::io::Result<()> {
        append_line(&self.path, &marker.to_json().to_canonical_string())
    }

    /// `markers()` — the world's own read: re-read and parse the log.
    pub fn markers(&self) -> std::io::Result<Vec<TrialMarker>> {
        read_rows(&self.path)?
            .iter()
            .map(|j| {
                TrialMarker::from_json(j).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "marker row")
                })
            })
            .collect()
    }

    /// `count_for(effect_id)` — durable markers the sink holds for one
    /// effect.
    pub fn count_for(&self, effect_id: &str) -> std::io::Result<usize> {
        Ok(self
            .markers()?
            .iter()
            .filter(|m| m.effect_id == effect_id)
            .count())
    }

    /// `duplicates()` — `effect_id → count` for every effect with more
    /// than one durable marker (the O8 detector surface).
    pub fn duplicates(&self) -> std::io::Result<BTreeMap<String, usize>> {
        let mut by_effect: BTreeMap<String, usize> = BTreeMap::new();
        for m in self.markers()? {
            *by_effect.entry(m.effect_id).or_insert(0) += 1;
        }
        Ok(by_effect.into_iter().filter(|(_, n)| *n > 1).collect())
    }

    /// `digest()` — the sink's content digest (`sha256:` of raw bytes); a
    /// missing file digests the empty content.
    pub fn digest(&self) -> String {
        let bytes = std::fs::read(&self.path).unwrap_or_default();
        idp_id("ops_sink", &bytes)
    }
}

// ── sentinel ─────────────────────────────────────────────────────────────────

/// `Sentinel` — the denied/read-only target: `intact()` re-reads and
/// digest-compares so a stray write is *detected*, never assumed absent.
pub struct Sentinel {
    path: PathBuf,
    expected: String,
}

impl Sentinel {
    /// `create(dir, contents)` — write `dir/sentinel.bin`, pin its digest,
    /// drop write permission (the digest is the detection, not the mode
    /// bit).
    pub fn create(dir: &Path, contents: &[u8]) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("sentinel.bin");
        std::fs::write(&path, contents)?;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        let _ = std::fs::set_permissions(&path, perms);
        Ok(Sentinel {
            path,
            expected: idp_id("ops_sentinel", contents),
        })
    }

    /// `intact()` — the current bytes still digest to the pinned value; a
    /// missing or mutated file reads `false`.
    pub fn intact(&self) -> bool {
        std::fs::read(&self.path)
            .map(|bytes| idp_id("ops_sentinel", &bytes) == self.expected)
            .unwrap_or(false)
    }
}

// ── observer ─────────────────────────────────────────────────────────────────

/// `ObservationPoint` — the three required observation intervals
/// (`before_launch` → `at_boundary` → `after_quiescence`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationPoint {
    /// Before the dispatch is launched.
    BeforeLaunch,
    /// At the executor-entry boundary.
    AtBoundary,
    /// After termination and quiescence.
    AfterQuiescence,
}

impl ObservationPoint {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservationPoint::BeforeLaunch => "before_launch",
            ObservationPoint::AtBoundary => "at_boundary",
            ObservationPoint::AfterQuiescence => "after_quiescence",
        }
    }
}

/// `ObservedEvent` — a durable supervisor note (executor entry, byte
/// count, termination). The instrumented executor emits these; the
/// observer only records.
#[derive(Debug, Clone, PartialEq)]
pub enum ObservedEvent {
    /// The executor was entered for `effect_id`.
    ExecutorEntry { effect_id: String },
    /// `count` bytes observed crossing the boundary.
    Bytes { count: u64 },
    /// The executor terminated for `effect_id` with `status`.
    Termination { effect_id: String, status: String },
}

impl ObservedEvent {
    fn to_json(&self) -> Json {
        let (note, extra): (&str, Vec<(&'static str, Json)>) = match self {
            ObservedEvent::ExecutorEntry { effect_id } => {
                ("executor_entry", vec![("effect_id", Json::str(effect_id))])
            }
            ObservedEvent::Bytes { count } => ("bytes", vec![("count", Json::Int(*count as i64))]),
            ObservedEvent::Termination { effect_id, status } => (
                "termination",
                vec![
                    ("effect_id", Json::str(effect_id)),
                    ("status", Json::str(status)),
                ],
            ),
        };
        let mut pairs = vec![("kind", Json::str("note")), ("note", Json::str(note))];
        pairs.extend(extra);
        Json::obj(pairs)
    }
}

/// `OpsObserver` — the independently surviving supervisor: its append-only
/// `observer.jsonl` lives on its own path, outside the run `Store`.
/// `open` on an existing directory retains every prior row — survival is a
/// file fact, not a process fact.
pub struct OpsObserver {
    path: PathBuf,
}

impl OpsObserver {
    /// `open(dir)` — bind (or re-bind) the supervisor at
    /// `dir/observer.jsonl`; re-opening never truncates.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(OpsObserver {
            path: dir.join("observer.jsonl"),
        })
    }

    /// `note(event)` — durably record one supervisor note.
    pub fn note(&mut self, event: &ObservedEvent) -> std::io::Result<()> {
        append_line(&self.path, &event.to_json().to_canonical_string())
    }

    /// `snapshot(point, sink, sentinel)` — durably record the boundary
    /// observation: sink marker count + content digest + sentinel intact,
    /// all re-read at call time.
    pub fn snapshot(
        &mut self,
        point: ObservationPoint,
        sink: &MarkerSink,
        sentinel: &Sentinel,
    ) -> std::io::Result<()> {
        let row = Json::obj([
            ("kind", Json::str("point")),
            ("point", Json::str(point.as_str())),
            ("sink_count", Json::Int(sink.markers()?.len() as i64)),
            ("sink_digest", Json::str(sink.digest())),
            ("sentinel_intact", Json::Bool(sentinel.intact())),
        ]);
        append_line(&self.path, &row.to_canonical_string())
    }

    /// `rows()` — replay the durable record (parsed, never skipped).
    pub fn rows(&self) -> std::io::Result<Vec<Json>> {
        read_rows(&self.path)
    }
}

// ── probe/verdict vocabulary ─────────────────────────────────────────────────

/// `ProbeSource` — where a probe claim came from. Only an instrument probe
/// (`ProbeVerdict` with admissible evidence) is admissible; an external
/// sink-zero read can confer neither settlement nor retry authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeSource {
    /// The executor's own `probe()` verdict (read-only instrument).
    Instrument,
    /// "The external world reads zero" — never an admissible probe.
    ExternalSinkZero,
}

/// `RecordedProbe` — the retained probe row (verdict + provenance;
/// external-zero is inadmissible by construction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedProbe {
    /// The probe verdict (`applied | not_applied | undeterminable`).
    pub verdict: ProbeVerdict,
    /// The claim's provenance.
    pub source: ProbeSource,
}

/// `ProbeAuthorization` — the apparatus's refusal surface: it records
/// whether admissible evidence exists for a *later owner's* decision; it
/// never grants retry/settle itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeAuthorization {
    /// An admissible instrument probe exists (`applied` ⇒ settle evidence;
    /// `not_applied` ⇒ retry-eligible evidence for the compensable row).
    Admissible(ProbeVerdict),
    /// No retry/settle may be authorized — `no_valid_probe`,
    /// `undeterminable`, or `external_zero`.
    Refused(&'static str),
}

/// `probe_authorization(probe)` — map a retained probe row to its
/// admissibility. `Refused` is terminal for the cell's settlement claim:
/// an external zero authorizes nothing.
pub fn probe_authorization(probe: Option<&RecordedProbe>) -> ProbeAuthorization {
    match probe {
        None => ProbeAuthorization::Refused("no_valid_probe"),
        Some(p) if p.source == ProbeSource::ExternalSinkZero => {
            ProbeAuthorization::Refused("external_zero")
        }
        Some(p) => match p.verdict {
            ProbeVerdict::Undeterminable => ProbeAuthorization::Refused("undeterminable"),
            v => ProbeAuthorization::Admissible(v),
        },
    }
}

// ── verdict folds ────────────────────────────────────────────────────────────

/// `TrialFailure` — the closed negative set (AC-2): every one of these
/// blocks the cell; none can be laundered into a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrialFailure {
    /// The supervisor record lacks a required interval or entry/
    /// termination evidence — existence alone never passes.
    MissingObserver,
    /// No marker landed for an effect whose outcome requires one.
    MissingMarker,
    /// Observer-recorded sink digest disagrees with the recomputed digest.
    DigestMismatch,
    /// The dispatch did not reach its observed outcome.
    Unobserved,
    /// The declared fault was armed but never reached.
    UnreachedFault,
    /// Non-zero boundary sink delta — mutation leaked past the injection.
    SinkDelta,
    /// The denied/read-only target was mutated.
    SentinelMutation,
    /// The resuming writer did not fence the crashed writer's lease.
    StaleFence,
    /// A repeated restoration added a commit or blind redispatch.
    DuplicateRestoration,
    /// The restore did not classify the effect `unknown{worker_lost}`.
    MissingUnknown,
    /// No probe obligation was scheduled for the unknown effect.
    MissingProbeObligation,
    /// The O8 detector missed a seeded duplicate.
    DetectorMiss,
}

impl TrialFailure {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TrialFailure::MissingObserver => "missing_observer",
            TrialFailure::MissingMarker => "missing_marker",
            TrialFailure::DigestMismatch => "digest_mismatch",
            TrialFailure::Unobserved => "unobserved",
            TrialFailure::UnreachedFault => "unreached_fault",
            TrialFailure::SinkDelta => "sink_delta",
            TrialFailure::SentinelMutation => "sentinel_mutation",
            TrialFailure::StaleFence => "stale_fence",
            TrialFailure::DuplicateRestoration => "duplicate_restoration",
            TrialFailure::MissingUnknown => "missing_unknown",
            TrialFailure::MissingProbeObligation => "missing_probe_obligation",
            TrialFailure::DetectorMiss => "detector_miss",
        }
    }
}

/// `CellVerdict` — the O-cell label set: `pass` and `defect_detected` are
/// distinct labels (O8's detection is detector sensitivity, never an
/// invariant pass).
#[derive(Debug, Clone, PartialEq)]
pub enum CellVerdict {
    /// The cell's expected outcome held with full evidence.
    Pass,
    /// The observer flagged a real defect (e.g. an O8 duplicate).
    DefectDetected { effect_id: String, count: usize },
    /// The cell cannot pass — the named negative held.
    Failed(TrialFailure),
}

/// `O1Cell` — the allowed-control fold's required evidence: the complete
/// supervisor record (all three intervals + entry and termination notes),
/// the `observed` outcome, the durable marker count for the effect, the
/// observer-recorded vs recomputed sink digests, and the sentinel bit.
#[derive(Debug)]
pub struct O1Cell {
    /// Complete supervisor record for the effect.
    pub observer_complete: bool,
    /// The dispatch reached `observed`.
    pub observed: bool,
    /// Durable markers the sink holds for the effect.
    pub marker_count: usize,
    /// The observer-recorded sink digest (at `after_quiescence`).
    pub recorded_digest: Option<String>,
    /// The world's recomputed sink digest.
    pub actual_digest: String,
    /// The sentinel's intact bit.
    pub sentinel_intact: bool,
}

/// `o1_outcome(cell)` — the O1 allowed-control fold.
pub fn o1_outcome(cell: &O1Cell) -> CellVerdict {
    if !cell.observer_complete {
        return CellVerdict::Failed(TrialFailure::MissingObserver);
    }
    if !cell.sentinel_intact {
        return CellVerdict::Failed(TrialFailure::SentinelMutation);
    }
    if !cell.observed {
        return CellVerdict::Failed(TrialFailure::Unobserved);
    }
    if cell.marker_count == 0 {
        return CellVerdict::Failed(TrialFailure::MissingMarker);
    }
    if cell.marker_count > 1 {
        return CellVerdict::DefectDetected {
            effect_id: String::new(),
            count: cell.marker_count,
        };
    }
    if cell.recorded_digest.as_deref() != Some(cell.actual_digest.as_str()) {
        return CellVerdict::Failed(TrialFailure::DigestMismatch);
    }
    CellVerdict::Pass
}

/// `O8Cell` — the detector-negative fold's required evidence.
#[derive(Debug)]
pub struct O8Cell {
    /// The supervisor record is present and complete.
    pub observer_complete: bool,
    /// The sentinel's intact bit.
    pub sentinel_intact: bool,
    /// `effect_id → count` the sink reports for `count > 1`
    /// (`MarkerSink::duplicates`).
    pub detected: Vec<(String, usize)>,
}

/// `o8_outcome(cell, effect_id)` — the O8 seeded-duplicate fold: the
/// expected outcome is `DefectDetected`; a miss is `DetectorMiss`, never a
/// pass.
pub fn o8_outcome(cell: &O8Cell, effect_id: &str) -> CellVerdict {
    if !cell.observer_complete {
        return CellVerdict::Failed(TrialFailure::MissingObserver);
    }
    if !cell.sentinel_intact {
        return CellVerdict::Failed(TrialFailure::SentinelMutation);
    }
    match cell.detected.iter().find(|(e, _)| e == effect_id) {
        Some((e, n)) => CellVerdict::DefectDetected {
            effect_id: e.clone(),
            count: *n,
        },
        None => CellVerdict::Failed(TrialFailure::DetectorMiss),
    }
}

/// `Kp4Cell` — the O2 fold's required evidence (commit → injected
/// termination before executor entry).
#[derive(Debug)]
pub struct Kp4Cell {
    /// The supervisor record carries the boundary observation.
    pub observer_present: bool,
    /// The armed `kp-4` injection actually fired (`FaultInjected{at:
    /// "kp-4"}` reached — an armed-but-unreached fault cannot pass).
    pub reached: bool,
    /// The observer-measured marker delta across the boundary (must be 0).
    pub sink_delta: i64,
    /// The sentinel's intact bit.
    pub sentinel_intact: bool,
    /// The resuming writer fenced the crashed writer's lease
    /// (`RestoreReport.fenced_lease_id` names the stale holder).
    pub new_fence: bool,
    /// The restore's `unknown{worker_lost}` effect set.
    pub unknowned: Vec<String>,
    /// A probe obligation (`control.retry.scheduled{kind: probe}`) was
    /// scheduled for the unknown effect.
    pub probe_scheduled: bool,
    /// Commits the *repeated* restoration added beyond the durable one.
    pub extra_commits: usize,
    /// Blind redispatches a repeated restoration triggered.
    pub extra_dispatches: usize,
    /// The retained instrument probe row, if one ran.
    pub probe: Option<RecordedProbe>,
}

/// `Kp4Outcome` — the O2 label set. Kp4 proves **classification, not
/// completion**: retained outcomes are `Unknown*`/`Resolved*`, never
/// `pass`; `Failed` names the violated negative.
#[derive(Debug, Clone, PartialEq)]
pub enum Kp4Outcome {
    /// `unknown{worker_lost}` retained; no admissible probe ran (an
    /// inadmissible external-zero claim counts as unprobed).
    UnknownUnprobed,
    /// `unknown{worker_lost}` retained; the instrument probe answered
    /// `undeterminable`.
    UnknownUndeterminable,
    /// The probe resolved the effect applied — classification resolved.
    ResolvedApplied,
    /// The probe resolved `not_applied` — retry-eligible evidence for the
    /// compensable row (the apparatus itself never redispatches).
    ResolvedNotApplied,
    /// The cell cannot pass — the named negative held.
    Failed(TrialFailure),
}

/// `kp4_outcome(cell, effect_id)` — the O2 fold.
pub fn kp4_outcome(cell: &Kp4Cell, effect_id: &str) -> Kp4Outcome {
    if !cell.observer_present {
        return Kp4Outcome::Failed(TrialFailure::MissingObserver);
    }
    if !cell.reached {
        return Kp4Outcome::Failed(TrialFailure::UnreachedFault);
    }
    if cell.sink_delta != 0 {
        return Kp4Outcome::Failed(TrialFailure::SinkDelta);
    }
    if !cell.sentinel_intact {
        return Kp4Outcome::Failed(TrialFailure::SentinelMutation);
    }
    if !cell.new_fence {
        return Kp4Outcome::Failed(TrialFailure::StaleFence);
    }
    if cell.extra_commits > 0 || cell.extra_dispatches > 0 {
        return Kp4Outcome::Failed(TrialFailure::DuplicateRestoration);
    }
    if !cell.unknowned.iter().any(|e| e == effect_id) {
        return Kp4Outcome::Failed(TrialFailure::MissingUnknown);
    }
    if !cell.probe_scheduled {
        return Kp4Outcome::Failed(TrialFailure::MissingProbeObligation);
    }
    match cell.probe {
        None => Kp4Outcome::UnknownUnprobed,
        Some(p) if p.source == ProbeSource::ExternalSinkZero => Kp4Outcome::UnknownUnprobed,
        Some(p) => match p.verdict {
            ProbeVerdict::Applied => Kp4Outcome::ResolvedApplied,
            ProbeVerdict::NotApplied => Kp4Outcome::ResolvedNotApplied,
            ProbeVerdict::Undeterminable => Kp4Outcome::UnknownUndeterminable,
        },
    }
}

// ── O4/O5 — the permission wait/answer/resume cells ─────────────────────────
//
// The permission cells' evidence is the *interval*: the suspended effect
// must show no prepare/commit/executor entry and no sink mutation at two
// observed points while its `pending` is owed; only an offered, authority-
// checked answer may resolve it; and the wakeup/resume must land exactly
// one marker on the *same* effect. `input_required`/`resume_paused` is no
// substitute — that effect may already be committed.

/// `PermissionFailure` — the O4/O5 closed negative set. Every member
/// blocks the cell; none can be laundered into a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionFailure {
    /// The durable `security.permission.pending` is absent or its offered
    /// set was not preserved.
    MissingPending,
    /// A sink delta, executor entry, or `prepared`/`committed` row landed
    /// while the permission was owed — the wait was not a real wait.
    MutationDuringWait,
    /// Fewer than two clean observer intervals cover the wait.
    MissingWaitInterval,
    /// The restart/restore did not preserve the pending
    /// (`RestoreReport.pending_permissions` lacks the id).
    LostPending,
    /// `control.wakeup.occurred`/`fired` for `permission_decided` never
    /// landed — the resume was not wakeup-driven.
    MissingWakeup,
    /// No final `security.permission.decided` resolved the pending.
    MissingDecision,
    /// The resume produced anything but exactly one marker on the same
    /// effect (0 = never resumed; >1 = a second dispatch/mutation).
    SecondMutation,
    /// The marker/sink digest disagrees with the observed control path
    /// (the O1 comparator).
    DigestMismatch,
    /// An identical re-answer minted a second `decided`/grant — the
    /// response was not idempotent.
    NonIdempotentAnswer,
    /// A conflicting later answer produced a grant or a mutation.
    ConflictingGrant,
    /// An unoffered answer was admitted (decided/granted/mutated).
    UnofferedAdmitted,
    /// An answer from an endorser without authority was admitted.
    UnauthorizedAdmitted,
    /// The deny path left `prepared`/`committed`/entry or a sink delta.
    DenyMutated,
    /// The denied/read-only target was mutated.
    SentinelMutation,
}

impl PermissionFailure {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionFailure::MissingPending => "missing_pending",
            PermissionFailure::MutationDuringWait => "mutation_during_wait",
            PermissionFailure::MissingWaitInterval => "missing_wait_interval",
            PermissionFailure::LostPending => "lost_pending",
            PermissionFailure::MissingWakeup => "missing_wakeup",
            PermissionFailure::MissingDecision => "missing_decision",
            PermissionFailure::SecondMutation => "second_mutation",
            PermissionFailure::DigestMismatch => "digest_mismatch",
            PermissionFailure::NonIdempotentAnswer => "non_idempotent_answer",
            PermissionFailure::ConflictingGrant => "conflicting_grant",
            PermissionFailure::UnofferedAdmitted => "unoffered_admitted",
            PermissionFailure::UnauthorizedAdmitted => "unauthorized_admitted",
            PermissionFailure::DenyMutated => "deny_mutated",
            PermissionFailure::SentinelMutation => "sentinel_mutation",
        }
    }
}

/// `PermissionOutcome` — the O4/O5 label set: `pass` only when every
/// required interval and transition holds.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionOutcome {
    /// The cell's expected outcome held with full evidence.
    Pass,
    /// The cell cannot pass — the named negative held.
    Failed(PermissionFailure),
}

/// `PermissionWaitCell` — the O4 fold's required evidence
/// (pending→suspend→offered/authority-checked answer→wakeup/resume on the
/// same effect, one mutation; restart preserves the pending; the identical
/// answer is idempotent; a conflicting key refuses).
#[derive(Debug)]
pub struct PermissionWaitCell {
    /// The durable `security.permission.pending` landed (and carried the
    /// offered set/cause coordinates).
    pub pending_durable: bool,
    /// The two observed no-mutation intervals held (zero sink delta and
    /// no prepare/commit/executor entry at each).
    pub wait_intervals_clean: u8,
    /// The restart/restore preserved the owed pending.
    pub restart_preserved: bool,
    /// `permission_decided` produced `occurred` then `fired`.
    pub wakeup_fired: bool,
    /// A final `decided` resolved the pending.
    pub decided_durable: bool,
    /// Durable markers the sink holds for the effect — must be exactly 1.
    pub marker_count: usize,
    /// The marker digest matches the O1-path observed digest.
    pub digest_matches: bool,
    /// An identical re-answer returned the recorded decision with no
    /// second `decided`/grant.
    pub idempotent_replay: bool,
    /// A conflicting new-key answer left no second grant/mutation.
    pub conflicting_refused: bool,
    /// The sentinel's intact bit.
    pub sentinel_intact: bool,
}

/// `o4_outcome(cell)` — the O4 permission-allow fold.
pub fn o4_outcome(cell: &PermissionWaitCell) -> PermissionOutcome {
    if !cell.pending_durable {
        return PermissionOutcome::Failed(PermissionFailure::MissingPending);
    }
    if cell.wait_intervals_clean < 2 {
        return PermissionOutcome::Failed(PermissionFailure::MissingWaitInterval);
    }
    if !cell.restart_preserved {
        return PermissionOutcome::Failed(PermissionFailure::LostPending);
    }
    if !cell.wakeup_fired {
        return PermissionOutcome::Failed(PermissionFailure::MissingWakeup);
    }
    if !cell.decided_durable {
        return PermissionOutcome::Failed(PermissionFailure::MissingDecision);
    }
    if cell.marker_count != 1 {
        return PermissionOutcome::Failed(PermissionFailure::SecondMutation);
    }
    if !cell.digest_matches {
        return PermissionOutcome::Failed(PermissionFailure::DigestMismatch);
    }
    if !cell.idempotent_replay {
        return PermissionOutcome::Failed(PermissionFailure::NonIdempotentAnswer);
    }
    if !cell.conflicting_refused {
        return PermissionOutcome::Failed(PermissionFailure::ConflictingGrant);
    }
    if !cell.sentinel_intact {
        return PermissionOutcome::Failed(PermissionFailure::SentinelMutation);
    }
    PermissionOutcome::Pass
}

/// `PermissionDenyCell` — the O5 fold's required evidence (pending→
/// offered deny→refusal; no prepare/commit/entry/mutation; a conflicting
/// later answer cannot authorize).
#[derive(Debug)]
pub struct PermissionDenyCell {
    /// The durable `security.permission.pending` landed.
    pub pending_durable: bool,
    /// A final `decided{deny}` resolved the pending.
    pub deny_decided: bool,
    /// The effect closed refused (the recorded deny served the resume).
    pub refused: bool,
    /// No `prepared`/`committed`/executor entry and a zero sink delta for
    /// the effect — deny cannot mutate.
    pub no_mutation: bool,
    /// A conflicting later answer produced no grant/mutation.
    pub conflicting_refused: bool,
    /// The sentinel's intact bit.
    pub sentinel_intact: bool,
}

/// `o5_outcome(cell)` — the O5 permission-deny fold.
pub fn o5_outcome(cell: &PermissionDenyCell) -> PermissionOutcome {
    if !cell.pending_durable {
        return PermissionOutcome::Failed(PermissionFailure::MissingPending);
    }
    if !cell.deny_decided {
        return PermissionOutcome::Failed(PermissionFailure::MissingDecision);
    }
    if !cell.no_mutation {
        return PermissionOutcome::Failed(PermissionFailure::DenyMutated);
    }
    if !cell.refused {
        return PermissionOutcome::Failed(PermissionFailure::MissingDecision);
    }
    if !cell.conflicting_refused {
        return PermissionOutcome::Failed(PermissionFailure::ConflictingGrant);
    }
    if !cell.sentinel_intact {
        return PermissionOutcome::Failed(PermissionFailure::SentinelMutation);
    }
    PermissionOutcome::Pass
}
