//! The closed `TopologyPreset` catalogue T0–T7 (§5e.3; ADR-0186 D2/D5 —
//! the catalogue is closed; recursive full-harness delegation is T3, not a
//! second contract). This slice implements T0 `single`, T1
//! `orchestrator_worker`, T2 `pipeline`, T6 `background_detached`; T3
//! (recursive), T4 (judge panel — ADR-0116's own stage), T5 (ensemble —
//! ADR-0123's), T7 (hosted peer — C2's) are declared and refuse
//! `ModeUnsupported`/their own slice's ticket.

use hh_env::handle::OnParentEnd;
use hh_subagent::types::{DelegationReason, MessagingPolicy, WaitMode};
use hh_wire::json::Json;

/// `TopologyPreset ∈ {T0..T7}` — the closed catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopologyPreset {
    /// `single` — no delegation (the lab's T0 arm; never a spawn).
    T0Single,
    /// `orchestrator_worker` — a fixed fan-out of role-`subagent`
    /// children (T1; `parallelism` reason).
    T1OrchestratorWorker {
        /// The declared fan-out (must be ≤ the parent's `fan_out` cap).
        fan_out: u32,
    },
    /// `pipeline` — ordered stages, each stage's result feeding the next
    /// stage's `supplies` (T2; `clean_context` reason per stage).
    T2Pipeline {
        /// The stage count (≥ 1).
        stages: u32,
    },
    /// `recursive` — full-harness delegation to depth (T3 — declared;
    /// the recursion bound is its own slice).
    T3Recursive {
        /// The recursion depth bound.
        depth: u32,
    },
    /// `judge_panel` — role-`judge` verifiers (T4 — ADR-0116's stage).
    T4JudgePanel,
    /// `ensemble` — sampled alternatives (T5 — ADR-0123's stage).
    T5Ensemble,
    /// `background_detached` — `wait.mode = background`,
    /// `on_parent_end = detach_to_child` (T6; `specialization` or the
    /// caller's reason; completion lands via the goal inbox).
    T6BackgroundDetached,
    /// `hosted_peer` — `process = hosted` children (T7 — C2's arm).
    T7HostedPeer,
}

/// The catalogue (closed — parsing an unknown preset is a data error,
/// never a default).
pub const TOPOLOGY_PRESETS: [(&str, TopologyPreset); 8] = [
    ("t0", TopologyPreset::T0Single),
    ("t1", TopologyPreset::T1OrchestratorWorker { fan_out: 0 }),
    ("t2", TopologyPreset::T2Pipeline { stages: 0 }),
    ("t3", TopologyPreset::T3Recursive { depth: 0 }),
    ("t4", TopologyPreset::T4JudgePanel),
    ("t5", TopologyPreset::T5Ensemble),
    ("t6", TopologyPreset::T6BackgroundDetached),
    ("t7", TopologyPreset::T7HostedPeer),
];

impl TopologyPreset {
    /// The canonical spelling (`Ref<TopologyPreset>` semantic id segment).
    pub fn as_str(&self) -> &'static str {
        match self {
            TopologyPreset::T0Single => "t0",
            TopologyPreset::T1OrchestratorWorker { .. } => "t1",
            TopologyPreset::T2Pipeline { .. } => "t2",
            TopologyPreset::T3Recursive { .. } => "t3",
            TopologyPreset::T4JudgePanel => "t4",
            TopologyPreset::T5Ensemble => "t5",
            TopologyPreset::T6BackgroundDetached => "t6",
            TopologyPreset::T7HostedPeer => "t7",
        }
    }

    /// Parse a canonical spelling (parameterless form — `t1` alone binds
    /// `fan_out = 0` as "caller declares").
    pub fn parse(s: &str) -> Option<TopologyPreset> {
        TOPOLOGY_PRESETS
            .iter()
            .find(|(n, _)| *n == s)
            .map(|(_, p)| *p)
    }

    /// Whether this slice executes the preset (T0/T1/T2/T6 — the
    /// ticket's admitted set).
    pub fn implemented(&self) -> bool {
        matches!(
            self,
            TopologyPreset::T0Single
                | TopologyPreset::T1OrchestratorWorker { .. }
                | TopologyPreset::T2Pipeline { .. }
                | TopologyPreset::T6BackgroundDetached
        )
    }

    /// The preset's declared `delegation_reason` (ADR-0186 D4 —
    /// per-topology, never inferred per-call).
    pub fn delegation_reason(&self) -> DelegationReason {
        match self {
            TopologyPreset::T0Single => DelegationReason::CleanContext,
            TopologyPreset::T1OrchestratorWorker { .. } => DelegationReason::Parallelism,
            TopologyPreset::T2Pipeline { .. } => DelegationReason::CleanContext,
            TopologyPreset::T3Recursive { .. } => DelegationReason::Specialization,
            TopologyPreset::T4JudgePanel => DelegationReason::IndependentCheck,
            TopologyPreset::T5Ensemble => DelegationReason::IndependentCheck,
            TopologyPreset::T6BackgroundDetached => DelegationReason::Specialization,
            TopologyPreset::T7HostedPeer => DelegationReason::Isolation,
        }
    }

    /// The `wait.mode` the preset's children spawn under.
    pub fn wait_mode(&self) -> WaitMode {
        match self {
            TopologyPreset::T6BackgroundDetached => WaitMode::Background,
            _ => WaitMode::Await,
        }
    }

    /// The `on_parent_end` the preset's children spawn under (T6's
    /// detach is the topology's defining arm).
    pub fn on_parent_end(&self) -> OnParentEnd {
        match self {
            TopologyPreset::T6BackgroundDetached => OnParentEnd::DetachToChild,
            _ => OnParentEnd::Teardown,
        }
    }

    /// The preset's declared `MessagingPolicy` (ADR-0191 M-2; OQ-428's
    /// defaults — `sibling`/`broadcast` false; T1 lifts `sibling` for its
    /// workers through the parent relay).
    pub fn messaging_policy(&self) -> MessagingPolicy {
        match self {
            TopologyPreset::T1OrchestratorWorker { .. } => MessagingPolicy {
                parent_to_child: true,
                child_to_parent: true,
                sibling: true,
                broadcast: false,
                max_pending: 16,
                coalesce: "none".into(),
            },
            _ => MessagingPolicy::default(),
        }
    }

    /// The canonical `Ref<TopologyPreset>` spelling the `spawned` row's
    /// `topology_ref` member carries.
    pub fn topology_ref(&self) -> String {
        let suffix = match self {
            TopologyPreset::T1OrchestratorWorker { fan_out } => format!("fan_out_{fan_out}"),
            TopologyPreset::T2Pipeline { stages } => format!("stages_{stages}"),
            TopologyPreset::T3Recursive { depth } => format!("depth_{depth}"),
            _ => self.as_str().to_string(),
        };
        format!("topology/{}", suffix)
    }
}

/// The orchestrator's wait policy over a plan's children (the C3 value —
/// the kernel only knows `wait.mode`; `{all, any, quorum(k)}` is the
/// orchestrator's reduction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitPolicy {
    /// Wait for every child's terminal.
    All,
    /// Wait for the first terminal.
    Any,
    /// Wait for `k` terminals.
    Quorum(u32),
}

impl WaitPolicy {
    /// Whether `completed`/`total` satisfies the policy.
    pub fn satisfied(&self, completed: usize, total: usize) -> bool {
        match self {
            WaitPolicy::All => completed >= total,
            WaitPolicy::Any => completed >= 1,
            WaitPolicy::Quorum(k) => completed >= *k as usize,
        }
    }

    /// The canonical spelling.
    pub fn to_json(&self) -> Json {
        match self {
            WaitPolicy::All => Json::str("all"),
            WaitPolicy::Any => Json::str("any"),
            WaitPolicy::Quorum(k) => Json::obj([("quorum", Json::Int(*k as i64))]),
        }
    }
}
