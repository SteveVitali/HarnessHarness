//! `hh-helper` — the out-of-process helper (S2.1; §5d.5 §4 helper row;
//! ADR-0050; R-2.5.5, R-2.2.5). The kernel launches one `hh-helper` binary
//! per contained environment and speaks `hh-helper/1` (this crate's
//! [`protocol`] module is the single schema source) over a bridged
//! `AF_UNIX` socket ([`wire`]).
//!
//! ```text
//!   kernel ──NDJSON/canonical-JSON──▶ hh-helper ──▶ seatbelt | podman
//!         exec/read/probe/cancel/          │
//!         terminate/detach/list_detached   └── exec journal (the durable
//!         env_apply/fs.*/snapshot/diff          store `read`/`probe`/
//!         probe_boundary/shutdown               `dedup` consult)
//! ```
//!
//! What the helper *is*: the executor-side half of the boundary — it
//! launches contained workloads, journals their output (`retained_bytes`
//! cap), enforces the deadline ladder itself (`term → kill` on the process
//! *group*, never the lone pid), surfaces detached groups, gates its own
//! `fs.*` verbs through the attached `ContainmentPolicy`
//! ([`crate::server`]'s `fs_op` reuses the admit classifiers — CC1), runs
//! the concrete probe battery, and holds the executor-side dedup window
//! plus `preserve_until` on kernel loss (R-2.5.5¹; `tier-c1`).
//!
//! What it is *not* (I-3): it never mints tokens (the kernel's attribution
//! token is *echoed* on every frame, never generated), never decides
//! authorization (a `commit_proof` it cannot recompute is `NotCommitted`),
//! never decides lifecycle outcomes, never holds secrets (SV-4 — env
//! projection is placeholders), and never invents success (every reply is
//! the verbatim observed result; a boundary gap is `unenforced`, not
//! `deny`).

pub mod exec;
pub mod fstree;
pub mod podman;
pub mod protocol;
pub mod seatbelt;
pub mod server;
pub mod wire;
