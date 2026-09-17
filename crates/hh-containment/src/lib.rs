//! `hh-containment` — the C0/Stage-1 **containment floor** of spec §5g.4
//! (R-2.8.4; ADR-0060/0061/0062).
//!
//! Containment is the deterministic backstop beneath every probabilistic gate:
//! it bounds what processes inside an environment can read, write, execute,
//! reach on the network, become and consume, and it holds even when the
//! monitor, a classifier, a hook or a human is wrong.
//!
//! This crate lands the Stage-1 slice (the §5g.4 stage map):
//!
//! - **`ContainmentPolicy/1`** ([`policy`]) — the typed, provenance-bearing,
//!   content-addressed record: `{policy_id: semantic_id, version_id,
//!   provenance, fs, net, proc, resources, amendment, residual_channels[],
//!   ext}` with `KERNEL_PROTECTED` / `KERNEL_DENY` and the deny-by-default
//!   kernel policy. Identity runs through `hh_identity` (CC1) — `policy_id`
//!   is the semantic projection (`provenance`, `ext`, justification text and
//!   host spelling excluded — T-LCD-10); `version_id` the full canonical
//!   record. There is **one** schema (CC7): `to_json`/`from_json` here.
//! - **`effective(layers)`** ([`meet`]) — the authority-ordered layered meet:
//!   start from the kernel default; any layer may *narrow*; only the entitled
//!   classes may loosen (the Stage-1 entitlement table is the [`meet`]
//!   module doc); deny entries survive, booleans AND, allow sets
//!   intersect; unauthorized loosening ⇒ [`meet::ContainmentWidening`];
//!   exempting a `KERNEL_PROTECTED` name ⇒ [`meet::MeetError::ProtectedPathExemption`].
//! - **`admits(policy, p)`** ([`admit`]) — the floor input to `authorize`:
//!   `admitted | refused{reason} | amendable{diff}` over the proposal's
//!   canonical arguments. Containment never grants a permission; a `refused`
//!   never consults Π (AC-R-2.8.4-14).
//! - **`ContainmentReport` + evidence** ([`report`]) —
//!   `enforcement_evidence: map<FieldGroup, attested|reported|probed|unknown>`,
//!   `lowering_loss`, `probes`; `unknown` is never coerced (T-LCD-07).
//! - **The EP2 model backend** ([`backend::Ep2Model`], [`probes`]) — the
//!   Stage-1 in-process *model* of the process-sandbox boundary: a declared
//!   syscall gate (`gate(Syscall, policy)`), the kernel-run probe battery
//!   (write-outside-root / protected-metadata / symlink-escape fail;
//!   `connect`/`sendto`/raw/ICMP/name-resolution fail under `none`;
//!   `io_uring`/`ptrace`/`process_vm_*`/`setuid` denied; only the bridged
//!   `AF_UNIX` channel succeeds — AC-H4-03), and the `lowering_loss` listing.
//!   It is honest about what it is: a *model*, not a live sandbox — real
//!   EP2 enforcement lands with the environment record and helper protocol
//!   (S1.16 / R-2.2.5).
//! - **`attach`** ([`attach`]) — the handle-slot operation: apply + probe +
//!   report, fail-closed on helper-absent / unsupported-backend /
//!   unknown-evidence (I-C4); `warn_and_degrade` stamps
//!   `containment_degraded` and never applies to Lab runs.
//! - **Events** ([`events`]) — the `security.containment.{applied,violated,
//!   unverified}` payload builders (audit-grade, content-free; the class rows
//!   are registered in `hh-ledger`).
//!
//! Deliberately **absent** (the stage map puts them later — they are deferred
//! in `docs/tickets/DEFERRALS.md`): `decide_egress`/the mediator, `amend()`,
//! the credential-binding slot resolution, `security.egress.*`, approval
//! cache, `network.calls` metering, resources→budget mapping, phase
//! schedules, `user_space_kernel`/`microvm`/`external` attested backends,
//! `tls.terminate`/`inspect_hooks` enforcement, and the T-CON/AC-H4
//! executable battery.
//!
//! Boundary rules (same discipline as `hh-monitor`): every type is a
//! canonical record — no `Text` is *read* on any decision path (the record
//! carries `Text` justifications/statements as authored data; nothing here
//! interprets them), authority is conferred by `ProvenanceRecord` never read
//! from content, and `unknown` evidence is never coerced.

pub mod admit;
pub mod attach;
pub mod backend;
pub mod events;
pub mod meet;
pub mod paths;
pub mod policy;
pub mod probes;
pub mod report;

pub use admit::{
    admit_input, floor_gate, required_field_groups, unreachable_permissions, workspace_scope,
    AdmitInput, AdmitVerdict, ContainmentDiff, RefusedReason, UnreachableGrant,
};
pub use attach::{
    attach, relied_groups, AttachError, AttachInput, AttachMode, AttachOutcome, PolicySlot,
    SealWarning,
};
pub use backend::{
    BackendCaps, ContainmentBackend, Ep2Model, GateVerdict, Syscall, UnsupportedBackend,
};
pub use meet::{effective, ContainmentWidening, MeetError};
pub use policy::{kernel_default, ContainmentPolicy, PolicyError, KERNEL_DENY, KERNEL_PROTECTED};
pub use report::{ContainmentReport, EnforcementEvidence, FieldGroup, LossItem, ProbeResult};
