//! Fault and perturbation profiles (spec §5h.2 + §5h.4's F-class strata;
//! R-2.9.2 AC-8; S3.3).
//!
//! A `FaultProfile`/`PerturbationProfile` is an **environment-factor level**
//! (ADR-0142's family-F rows over suites A/D — a level on the environment
//! factor, never a new suite and never a metric). The profile is bound on the
//! run (`EvalRun::{fault_profile, perturbation_profile}`) and is one of the
//! fields `compare` requires identical across arms unless it is the varied
//! factor.
//!
//! The `FaultType` sum is the closed class list the Stage-3 profiles draw
//! from (HAL-class faults over coding/terminal + structured-tool planes).

use std::collections::BTreeMap;

use hh_wire::Json;

use crate::json_util::*;

/// The closed fault-class sum (the `faults[]` element kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FaultType {
    /// `tool_timeout` — a tool call exceeds its deadline.
    ToolTimeout,
    /// `tool_error_class` — a tool returns an injected error class.
    ToolErrorClass,
    /// `model_5xx` — a model-boundary failure class.
    Model5xx,
    /// `model_latency` — injected model latency.
    ModelLatency,
    /// `context_truncation` — the context cap forces truncation.
    ContextTruncation,
    /// `fs_corruption` — workspace bytes corrupted between calls.
    FsCorruption,
    /// `process_kill` — a child process is killed mid-flight.
    ProcessKill,
    /// `network_partition` — the agent-phase network partitions.
    NetworkPartition,
    /// `clock_skew` — the wall/working clock skews.
    ClockSkew,
    /// `disk_full` — writes hit `ENOSPC`.
    DiskFull,
    /// `egress_deny` — an allowlisted egress is denied mid-run.
    EgressDeny,
    /// `tool_response_drift` — a tool's response schema drifts.
    ToolResponseDrift,
    /// `interleaved_invalidation` — a dependency is invalidated mid-run.
    InterleavedInvalidation,
    /// `partial_write` — an effect's output is truncated/partial.
    PartialWrite,
}

impl FaultType {
    /// The full closed set.
    pub const ALL: [FaultType; 14] = [
        FaultType::ToolTimeout,
        FaultType::ToolErrorClass,
        FaultType::Model5xx,
        FaultType::ModelLatency,
        FaultType::ContextTruncation,
        FaultType::FsCorruption,
        FaultType::ProcessKill,
        FaultType::NetworkPartition,
        FaultType::ClockSkew,
        FaultType::DiskFull,
        FaultType::EgressDeny,
        FaultType::ToolResponseDrift,
        FaultType::InterleavedInvalidation,
        FaultType::PartialWrite,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FaultType::ToolTimeout => "tool_timeout",
            FaultType::ToolErrorClass => "tool_error_class",
            FaultType::Model5xx => "model_5xx",
            FaultType::ModelLatency => "model_latency",
            FaultType::ContextTruncation => "context_truncation",
            FaultType::FsCorruption => "fs_corruption",
            FaultType::ProcessKill => "process_kill",
            FaultType::NetworkPartition => "network_partition",
            FaultType::ClockSkew => "clock_skew",
            FaultType::DiskFull => "disk_full",
            FaultType::EgressDeny => "egress_deny",
            FaultType::ToolResponseDrift => "tool_response_drift",
            FaultType::InterleavedInvalidation => "interleaved_invalidation",
            FaultType::PartialWrite => "partial_write",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<FaultType> {
        FaultType::ALL.into_iter().find(|f| f.as_str() == s)
    }
}

/// `fault_profile/1` — a named fault level: `{profile_id, faults[{type,
/// rate_ppm, at}], provenance}`. Deterministic to evaluate — `rate_ppm` is
/// the declared injection probability (the *injection* is the environment's
/// job; the profile is data).
#[derive(Debug, Clone, PartialEq)]
pub struct FaultProfile {
    /// The profile's registry id.
    pub profile_id: String,
    /// The faults the level injects.
    pub faults: Vec<FaultSpec>,
    /// The profile's provenance ref.
    pub provenance_ref: Option<String>,
}

/// One fault entry — `{type, rate_ppm, at}` (`at` = the injection point
/// spelling: `effect_id`, `turn`, `phase` — opaque to the level record).
#[derive(Debug, Clone, PartialEq)]
pub struct FaultSpec {
    /// The fault class.
    pub fault_type: FaultType,
    /// The injection probability (ppm of 1.0; `1_000_000` = always).
    pub rate_ppm: i64,
    /// The injection point spelling.
    pub at: String,
}

/// `perturbation_profile/1` — a structural-perturbation level (the task's
/// *surface* varies, not the run's fault environment): `{profile_id,
/// perturbations[{kind, params}], provenance}`.
#[derive(Debug, Clone, PartialEq)]
pub struct PerturbationProfile {
    /// The profile's registry id.
    pub profile_id: String,
    /// The perturbations the level applies.
    pub perturbations: Vec<PerturbationSpec>,
    /// The profile's provenance ref.
    pub provenance_ref: Option<String>,
}

/// One perturbation entry — `{kind, params}` (`kind` spellings:
/// `rename_variable`, `reorder_tools`, `paraphrase_instructions`,
/// `distractor_files`, `relocate_paths`; the closed Stage-3 set).
#[derive(Debug, Clone, PartialEq)]
pub struct PerturbationSpec {
    /// The perturbation kind.
    pub kind: PerturbationKind,
    /// The kind's parameters (opaque record — the level's own data).
    pub params: Json,
}

/// The closed perturbation-kind set (Stage-3 F-class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerturbationKind {
    /// `rename_variable`.
    RenameVariable,
    /// `reorder_tools`.
    ReorderTools,
    /// `paraphrase_instructions`.
    ParaphraseInstructions,
    /// `distractor_files`.
    DistractorFiles,
    /// `relocate_paths`.
    RelocatePaths,
}

impl PerturbationKind {
    /// The full closed set.
    pub const ALL: [PerturbationKind; 5] = [
        PerturbationKind::RenameVariable,
        PerturbationKind::ReorderTools,
        PerturbationKind::ParaphraseInstructions,
        PerturbationKind::DistractorFiles,
        PerturbationKind::RelocatePaths,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PerturbationKind::RenameVariable => "rename_variable",
            PerturbationKind::ReorderTools => "reorder_tools",
            PerturbationKind::ParaphraseInstructions => "paraphrase_instructions",
            PerturbationKind::DistractorFiles => "distractor_files",
            PerturbationKind::RelocatePaths => "relocate_paths",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<PerturbationKind> {
        PerturbationKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl FaultProfile {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("fault_profile/1"));
        m.insert("profile_id".into(), Json::str(&self.profile_id));
        m.insert(
            "faults".into(),
            Json::Arr(
                self.faults
                    .iter()
                    .map(|f| {
                        Json::obj([
                            ("type", Json::str(f.fault_type.as_str())),
                            ("rate_ppm", Json::Int(f.rate_ppm)),
                            ("at", Json::str(&f.at)),
                        ])
                    })
                    .collect(),
            ),
        );
        if let Some(p) = &self.provenance_ref {
            m.insert("provenance".into(), Json::str(p));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<FaultProfile, SchemaError> {
        const REC: &str = "fault_profile/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["schema", "profile_id", "faults", "provenance"], REC)?;
        Ok(FaultProfile {
            profile_id: str_at(m, "profile_id", REC)?.to_string(),
            faults: arr_at(m, "faults", REC)?
                .iter()
                .map(|f| {
                    Ok(FaultSpec {
                        fault_type: FaultType::parse(
                            f.get("type").and_then(Json::as_str).unwrap_or(""),
                        )
                        .ok_or_else(|| SchemaError::v("faults", "unknown fault type"))?,
                        rate_ppm: f
                            .get("rate_ppm")
                            .and_then(Json::as_int)
                            .ok_or_else(|| SchemaError::v("faults", "missing rate_ppm"))?,
                        at: f.get("at").and_then(Json::as_str).unwrap_or("").to_string(),
                    })
                })
                .collect::<Result<Vec<_>, SchemaError>>()?,
            provenance_ref: opt_str_at(m, "provenance")?.map(str::to_string),
        })
    }
}

impl PerturbationProfile {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("perturbation_profile/1"));
        m.insert("profile_id".into(), Json::str(&self.profile_id));
        m.insert(
            "perturbations".into(),
            Json::Arr(
                self.perturbations
                    .iter()
                    .map(|p| {
                        let mut pm = BTreeMap::new();
                        pm.insert("kind".into(), Json::str(p.kind.as_str()));
                        pm.insert("params".into(), p.params.clone());
                        Json::Obj(pm)
                    })
                    .collect(),
            ),
        );
        if let Some(p) = &self.provenance_ref {
            m.insert("provenance".into(), Json::str(p));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<PerturbationProfile, SchemaError> {
        const REC: &str = "perturbation_profile/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &["schema", "profile_id", "perturbations", "provenance"],
            REC,
        )?;
        Ok(PerturbationProfile {
            profile_id: str_at(m, "profile_id", REC)?.to_string(),
            perturbations: arr_at(m, "perturbations", REC)?
                .iter()
                .map(|p| {
                    Ok(PerturbationSpec {
                        kind: PerturbationKind::parse(
                            p.get("kind").and_then(Json::as_str).unwrap_or(""),
                        )
                        .ok_or_else(|| {
                            SchemaError::v("perturbations", "unknown perturbation kind")
                        })?,
                        params: p.get("params").cloned().unwrap_or(Json::Null),
                    })
                })
                .collect::<Result<Vec<_>, SchemaError>>()?,
            provenance_ref: opt_str_at(m, "provenance")?.map(str::to_string),
        })
    }
}

/// The canonical Stage-3 F-class catalogue rows (HAL-class levels over
/// suite families A `terminal_bench` and D `tau_bench` — data, not code).
pub fn stage3_fault_profiles() -> Vec<FaultProfile> {
    vec![
        FaultProfile {
            profile_id: "f/tool-timeout-p50".into(),
            faults: vec![FaultSpec {
                fault_type: FaultType::ToolTimeout,
                rate_ppm: 500_000,
                at: "phase:agent".into(),
            }],
            provenance_ref: None,
        },
        FaultProfile {
            profile_id: "f/model-5xx-p10".into(),
            faults: vec![FaultSpec {
                fault_type: FaultType::Model5xx,
                rate_ppm: 100_000,
                at: "model_call".into(),
            }],
            provenance_ref: None,
        },
        FaultProfile {
            profile_id: "f/fs-corruption-once".into(),
            faults: vec![FaultSpec {
                fault_type: FaultType::FsCorruption,
                rate_ppm: 1_000_000,
                at: "workspace".into(),
            }],
            provenance_ref: None,
        },
        FaultProfile {
            profile_id: "f/network-partition-agent".into(),
            faults: vec![FaultSpec {
                fault_type: FaultType::NetworkPartition,
                rate_ppm: 1_000_000,
                at: "phase:agent".into(),
            }],
            provenance_ref: None,
        },
        FaultProfile {
            profile_id: "f/tool-response-drift".into(),
            faults: vec![FaultSpec {
                fault_type: FaultType::ToolResponseDrift,
                rate_ppm: 250_000,
                at: "tool_surface".into(),
            }],
            provenance_ref: None,
        },
    ]
}

/// The canonical Stage-3 perturbation profiles (F-class over A/D).
pub fn stage3_perturbation_profiles() -> Vec<PerturbationProfile> {
    vec![
        PerturbationProfile {
            profile_id: "p/rename-variable".into(),
            perturbations: vec![PerturbationSpec {
                kind: PerturbationKind::RenameVariable,
                params: Json::obj([("seed", Json::Int(0))]),
            }],
            provenance_ref: None,
        },
        PerturbationProfile {
            profile_id: "p/distractor-files".into(),
            perturbations: vec![PerturbationSpec {
                kind: PerturbationKind::DistractorFiles,
                params: Json::obj([("count", Json::Int(3))]),
            }],
            provenance_ref: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_profile_roundtrip() {
        for p in stage3_fault_profiles() {
            let j = p.to_json();
            assert_eq!(FaultProfile::from_json(&j).unwrap(), p);
        }
        for p in stage3_perturbation_profiles() {
            let j = p.to_json();
            assert_eq!(PerturbationProfile::from_json(&j).unwrap(), p);
        }
    }

    #[test]
    fn closed_sum_refuses_unknown() {
        assert!(FaultType::parse("power_loss").is_none());
        assert!(PerturbationKind::parse("rewrite_tests").is_none());
    }
}
