//! `lab/coordination-topology-v1` — the R-2.6.5 topology arm (§5e.5;
//! ADR-0193 D6; AC-R-2.6.5-7).
//!
//! The four-preset `topology` factor × `merge_policy` ×
//! `default_isolation` under `MatchSpec{matched_total}` — child spend is
//! `charged_to = subject` (the arm's `usage` fold is [`fold_arm_usage`]'s,
//! never a new accounting dimension), and every "multi-agent helps"
//! statement reports through the conditional `budget_match` evidence
//! row the `ComparisonReport` consumes. The arm's merge instrumentation
//! reads the parent's durable `control.merge.completed` rows —
//! `lost_write_count`, `conflicts_open_at_completion`, `absent_children`
//! are folded facts, never remembered counters.

use std::collections::BTreeMap;

use hh_subagent::types::{MergePolicy, SpawnError};
use hh_wire::json::Json;

use crate::lab::{match_report, ArmExecution, LabRefusal};
use crate::topology::TopologyPreset;

/// The `topology` factor — `single_agent | orchestrator_worker_isolated |
/// orchestrator_worker_share | peer_messaging` (closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoordinationTopology {
    /// No delegation — the baseline arm (T0).
    SingleAgent,
    /// Orchestrator+workers under isolated environments (`fork_snapshot`
    /// or `scoped_subtree` — the arm's `default_isolation` factor names
    /// which; T1 underneath).
    OrchestratorWorkerIsolated,
    /// Orchestrator+workers under `share` — the `resource(key)` lease and
    /// merge machinery are what this arm exercises (T1 underneath).
    OrchestratorWorkerShare,
    /// Orchestrator+workers with `sibling` messaging enabled (T1 with the
    /// messaging gate open — relayed through the parent, M-2).
    PeerMessaging,
}

impl CoordinationTopology {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CoordinationTopology::SingleAgent => "single_agent",
            CoordinationTopology::OrchestratorWorkerIsolated => "orchestrator_worker_isolated",
            CoordinationTopology::OrchestratorWorkerShare => "orchestrator_worker_share",
            CoordinationTopology::PeerMessaging => "peer_messaging",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CoordinationTopology> {
        Some(match s {
            "single_agent" => CoordinationTopology::SingleAgent,
            "orchestrator_worker_isolated" => CoordinationTopology::OrchestratorWorkerIsolated,
            "orchestrator_worker_share" => CoordinationTopology::OrchestratorWorkerShare,
            "peer_messaging" => CoordinationTopology::PeerMessaging,
            _ => return None,
        })
    }

    /// The underlying catalogue preset (`fan_out` the caller declares).
    pub fn preset(self, fan_out: u32) -> TopologyPreset {
        match self {
            CoordinationTopology::SingleAgent => TopologyPreset::T0Single,
            _ => TopologyPreset::T1OrchestratorWorker { fan_out },
        }
    }

    /// The `MessagingPolicy` member the topology requires — sibling
    /// traffic is admitted only on the `peer_messaging` arm (OQ-428's
    /// default `false` holds elsewhere; the caller stamps this on each
    /// stage spec, never at runtime).
    pub fn requires_sibling_messaging(self) -> bool {
        matches!(self, CoordinationTopology::PeerMessaging)
    }
}

/// The component-level factors one arm binds (`{topology, merge_policy,
/// default_isolation}` — §5e.5; the model × topology interaction is the
/// experiment's `contrast`, not this row's).
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationFactors {
    /// The topology preset.
    pub topology: CoordinationTopology,
    /// The merge policy the arm's merge runs under.
    pub merge_policy: MergePolicy,
    /// The `default_isolation` the stage specs carry (`share` /
    /// `scoped_subtree` / `fork_snapshot`).
    pub default_isolation: String,
}

impl CoordinationFactors {
    /// The canonical JSON (the arm row's `factors` member).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("topology", Json::str(self.topology.as_str())),
            ("merge_policy", Json::str(self.merge_policy.as_str())),
            ("default_isolation", Json::str(&self.default_isolation)),
        ])
    }
}

/// One arm's `coordination-topology-v1` record — the
/// `instrument-grade` reporting row (§5e.5: success,
/// `lost_write_count`, `conflicts_open_at_completion`,
/// `absent_children`, usage `charged_to = subject`).
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationArmReport {
    /// The arm id (`arm:{topology}:{merge_policy}:{isolation}:{model}` —
    /// the caller names it).
    pub arm_id: String,
    /// The bound factors.
    pub factors: CoordinationFactors,
    /// Every child terminal was `result` (success — the arm's own
    /// terminal profile is G-5's, not a claimed completion).
    pub success: bool,
    /// `coordination.lost_write_count` — veto metric (must be 0).
    pub lost_write_count: u64,
    /// Conflicts still open at completion (the merge report's
    /// `conflicts[]` minus resolved rows).
    pub conflicts_open_at_completion: u64,
    /// `absent[]` children at the merge.
    pub absent_children: u64,
    /// The arm usage (subject-charged; matched_total's fold).
    pub usage: Json,
}

impl CoordinationArmReport {
    /// The canonical JSON arm row.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("arm_id", Json::str(&self.arm_id)),
            ("factors", self.factors.to_json()),
            ("success", Json::Bool(self.success)),
            ("lost_write_count", Json::Int(self.lost_write_count as i64)),
            (
                "conflicts_open_at_completion",
                Json::Int(self.conflicts_open_at_completion as i64),
            ),
            ("absent_children", Json::Int(self.absent_children as i64)),
            ("usage", self.usage.clone()),
            ("charged_to", Json::str("subject")),
        ])
    }
}

/// `coordination_arm(store, parent_run_id, execution, factors) → report`
/// — fold the executed arm into its reporting row: `success` is
/// `all_terminal` (every child reached `result`), the merge counters read
/// the parent's durable `control.merge.completed`/`resolved` rows.
pub fn coordination_arm(
    store: &hh_ledger::store::Store,
    parent_run_id: &str,
    execution: &ArmExecution,
    factors: &CoordinationFactors,
) -> Result<CoordinationArmReport, SpawnError> {
    let events = store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let mut conflicts_open = 0u64;
    let mut absent = 0u64;
    let mut resolved_ids: std::collections::BTreeSet<String> = Default::default();
    for e in events {
        match e.class.as_str() {
            "control.merge.resolved" => {
                if let Some(id) = e.payload.get("conflict_id").and_then(Json::as_str) {
                    resolved_ids.insert(id.to_string());
                }
            }
            "control.merge.completed" => {
                absent += e
                    .payload
                    .get("absent_children")
                    .and_then(Json::as_int)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or_else(|| {
                        e.payload
                            .get("absent")
                            .and_then(|a| match a {
                                Json::Arr(xs) => Some(xs.len() as i64),
                                _ => None,
                            })
                            .unwrap_or(0)
                            .max(0) as u64
                    });
                // Conflicts fold through the blobbed report when present.
                if let Some(report_ref) = e.payload.get("merge_report_ref").and_then(Json::as_str) {
                    if let Ok(parsed) = hh_identity::idp::parse_id(report_ref) {
                        if let Ok(bytes) = store.get_blob(&hh_identity::idp::ContentAddress {
                            idp: "idp/1",
                            algorithm: "sha256",
                            digest: parsed.digest_hex,
                            media_type: "application/json".to_string(),
                            size: 0,
                        }) {
                            if let Ok(text) = String::from_utf8(bytes) {
                                if let Ok(report) = hh_wire::json::parse(&text) {
                                    if let Some(Json::Arr(cs)) = report.get("conflicts") {
                                        for c in cs {
                                            let open = c
                                                .get("conflict_id")
                                                .and_then(Json::as_str)
                                                .is_none_or(|id| !resolved_ids.contains(id));
                                            if open {
                                                conflicts_open += 1;
                                            }
                                        }
                                    }
                                    absent += report
                                        .get("absent")
                                        .and_then(|a| match a {
                                            Json::Arr(xs) => Some(xs.len() as i64),
                                            _ => None,
                                        })
                                        .unwrap_or(0)
                                        .max(0)
                                        as u64;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(CoordinationArmReport {
        arm_id: execution.arm_id.clone(),
        factors: factors.clone(),
        success: execution.all_terminal,
        lost_write_count: execution.lost_write_count,
        conflicts_open_at_completion: conflicts_open,
        absent_children: absent,
        usage: Json::Obj(
            execution
                .usage
                .amounts
                .iter()
                .map(|(d, v)| (d.as_str().to_string(), Json::Int(*v)))
                .collect::<BTreeMap<_, _>>(),
        ),
    })
}

/// `coordination_match_report(arms, specs)` — the conditional-report
/// gate for the topology arm (ADR-0193 D6): the [`match_report`]
/// matched-total fold (a `MissingMatchSpec`/`IncommensurableMatch`
/// refusal stands) plus the per-arm factor/membership rows and the
/// `kind: coordination_topology_v1` marker every "multi-agent helps"
/// claim cites.
pub fn coordination_match_report(
    arms: &[(
        &CoordinationArmReport,
        Option<&hh_budget::matchspec::MatchSpec>,
    )],
    executions: &BTreeMap<String, &ArmExecution>,
) -> Result<Json, LabRefusal> {
    // Re-run the matched gate over the underlying executions — the
    // refusal semantics are identical (an unmatchable arm refuses, never
    // silently compares).
    let exec_pairs: Vec<(&ArmExecution, Option<&hh_budget::matchspec::MatchSpec>)> = arms
        .iter()
        .filter_map(|(r, s)| executions.get(&r.arm_id).map(|e| (*e, *s)))
        .collect();
    let base = match_report(&exec_pairs)?;
    let mut rows = Vec::new();
    for (report, spec) in arms {
        let mut row = report.to_json();
        if let Json::Obj(m) = &mut row {
            if let Some(s) = spec {
                m.insert(
                    "match_mode".into(),
                    Json::str(match s.mode {
                        hh_budget::matchspec::MatchMode::MatchedTotal => "matched_total",
                        hh_budget::matchspec::MatchMode::MatchedCap => "matched_cap",
                        _ => "other",
                    }),
                );
            }
        }
        rows.push(row);
    }
    let mut out = base;
    if let Json::Obj(m) = &mut out {
        m.insert(
            "kind".into(),
            Json::str("coordination_topology_v1_arm_report"),
        );
        m.insert("coordination_arms".into(), Json::Arr(rows));
    }
    Ok(out)
}
