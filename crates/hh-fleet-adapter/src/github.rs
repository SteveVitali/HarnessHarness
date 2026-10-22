//! `GithubTracker` — the `TrackerLogic` for the GitHub Issues variant
//! (R4.7; DF-S5.6-1; HUMAN-H2 "GitHub Issues + tunnel"). The bind
//! document is `hh.fleet.github_source/1`:
//!
//! ```json
//! {"schema_version": "hh.fleet.github_source/1",
//!  "github": {GithubConfig — repo/source_id/owner_map/…},
//!  "issues": [<raw GitHub issue objects>],
//!  "occurrences": [<SourceOccurrence docs>],
//!  "suspended": [<source_id>], "activate_run": [<item_id>],
//!  "capabilities": {…}, "probe": {…}}
//! ```
//!
//! The variant is records-in/records-out (T-LCD-12): the *host* holds
//! the GitHub REST lane (the token is broker-held; `subprocess_confined`
//! placement admits no network egress) and pins the issue snapshot into
//! `issues[]` at bind; the plugin normalizes through the one shared
//! `hh_fleet::github` mapping (CC1 — never a second dialect codec).

use std::collections::BTreeSet;

use hh_fleet::capabilities::{AdapterCapabilities, CapState};
use hh_fleet::errors::FleetError;
use hh_fleet::github::{self, GithubConfig, GITHUB_SOURCE_SCHEMA};
use hh_fleet::source::SourceOccurrence;
use hh_wire::json::Json;

use crate::tracker::TrackerLogic;

/// `GithubTracker` — the GitHub Issues `TrackerLogic`: the six
/// `work_source_adapter` ops over the pinned `hh.fleet.github_source/1`
/// document. Like `FixtureTracker` every op is a deterministic read of
/// the bound document — `occurrences` answers verbatim (replay-stable
/// ids; the durable fold owns idempotence), `list`/`get` normalize the
/// bound `issues[]` snapshot through `github::issue_record`, and
/// `activate_run` answers the candidates whose issues are `open` (a
/// closed issue is never dispatchable).
pub struct GithubTracker {
    /// The GitHub dialect config (`github` member).
    cfg: GithubConfig,
    /// The raw GitHub issue objects bound at `bind` (the host-side
    /// `GET /repos/{repo}/issues` snapshot — records-in).
    issues: Vec<Json>,
    /// The verbatim `occurrences[]` member (host-mapped records — the
    /// push lane's own dedupe is durable, not adapter-side).
    occurrences: Vec<SourceOccurrence>,
    /// The verbatim `suspended[]` member (RC-4's source suspension).
    suspended_set: BTreeSet<String>,
    /// The declared dispatchable set — `None` = derive from open issues.
    runnable: Option<BTreeSet<String>>,
    /// The declared capability set (tri-state — `probe` resolves).
    caps: AdapterCapabilities,
}

impl GithubTracker {
    /// `from_doc(doc)` — parse the pinned `hh.fleet.github_source/1`
    /// document; an undecodable doc is a bind refusal, never a fault.
    pub fn from_doc(doc: Json) -> Result<GithubTracker, FleetError> {
        let Json::Obj(o) = &doc else {
            return Err(FleetError::SchemaViolation {
                detail: "github source doc must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "schema_version",
            "github",
            "issues",
            "occurrences",
            "suspended",
            "activate_run",
            "capabilities",
            "probe",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("github source doc unknown member {k}"),
                });
            }
        }
        match o.get("schema_version").and_then(Json::as_str) {
            Some(GITHUB_SOURCE_SCHEMA) => {}
            Some(other) => {
                return Err(FleetError::SchemaViolation {
                    detail: format!("github source doc schema_version {other}"),
                })
            }
            None => {
                return Err(FleetError::SchemaViolation {
                    detail: "github source doc schema_version required".into(),
                })
            }
        }
        let cfg = GithubConfig::from_json(o.get("github").ok_or_else(|| {
            FleetError::SchemaViolation {
                detail: "github source doc requires the github member".into(),
            }
        })?)?;
        let issues = match o.get("issues") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a.clone(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github source doc issues must be an array".into(),
                })
            }
        };
        let occurrences = match o.get("occurrences") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .map(SourceOccurrence::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github source doc occurrences must be an array".into(),
                })
            }
        };
        let suspended_set = match o.get("suspended") {
            None | Some(Json::Null) => BTreeSet::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github source doc suspended must be an array".into(),
                })
            }
        };
        let runnable = match o.get("activate_run") {
            None | Some(Json::Null) => None,
            Some(Json::Arr(a)) => Some(
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github source doc activate_run must be an array".into(),
                })
            }
        };
        let mut caps = match o.get("capabilities") {
            None | Some(Json::Null) => GithubTracker::default_caps(),
            Some(c) => {
                AdapterCapabilities::from_json(c).map_err(|d| FleetError::SchemaViolation {
                    detail: format!("github source doc capabilities: {d}"),
                })?
            }
        };
        if let Some(Json::Obj(p)) = o.get("probe") {
            for (name, v) in p {
                if let Json::Bool(b) = v {
                    caps.probe_with(name, *b);
                }
            }
        }
        Ok(GithubTracker {
            cfg,
            issues,
            occurrences,
            suspended_set,
            runnable,
            caps,
        })
    }

    /// The GitHub variant's declared capability set — `X-GitHub-Delivery`
    /// is a durable delivery id (`push_delivery_id: declared`), the REST
    /// issues read is `poll`, `owner_map` is `assignee_mapping`; R4.7 is
    /// the read+ingress half — no agent write tools through the source
    /// and no blocker-metadata mapping (honest `declared_absent`).
    pub fn default_caps() -> AdapterCapabilities {
        AdapterCapabilities {
            push_delivery_id: CapState::Declared,
            poll: CapState::Declared,
            native_write_tools: CapState::DeclaredAbsent,
            assignee_mapping: CapState::Declared,
            blocker_metadata: CapState::DeclaredAbsent,
            volatile_fields: vec!["updated_at".to_string()],
        }
    }

    /// The normalized records `list`/`get` serve.
    fn records(&self) -> Vec<Json> {
        self.issues.iter().map(github::issue_record).collect()
    }

    /// The item ids of the doc's *open* issues — the derived
    /// `activate_run` set when no explicit `activate_run` member binds.
    fn open_item_ids(&self) -> BTreeSet<String> {
        self.issues
            .iter()
            .filter(|i| i.get("state").and_then(Json::as_str) == Some("open"))
            .filter_map(|i| i.get("number").and_then(Json::as_int))
            .map(|n| github::github_item_id(&self.cfg.repo, n))
            .collect()
    }
}

impl TrackerLogic for GithubTracker {
    fn capabilities(&mut self) -> Json {
        self.caps.to_json()
    }

    fn occurrences(&mut self, since: Option<u64>) -> Json {
        Json::Arr(
            self.occurrences
                .iter()
                .filter(|o| since.map(|s| o.observed_at_ms > s).unwrap_or(true))
                .map(SourceOccurrence::to_json)
                .collect(),
        )
    }

    fn suspended(&mut self, source_id: &str) -> Json {
        // The declared suspended set plus the GitHub-side suspension: an
        // archived repository marks its own source suspended (RC-4).
        Json::Bool(
            self.suspended_set.contains(source_id)
                || (self.cfg.repo_archived && source_id == self.cfg.source_id),
        )
    }

    fn activate_run(&mut self, candidates: &[String]) -> Json {
        let allowed = self
            .runnable
            .clone()
            .unwrap_or_else(|| self.open_item_ids());
        Json::Arr(
            candidates
                .iter()
                .filter(|c| allowed.contains(*c))
                .map(Json::str)
                .collect(),
        )
    }

    fn list(&mut self, states: &[String]) -> Json {
        Json::Arr(
            self.records()
                .into_iter()
                .filter(|r| {
                    states.is_empty()
                        || r.get("state")
                            .and_then(Json::as_str)
                            .map(|s| states.iter().any(|w| w == s))
                            .unwrap_or(false)
                })
                .collect(),
        )
    }

    fn get(&mut self, native_ids: &[String]) -> Json {
        Json::Arr(
            self.records()
                .into_iter()
                .filter(|r| {
                    r.get("native_id")
                        .and_then(Json::as_str)
                        .map(|id| native_ids.iter().any(|w| w == id))
                        .unwrap_or(false)
                })
                .collect(),
        )
    }
}
