//! `diskwatch` — the runner's free-disk floor and layer-prune decisions
//! (manifest 266/R6.13; spec §3.9, REQ-XB-207). The recipe declares
//! `disk_floor_gib` (default 20); the runner stops admitting slots below
//! it and prunes *derived* harness layers after their task's slots so a
//! bounded VM disk never strangles a window. Pure decision functions: the
//! free-GiB probe lives on the VM agent side; this is the policy it calls.
//! No file-existence reads — decisions take measured inputs (REQ-XB-105).

/// The spec's default floor when a declaration names none (§3.9).
pub const DEFAULT_DISK_FLOOR_GIB: u64 = 20;

/// What admission control should do with the next slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Free disk is at or above the floor — admit.
    Admit,
    /// Free disk is below the declared floor — stop admitting.
    StopBelowFloor,
}

/// `admit` — a slot may start only while free disk ≥ `floor_gib`
/// (REQ-XB-207). The boundary is inclusive at the floor.
pub fn admit(free_gib: u64, floor_gib: u64) -> Admission {
    if free_gib >= floor_gib {
        Admission::Admit
    } else {
        Admission::StopBelowFloor
    }
}

/// One candidate for pruning — a podman layer on the VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// The layer's digest.
    pub digest: String,
    /// The task that derived it, if any (a harness layer built for a
    /// task's cells). Shared base layers carry `None`.
    pub task: Option<String>,
}

/// `prune_set` — the layers to remove once `settled_tasks` covers a task's
/// slots: exactly the derived harness layers of tasks whose slots have all
/// settled. Shared/base layers (`task: None`) are never pruned; layers of
/// tasks still in flight are kept (REQ-XB-207).
pub fn prune_set(layers: &[Layer], settled_tasks: &[&str]) -> Vec<String> {
    layers
        .iter()
        .filter(|l| {
            l.task
                .as_deref()
                .map(|t| settled_tasks.contains(&t))
                .unwrap_or(false)
        })
        .map(|l| l.digest.clone())
        .collect()
}
