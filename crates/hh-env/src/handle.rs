//! `EnvHandle` — the kernel-owned handle and the only runtime reachability path
//! (§5a.5 §2; ADR-0136 §5). The handle owns the `containment: PolicySlot`, the
//! attached `ContainmentReport`, the capability declaration, the meters and the
//! session; every operation on the environment is kernel-mediated through it.
//!
//! `HandleState` is the closed machine `declared → provisioning → ready ⇄
//! {detached, suspended} → unreachable → {reattached → ready | replaced |
//! failed} → torn_down`. `suspended` is in the sum but unreachable at Stage 1 —
//! `suspend()` is `Unsupported` (the `SuspendKind::Unknown` declaration is
//! honest, never coerced).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_containment::attach::PolicySlot;
use hh_containment::policy::{IsolationClass, ResourceLimits};
use hh_containment::report::{verify_report, ContainmentReport};

use crate::errors::EnvError;
use crate::record::{EnvironmentClass, ResolvedImage};

/// `Tri` — the tri-state capability member (T-LCD-07: `unknown` is a first-
/// class answer, never coerced to `supported`/`unsupported`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tri {
    /// The capability is declared supported.
    Supported,
    /// The capability is declared unsupported.
    Unsupported,
    /// The capability was never declared — honest `unknown`.
    Unknown,
}

impl Tri {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Tri::Supported => "supported",
            Tri::Unsupported => "unsupported",
            Tri::Unknown => "unknown",
        }
    }
}

/// `HandleState` (ADR-0136 §5) — the closed lifecycle machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandleState {
    /// Recorded, not yet provisioned.
    Declared,
    /// Provisioning in flight.
    Provisioning,
    /// Provisioned + attached + verified — the only state that dispatches.
    Ready,
    /// Detached (session released; the environment persists).
    Detached,
    /// Suspended — in the closed sum but unreachable at Stage 1.
    Suspended,
    /// Contact lost — kernel death is *not* environment death; `heal` decides
    /// `reattached`/`replaced`/`failed` (AC-R-2.2.5-2's "kernel restart").
    Unreachable,
    /// Reattached after `unreachable` — transitions back to `ready`.
    Reattached,
    /// Replaced (a successor handle owns the environment) — terminal.
    Replaced,
    /// Failed (unrecoverable) — terminal.
    Failed,
    /// Torn down — terminal.
    TornDown,
}

impl HandleState {
    /// The canonical spelling (`state` member of `action.environment.*`).
    pub fn as_str(self) -> &'static str {
        match self {
            HandleState::Declared => "declared",
            HandleState::Provisioning => "provisioning",
            HandleState::Ready => "ready",
            HandleState::Detached => "detached",
            HandleState::Suspended => "suspended",
            HandleState::Unreachable => "unreachable",
            HandleState::Reattached => "reattached",
            HandleState::Replaced => "replaced",
            HandleState::Failed => "failed",
            HandleState::TornDown => "torn_down",
        }
    }

    /// Terminal states — no further transitions.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            HandleState::Replaced | HandleState::Failed | HandleState::TornDown
        )
    }

    /// Whether `self → next` is a legal transition (the ADR-0136 §5 machine).
    /// `Suspended` is unreachable at Stage 1 — the `suspend()` op refuses
    /// `Unsupported` rather than entering it, so no `→ Suspended` edge exists.
    pub fn allows(self, next: HandleState) -> bool {
        use HandleState::*;
        matches!(
            (self, next),
            (Declared, Provisioning)
                | (Provisioning, Ready)
                | (Provisioning, Failed)
                | (Ready, Detached)
                | (Ready, Unreachable)
                | (Ready, TornDown)
                | (Detached, Ready)
                | (Detached, TornDown)
                | (Unreachable, Reattached)
                | (Unreachable, Replaced)
                | (Unreachable, Failed)
                | (Unreachable, TornDown)
                | (Reattached, Ready)
                | (Reattached, Failed)
                // terminal failure can be entered from any live state on an
                // unrecoverable fault (helper death with on_loss=fail_run).
                | (Declared, Failed)
                | (Ready, Failed)
                | (Detached, Failed)
                | (Reattached, TornDown)
                | (Declared, TornDown)
        )
    }
}

/// `SuspendKind` — what a suspend captures (`memory` | `fs_only` | `unknown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendKind {
    /// Full memory + fs.
    Memory,
    /// Filesystem only.
    FsOnly,
    /// Never declared — `suspend()` is `Unsupported`/`UnknownCapability`.
    Unknown,
}

/// `OutputOverflow` — the helper's declared overflow posture (`error` | `tail`
/// | `unknown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputOverflow {
    /// Refuse at the cap (`output_cap_exceeded`).
    Error,
    /// Keep the tail, mark truncation.
    Tail,
    /// Never declared.
    Unknown,
}

/// `Health` — the handle's self-reported liveness (never an authority source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Live.
    Healthy,
    /// Degraded (probe failures, throttled).
    Degraded,
    /// Unknown (no signal yet).
    Unknown,
}

/// `OnLoss` — what `heal` does when the environment is unrecoverable (Stage-1
/// default `fail_run`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLoss {
    /// The run fails (the Stage-1 default).
    FailRun,
    /// Replace from the last snapshot (Stage 2+).
    ReplaceFromSnapshot,
    /// Replace from the image (Stage 2+).
    ReplaceFromImage,
}

/// `SnapshotCadence` — when the driver auto-snapshots (Stage-1 default
/// `never`; the only supported kind is `path_baseline` on demand).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCadence {
    /// At every turn end.
    OnTurnEnd,
    /// Every N effects.
    EveryNEffects(u64),
    /// On idle.
    OnIdle,
    /// Never (the Stage-1 default).
    Never,
}

/// `DeriveMode` — how a child environment forks from a parent (Stage 2+; the
/// `ParentEdge` records it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeriveMode {
    /// A fresh environment from the image.
    FreshFromImage,
    /// A fork of a parent snapshot.
    ForkSnapshot,
    /// A shared view of the parent.
    Share,
    /// A scoped subtree.
    ScopedSubtree,
}

/// `OnParentEnd` — what a derived child does when its parent ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnParentEnd {
    /// Tear down with the parent.
    Teardown,
    /// Detach to a surviving parent.
    DetachToChild,
}

/// `EnvCapabilityDeclaration` — the capability member (T-LCD-07 — every member
/// is tri-state or a declared set, never coerced).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvCapabilityDeclaration {
    /// `snapshot` kinds — `SnapshotKind` name → `Tri`.
    pub snapshot: BTreeMap<String, Tri>,
    /// What `suspend()` would capture.
    pub suspend: SuspendKind,
    /// `restore_in_place`.
    pub restore_in_place: Tri,
    /// The `reattach_window_ms` the heal path honors (`None` = no window —
    /// the first loss is terminal).
    pub reattach_window_ms: Option<u64>,
    /// The helper's output-overflow posture.
    pub output_overflow: OutputOverflow,
    /// The per-class meter names the class declares beyond the three kernel
    /// clocks (e.g. `network.calls`).
    pub meters: BTreeSet<String>,
    /// Per-phase network policy.
    pub per_phase_network_policy: Tri,
    /// Declared residual channels (channels that outlive the effect —
    /// completeness is `partial`/`unknown` while any is open).
    pub residual_channels: Vec<String>,
}

impl EnvCapabilityDeclaration {
    /// The Stage-1 `local_sandboxed` declaration — `path_baseline` snapshots
    /// supported, `fs_tree`/`fs_layer`/`memory`/`shell_state` unknown (never
    /// coerced to unsupported); `suspend`/`restore_in_place` unknown; a
    /// reattach window for the heal path; `error` overflow.
    pub fn stage1_local_sandboxed() -> Self {
        let mut snapshot = BTreeMap::new();
        snapshot.insert("path_baseline".to_string(), Tri::Supported);
        for k in ["fs_tree", "fs_layer", "memory", "shell_state"] {
            snapshot.insert(k.to_string(), Tri::Unknown);
        }
        EnvCapabilityDeclaration {
            snapshot,
            suspend: SuspendKind::Unknown,
            restore_in_place: Tri::Unknown,
            reattach_window_ms: Some(30_000),
            output_overflow: OutputOverflow::Error,
            meters: ["network.calls", "helper.spawns"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            per_phase_network_policy: Tri::Unsupported,
            residual_channels: vec![],
        }
    }

    /// The Stage-1 `local_host` declaration — identical minus the sandbox
    /// meters (nothing is isolated; `none` is honest).
    pub fn stage1_local_host() -> Self {
        let mut c = Self::stage1_local_sandboxed();
        c.meters = ["helper.spawns"].iter().map(|s| s.to_string()).collect();
        c
    }

    /// Whether a `snapshot(kind)` is supported — `Ok` only on `Supported`;
    /// `Unsupported`/`Unknown` are distinct refusals (never coerced).
    pub fn snapshot_supported(&self, kind: crate::snapshot::SnapshotKind) -> Result<(), EnvError> {
        match self.snapshot.get(kind.as_str()) {
            Some(Tri::Supported) => Ok(()),
            Some(Tri::Unsupported) => Err(EnvError::Unsupported {
                capability: "snapshot",
                detail: format!("kind {} declared unsupported", kind.as_str()),
            }),
            Some(Tri::Unknown) | None => Err(EnvError::UnknownCapability {
                capability: format!("snapshot.{}", kind.as_str()),
            }),
        }
    }
}

/// `Roots` — the workspace roots the fs surface closes over (canonical,
/// symlink-resolved at bind).
#[derive(Debug, Clone, PartialEq)]
pub struct Roots {
    /// The readable workspace roots.
    pub workspace_roots: Vec<String>,
    /// The writable roots (fs_write closes over these).
    pub writable_roots: Vec<String>,
    /// The working directory (inside a workspace root).
    pub cwd: String,
}

impl Roots {
    /// Whether `path` (canonical) is inside a writable root — the R-NOSIDE
    /// guard for `fs_write`/`upload`/`apply_patch`.
    pub fn is_writable(&self, path: &str) -> bool {
        self.writable_roots
            .iter()
            .any(|r| path == r || path.starts_with(&format!("{r}/")))
    }

    /// Whether `path` is inside a readable root (`fs_read`/`fs_list`).
    pub fn is_readable(&self, path: &str) -> bool {
        self.workspace_roots
            .iter()
            .any(|r| path == r || path.starts_with(&format!("{r}/")))
            || self.is_writable(path)
    }
}

/// `EnvSession` — the live session record (`session_id`, the helper identity
/// claim, `attached_at`, the lease generation the session runs under).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSession {
    /// The session id (kernel-allocated).
    pub session_id: String,
    /// The helper identity — a *claim*, never an authority source.
    pub helper_identity: String,
    /// When the session attached.
    pub attached_at_ms: u64,
    /// The lease generation the session is bound to (a fenced lease ⇒ the
    /// session is dead — `verify` catches it).
    pub lease_generation: u64,
}

/// `MeterSample` — a per-class meter reading with its provenance (`measured` |
/// `estimated`; `unavailable` ⇒ the consumer renders `n/a{no_detector}`).
#[derive(Debug, Clone, PartialEq)]
pub struct MeterSample {
    /// The value.
    pub value: u64,
    /// How it was obtained.
    pub provenance: MeterProvenance,
}

/// `MeterProvenance` — `measured` | `estimated` (never silently conflated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterProvenance {
    /// Measured by the helper/kernel.
    Measured,
    /// Estimated (the class reports an estimate).
    Estimated,
}

/// `EnvMeters` — the three kernel-clock counters (`reserved_ms`,
/// `active_ms`, `suspended_ms`) plus declared per-class meters.
///
/// The clocks are kernel-side (never self-reported):
///
/// - `reserved_ms` — wall time since provisioning entry (entry → terminal);
///   it is `now − entry_ms`, always ≥ `active + suspended`.
/// - `active_ms` — accrued only while `state = ready` with a live session —
///   *excluding* a kernel-dead gap (AC-R-2.2.5-2: `active_ms` excludes the
///   window between `last_contact` and the `reattached` transition).
/// - `suspended_ms` — accrued while `state = suspended` (unreachable at
///   Stage 1 but the clock is honest).
#[derive(Debug, Clone)]
pub struct EnvMeters {
    /// Provisioning entry (the `reserved` base).
    entry_ms: u64,
    /// Accrued active milliseconds (committed at each transition out of a
    /// live `ready`).
    active_ms: u64,
    /// Accrued suspended milliseconds.
    suspended_ms: u64,
    /// `Some(t)` while the handle is `ready` with a live session (the active
    /// bucket started at `t`).
    active_since: Option<u64>,
    /// `Some(t)` while `suspended`.
    suspended_since: Option<u64>,
    /// The last kernel↔environment contact (the heal gap's start).
    last_contact_ms: u64,
    /// The declared per-class meters.
    pub extra: BTreeMap<String, MeterSample>,
}

/// A point-in-time read of the three clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetersView {
    /// `reserved_ms` — wall time since provisioning entry.
    pub reserved_ms: u64,
    /// `active_ms` — time in `ready`+live, excluding the kernel-dead gap.
    pub active_ms: u64,
    /// `suspended_ms` — time in `suspended`.
    pub suspended_ms: u64,
}

impl EnvMeters {
    /// A fresh meter at provisioning entry (`now`).
    pub fn new(now: u64) -> Self {
        EnvMeters {
            entry_ms: now,
            active_ms: 0,
            suspended_ms: 0,
            active_since: None,
            suspended_since: None,
            last_contact_ms: now,
            extra: BTreeMap::new(),
        }
    }

    /// Enter a live `ready` (start the active clock) at `now`.
    pub fn enter_active(&mut self, now: u64) {
        self.active_since = Some(now);
        self.last_contact_ms = now;
    }

    /// Leave `ready`/`active` — accrue up to `now`.
    pub fn leave_active(&mut self, now: u64) {
        if let Some(t) = self.active_since.take() {
            self.active_ms += now.saturating_sub(t);
        }
    }

    /// Enter `suspended`.
    pub fn enter_suspended(&mut self, now: u64) {
        self.suspended_since = Some(now);
    }

    /// Leave `suspended`.
    pub fn leave_suspended(&mut self, now: u64) {
        if let Some(t) = self.suspended_since.take() {
            self.suspended_ms += now.saturating_sub(t);
        }
    }

    /// Kernel↔environment contact — refresh `last_contact` (each op does).
    pub fn touch(&mut self, now: u64) {
        self.last_contact_ms = now;
    }

    /// The last contact time (the heal gap's start).
    pub fn last_contact(&self) -> u64 {
        self.last_contact_ms
    }

    /// `mark_unreachable(now)` — contact lost. The active clock stops at
    /// `last_contact` (the kernel-dead gap `[last_contact..now]` is excluded
    /// from `active_ms` — AC-R-2.2.5-2's honest-meter rule: the kernel was not
    /// there to observe the environment run).
    pub fn mark_unreachable(&mut self, _now: u64) {
        if let Some(t) = self.active_since.take() {
            self.active_ms += self.last_contact_ms.saturating_sub(t);
        }
        if let Some(t) = self.suspended_since.take() {
            self.suspended_ms += self.last_contact_ms.saturating_sub(t);
        }
    }

    /// `reattach(now)` — restart the active clock after a successful heal.
    pub fn reattach(&mut self, now: u64) {
        self.active_since = Some(now);
        self.last_contact_ms = now;
    }

    /// The three-clock view at `now`.
    pub fn view(&self, now: u64) -> MetersView {
        let active = self.active_ms
            + self
                .active_since
                .map(|t| now.saturating_sub(t))
                .unwrap_or(0);
        let suspended = self.suspended_ms
            + self
                .suspended_since
                .map(|t| now.saturating_sub(t))
                .unwrap_or(0);
        MetersView {
            reserved_ms: now.saturating_sub(self.entry_ms),
            active_ms: active,
            suspended_ms: suspended,
        }
    }

    /// The counter invariant — `reserved_ms ≥ active_ms + suspended_ms`
    /// (reserved covers the whole lifetime; active/suspended are disjoint
    /// sub-buckets of it).
    pub fn invariant_ok(&self, now: u64) -> bool {
        let v = self.view(now);
        v.reserved_ms >= v.active_ms + v.suspended_ms
    }
}

/// `ConnectionInfo` — how a tool reaches the environment (`shell_command` for
/// the local classes; `ports`/`live_url` for remote; `none` under R-NONET).
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionInfo {
    /// A shell command the helper runs (`local_host`/`local_sandboxed`).
    ShellCommand(String),
    /// A set of ports (remote).
    Ports(Vec<u16>),
    /// A live URL (provider-hosted).
    LiveUrl(String),
    /// No reachable surface (R-NONET — the fs surface is the only path).
    None,
}

/// `ParentEdge` — a derived child's link to its parent (`derive` lands at
/// Stage 2; the member is declared so a child's record is honest).
#[derive(Debug, Clone, PartialEq)]
pub struct ParentEdge {
    /// The parent's `env_handle_id`.
    pub env_handle_id: String,
    /// How the child was derived.
    pub mode: DeriveMode,
    /// What the child does when the parent ends.
    pub on_parent_end: OnParentEnd,
}

/// `EnvHandle` — the kernel-owned handle (§5a.5 §2). Constructed by
/// `EnvDriver::provision`; the only runtime reachability path (R-NOSIDE —
/// there is no `EnvHandle::open_arbitrary_channel`).
#[derive(Debug, Clone)]
pub struct EnvHandle {
    /// `env_handle_id` — kernel-allocated.
    pub env_handle_id: String,
    /// The owning run.
    pub run_id: String,
    /// The `environment_ref` — `(semantic_id, version_id)` of the record.
    pub environment_ref: (String, String),
    /// The class.
    pub class: EnvironmentClass,
    /// The lifecycle state.
    pub state: HandleState,
    /// The health claim.
    pub health: Health,
    /// The live session (`Some` only in `ready`).
    pub session: Option<EnvSession>,
    /// The capability declaration.
    pub capabilities: EnvCapabilityDeclaration,
    /// The `containment: PolicySlot` (DF-S1.12-3 — the deferred seam).
    pub containment: PolicySlot,
    /// The attached `ContainmentReport` (the honest `isolation_class` source).
    pub report: Option<ContainmentReport>,
    /// The credential bindings — **placeholder spellings only** (L7/SV-4: no
    /// value ever enters the handle).
    pub credential_bindings: Vec<String>,
    /// The workspace roots.
    pub roots: Roots,
    /// The declared resource bounds.
    pub limits: ResourceLimits,
    /// The budget nodes the environment draws on.
    pub budget_node_refs: Vec<String>,
    /// The kernel-clock meters.
    pub meters: EnvMeters,
    /// Snapshot refs taken.
    pub snapshots: Vec<String>,
    /// The parent edge, if derived.
    pub parent: Option<ParentEdge>,
    /// `on_loss`.
    pub on_loss: OnLoss,
    /// `snapshot_cadence`.
    pub snapshot_cadence: SnapshotCadence,
    /// How many times `heal` ran (the audit counter).
    pub heal_count: u64,
    /// The resolved image (recorded on `provisioned`).
    pub image: ResolvedImage,
    /// The `security.containment.applied` event id `attach` produced.
    pub applied_event_ref: Option<String>,
    /// Provisioning entry (ms).
    pub created_ms: u64,
}

impl EnvHandle {
    /// The *honest* `isolation_class` — the attached report's when present
    /// (the truth), else the class's implied claim (pre-attach; `local_host`'s
    /// `none` is honest because nothing is enforced).
    pub fn isolation_class(&self) -> IsolationClass {
        self.report
            .as_ref()
            .map(|r| r.isolation_class)
            .unwrap_or_else(|| self.class.implied_isolation())
    }

    /// `verify_environment` — the per-use revalidation (DF-S1.12-3's "verify
    /// on every use"). A pure check: `ready` + a live session + a present,
    /// non-stale report. Emits no event itself — the *explicit* `verify` op
    /// appends `action.environment.verified`; this is the gate every fs op and
    /// dispatch runs.
    pub fn verify_environment(&self) -> Result<(), EnvError> {
        if self.state != HandleState::Ready {
            return Err(EnvError::Unavailable {
                env_handle_id: self.env_handle_id.clone(),
                state: self.state.as_str(),
            });
        }
        if self.session.is_none() {
            return Err(EnvError::SessionLost {
                env_handle_id: self.env_handle_id.clone(),
            });
        }
        let report = self
            .report
            .as_ref()
            .ok_or_else(|| EnvError::ContainmentUnverified {
                field_group: "attach".to_string(),
                reason: "no_report".to_string(),
            })?;
        verify_report(report, &self.containment.policy().version_id).map_err(|_| {
            EnvError::ContainmentUnverified {
                field_group: "report".to_string(),
                reason: "stale".to_string(),
            }
        })?;
        Ok(())
    }

    /// Transition to `next` — refuses an illegal edge (`InvalidState`), and
    /// drives the meter buckets (`leave_active`/`enter_active`/`…`).
    pub fn transition(&mut self, next: HandleState, now: u64) -> Result<(), EnvError> {
        if !self.state.allows(next) {
            return Err(EnvError::InvalidState {
                op: "transition",
                state: self.state.as_str(),
            });
        }
        // Drive the clock buckets at the edge.
        match (self.state, next) {
            (HandleState::Ready, _) if self.session.is_some() => self.meters.leave_active(now),
            (_, HandleState::Ready) => self.meters.enter_active(now),
            (_, HandleState::Suspended) => self.meters.enter_suspended(now),
            (HandleState::Suspended, _) => self.meters.leave_suspended(now),
            _ => {}
        }
        self.state = next;
        Ok(())
    }
}
