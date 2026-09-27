//! `WorkSourceAdapter` — the work-source port (§5i.1 #2; §9's "fixture
//! adapter"). One trait, one Stage-4 implementation: `FixtureAdapter`, a
//! deterministic, replayable source whose `occurrences(since)` returns the
//! *same* snapshot for the same fixture document — the activation's whole
//! observable world is content-pinned (`fixture_ref`), so a restart replays
//! identical inputs (AC-2/AC-5's equality demands the source is not a
//! process-side variable).
//!
//! The real issue-tracker/webhook adapter is the Stage-5 slice's
//! (`088_S5.6__fleet-adapters` under HUMAN-H2 — signed ingress +
//! capability probes). The trait's contract is already the real one:
//! `occurrences(since) → [SourceOccurrence]` is a pure projection of the
//! source document, `suspended(source)` reads the source-declared
//! suspension set (RC-4), and `reconcile(path, params)` is the adapter's
//! own reconcile verb.

use hh_wire::json::Json;
use std::collections::BTreeSet;

use crate::errors::FleetError;
use crate::work_item::WorkItemInit;

/// `SourceOccurrence` — one adapter observation the `observe` seam stores
/// (`context.observation.recorded` + the `external`/`manual` wakeup
/// occurrence). `occurrence_id` is the dedup key — an adapter that replays
/// the same id never double-fires (level-triggered).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceOccurrence {
    /// The occurrence's durable key (the adapter's own id — replaying the
    /// fixture yields the same id, never a new one).
    pub occurrence_id: String,
    /// The trigger kind the occurrence materialises — `external` or
    /// `manual` (the fixture may also present a `timer` occurrence
    /// directly; the kernel-internal path still works).
    pub trigger_kind: String,
    /// The trigger's `kind` qualifier (for `external{kind}` subscriptions —
    /// matched verbatim, e.g. `"ticket.updated"`).
    pub external_kind: Option<String>,
    /// The work item the occurrence admits (the occurrence's payload —
    /// identical under replay).
    pub item: WorkItemInit,
    /// An adapter-side `observe_at_ms` (deterministic; never the wall).
    pub observed_at_ms: u64,
}

/// `WorkSourceAdapter` — the port. Implementations are deterministic
/// projections of a pinned source document; the fixture is the reference.
pub trait WorkSourceAdapter {
    /// `occurrences(since) → [SourceOccurrence]` — the events the source
    /// observed since `since` (a source-side cursor; `None` = the full
    /// snapshot). Deterministic for a pinned document.
    fn occurrences(&self, since: Option<u64>) -> Vec<SourceOccurrence>;

    /// `suspended(source_id)` — whether the source marks this work source
    /// suspended (RC-4's source suspension; durable, never process state).
    fn suspended(&self, source_id: &str) -> bool;

    /// `reconcile(path, params)` — the adapter's own reconcile verb —
    /// `activate.run`: the fixture returns its declared dispatchable set
    /// (spec §5i.1 #2: "the work-source adapter (fixture) ... the
    /// dispatch is the fixture adapter's `reconcile activate.run`").
    fn activate_run(&self, candidates: &[String]) -> Vec<String>;
}

/// `FixtureAdapter` — the reference adapter. Constructed from the pinned
/// fixture document (`hh.fleet.fixture/1`):
///
/// ```json
/// {"schema_version":"hh.fleet.fixture/1",
///  "occurrences":[{"occurrence_id","trigger","kind"?,"item","observed_at_ms"}],
///  "suspended":["source_id", …],
///  "activate_run":["item_id", …]}
/// ```
pub struct FixtureAdapter {
    /// The pinned document (canonical bytes — `fixture_ref` covers them).
    pub doc: Json,
    /// The parsed occurrence set.
    pub occurrence_set: Vec<SourceOccurrence>,
    /// The source-suspended set (RC-4).
    pub suspended_set: BTreeSet<String>,
    /// The declared dispatchable item ids (`activate_run`'s answer).
    pub runnable: BTreeSet<String>,
}

impl FixtureAdapter {
    /// Parse + validate the fixture document — strict (unknown members fail).
    pub fn from_doc(doc: Json) -> Result<FixtureAdapter, FleetError> {
        let Json::Obj(o) = &doc else {
            return Err(FleetError::SchemaViolation {
                detail: "fixture doc must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &["schema_version", "occurrences", "suspended", "activate_run"];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("fixture doc unknown member {k}"),
                });
            }
        }
        let mut occurrence_set = Vec::new();
        match o.get("occurrences") {
            Some(Json::Arr(a)) => {
                for v in a {
                    occurrence_set.push(Self::occurrence_from_json(v)?);
                }
            }
            None => {}
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fixture occurrences must be an array".into(),
                })
            }
        }
        let suspended_set = match o.get("suspended") {
            None | Some(Json::Null) => BTreeSet::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fixture suspended must be an array".into(),
                })
            }
        };
        let runnable = match o.get("activate_run") {
            None | Some(Json::Null) => BTreeSet::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fixture activate_run must be an array".into(),
                })
            }
        };
        Ok(FixtureAdapter {
            doc,
            occurrence_set,
            suspended_set,
            runnable,
        })
    }

    fn occurrence_from_json(v: &Json) -> Result<SourceOccurrence, FleetError> {
        let Json::Obj(o) = v else {
            return Err(FleetError::SchemaViolation {
                detail: "fixture occurrence must be an object".into(),
            });
        };
        let s = |k: &str| -> Result<String, FleetError> {
            o.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("fixture occurrence {k} required"),
                })
        };
        let trigger_kind = s("trigger")?;
        if !matches!(trigger_kind.as_str(), "external" | "manual" | "timer") {
            return Err(FleetError::UnsupportedTrigger { trigger: trigger_kind });
        }
        Ok(SourceOccurrence {
            occurrence_id: s("occurrence_id")?,
            trigger_kind,
            external_kind: o
                .get("kind")
                .and_then(Json::as_str)
                .map(str::to_string),
            item: WorkItemInit::from_json(o.get("item").ok_or_else(|| {
                FleetError::SchemaViolation {
                    detail: "fixture occurrence item required".into(),
                }
            })?)?,
            observed_at_ms: o
                .get("observed_at_ms")
                .and_then(Json::as_int)
                .map(|i| i.max(0) as u64)
                .unwrap_or(0),
        })
    }
}

impl WorkSourceAdapter for FixtureAdapter {
    fn occurrences(&self, since: Option<u64>) -> Vec<SourceOccurrence> {
        self.occurrence_set
            .iter()
            .filter(|o| since.map(|s| o.observed_at_ms > s).unwrap_or(true))
            .cloned()
            .collect()
    }

    fn suspended(&self, source_id: &str) -> bool {
        self.suspended_set.contains(source_id)
    }

    fn activate_run(&self, candidates: &[String]) -> Vec<String> {
        // `activate.run` intersects the candidates with the declared set —
        // a fixture answer, never a heuristic.
        candidates
            .iter()
            .filter(|c| self.runnable.is_empty() || self.runnable.contains(*c))
            .cloned()
            .collect()
    }
}
