//! `hh-orchestrator` — the C3/Stage-4 `orchestrator` component class
//! (spec §5e.3; ticket S4.6; ADR-0186).
//!
//! Binds the closed [`topology::TopologyPreset`] catalogue (T0 `single`,
//! T1 `orchestrator_worker`, T2 `pipeline`, T6 `background_detached`
//! implemented; T3/T4/T5/T7 declared and refused until their own slices),
//! executes admitted `delegate` decisions through `hh-subagent`'s one
//! `spawn` path, reduces wait `{all, any, quorum(k)}`, performs
//! `detach_to_child`, relays tree-local messages under the sealed
//! `MessagingPolicy`, and carries the mandatory typed
//! `delegation_reason` on every spawn (`owner = code` ⇒ a declared label;
//! `owner = model` ⇒ a `model_claim` at `delegate`).
//!
//! `lab/delegation-v1`'s first execution lives in [`lab`] — matched-budget
//! conditional reporting only (`MatchSpec` on every arm; an unmatchable
//! arm is a typed refusal, never an unverifiable claim).
//!
//! The C1 kernel slice (`hh-subagent`) never names this crate — C1
//! survives `removability(1)` without C3; C3 is independently removable
//! (nothing below depends on it).

pub mod lab;
pub mod orchestrator;
pub mod topology;

pub use orchestrator::{Orchestrator, ParentBinding, SpawnedShallow, TopologyRun};
pub use topology::{TopologyPreset, WaitPolicy, TOPOLOGY_PRESETS};
