//! `WorkSourceAdapter` — the work-source port (§5i.1 #2; §9's "fixture
//! adapter"; ADR-0205 D4's registered component class). Implementations:
//! `FixtureAdapter` (the Stage-4 in-process proxy — deterministic and
//! replayable; the HUMAN-H2 proxy at S5.6), `WebhookAdapter` (the signed
//! ingress lane, [`crate::ingress`]), and the L5 plugin adapter
//! (`hh-fleet-adapter` — the real-tracker lane, out of process).
//!
//! The trait's contract is the real one: `occurrences(since) →
//! [SourceOccurrence]` is the observation feed (poll for pull adapters,
//! the ingress queue for push); `list`/`get` are the adapter read
//! minimum (`[WorkSourceRecord]` — omission = no longer visible, never
//! a synthetic state); `suspended`/`activate_run` are the reconcile
//! verbs; `capabilities()` is the tri-state declaration (probed at
//! bind); `fault()` reports `SourceUnavailable` (skip dispatch this
//! tick — keep activations, never a synthetic state). All methods take
//! `&mut self`: a plugin adapter's `invoke` and the ingress's queue
//! drain are stateful seams (S5.6 — the Stage-4 `&self` signature could
//! not serve an out-of-process adapter).

use hh_wire::json::Json;
use std::collections::BTreeSet;

use crate::capabilities::AdapterCapabilities;
use crate::errors::FleetError;
use crate::work_item::WorkItemInit;

/// `SourceOccurrence` — one adapter observation the `observe` seam stores
/// (`context.observation.recorded` + the `external`/`manual` wakeup
/// occurrence). `occurrence_id` is the dedup key — an adapter that replays
/// the same id never double-fires (level-triggered).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceOccurrence {
    /// The occurrence's durable key (the adapter's own id — replaying the
    /// fixture yields the same id, never a new one; the ingress derives
    /// it by the OQ-313 rule).
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
    /// The producing adapter's label — the `context.observation.recorded`
    /// row's `adapter` member (`fixture` | `webhook` | `plugin:*` —
    /// durable provenance, S5.6; the Stage-4 hardcode was `fixture`).
    pub actor: String,
}

impl SourceOccurrence {
    /// The canonical wire form (`occurrences` op output — the plugin lane
    /// carries occurrences as records, never foreign envelopes).
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("occurrence_id".into(), Json::str(&self.occurrence_id));
        m.insert("trigger".into(), Json::str(&self.trigger_kind));
        if let Some(k) = &self.external_kind {
            m.insert("kind".into(), Json::str(k));
        }
        m.insert("item".into(), self.item.to_json());
        m.insert(
            "observed_at_ms".into(),
            Json::Int(self.observed_at_ms as i64),
        );
        m.insert("actor".into(), Json::str(&self.actor));
        Json::Obj(m)
    }

    /// The strict decode — one schema, both lanes (fixture `occurrences[]`
    /// member and the plugin's `occurrences` op output).
    pub fn from_json(j: &Json) -> Result<SourceOccurrence, FleetError> {
        let Json::Obj(o) = j else {
            return Err(FleetError::SchemaViolation {
                detail: "occurrence must be an object".into(),
            });
        };
        Ok(SourceOccurrence {
            occurrence_id: o
                .get("occurrence_id")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: "occurrence occurrence_id required".into(),
                })?,
            trigger_kind: o
                .get("trigger")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: "occurrence trigger required".into(),
                })?,
            external_kind: o.get("kind").and_then(Json::as_str).map(str::to_string),
            item: WorkItemInit::from_json(o.get("item").ok_or_else(|| {
                FleetError::SchemaViolation {
                    detail: "occurrence item required".into(),
                }
            })?)?,
            observed_at_ms: o
                .get("observed_at_ms")
                .and_then(Json::as_int)
                .map(|i| i.max(0) as u64)
                .unwrap_or(0),
            actor: o
                .get("actor")
                .and_then(Json::as_str)
                .unwrap_or("fixture")
                .to_string(),
        })
    }
}

/// `WorkSourceAdapter` — the port. Implementations are deterministic/// `WorkSourceAdapter` — the port. Implementations are deterministic
/// projections of a pinned source document; the fixture is the reference.
pub trait WorkSourceAdapter {
    /// `occurrences(since) → [SourceOccurrence]` — the events the source
    /// observed since `since` (a source-side cursor; `None` = the full
    /// snapshot). Deterministic for a pinned document.
    fn occurrences(&mut self, since: Option<u64>) -> Vec<SourceOccurrence>;

    /// `suspended(source_id)` — whether the source marks this work source
    /// suspended (RC-4's source suspension; durable, never process state).
    fn suspended(&mut self, source_id: &str) -> bool;

    /// `activate_run(candidates)` — the adapter's reconcile verb —
    /// `activate.run`: the fixture returns its declared dispatchable set
    /// (spec §5i.1 #2: "the work-source adapter (fixture) ... the
    /// dispatch is the fixture adapter's `reconcile activate.run`").
    fn activate_run(&mut self, candidates: &[String]) -> Vec<String>;

    /// `list(states[]) → [WorkSourceRecord]` — the read minimum
    /// (ADR-0205 D4): normalized records filtered by state name; an
    /// omitted record is "no longer visible", never a synthetic state.
    /// The default is the empty answer (an adapter that does not declare
    /// `poll` has nothing to list).
    fn list(&mut self, _states: &[String]) -> Vec<Json> {
        Vec::new()
    }

    /// `get(native_ids[]) → [WorkSourceRecord]` — the read minimum's
    /// direct-fetch half; a malformed *requested* record is an error the
    /// adapter reports through `fault()`/the typed refusal at its own
    /// boundary, never a silent omission.
    fn get(&mut self, _native_ids: &[String]) -> Vec<Json> {
        Vec::new()
    }

    /// The tri-state capability record (§5i.1 #3; T-LCD-07). The default
    /// is all-`unknown` — an adapter that declares nothing answers
    /// `unknown` for every member, never a fabricated `declared`.
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::all_unknown()
    }

    /// `fault()` — `Some(reason)` when the adapter is `SourceUnavailable`
    /// (a plugin that faulted, a signed-ingress whose verifier cannot
    /// resolve keys): the reconciler skips dispatch this tick and keeps
    /// activations — never a synthetic state (§5i.1 #5 failure row).
    fn fault(&self) -> Option<String> {
        None
    }
}

/// `FixtureAdapter` — the reference adapter. Constructed from the pinned
/// fixture document (`hh.fleet.fixture/1`):
///
/// ```json
/// {"schema_version":"hh.fleet.fixture/1",
///  "occurrences":[{"occurrence_id","trigger","kind"?,"item","observed_at_ms"}],
///  "suspended":["source_id", …],
///  "activate_run":["item_id", …],
///  "records":[{"native_id","state",…}],
///  "capabilities":{"poll":"declared", …}}
/// ```
///
/// `records` (S5.6) is the poll lane's declared `WorkSourceRecord` set —
/// the read minimum `list`/`get` serve it; `capabilities` is the
/// adapter's declared tri-state (`unknown` members resolve through the
/// doc's `probe` table — `probe{name: bool}` answers them `probed`/
/// `probed_absent`, exercising T-LCD-07 deterministically).
pub struct FixtureAdapter {
    /// The pinned document (canonical bytes — `fixture_ref` covers them).
    pub doc: Json,
    /// The parsed occurrence set.
    pub occurrence_set: Vec<SourceOccurrence>,
    /// The source-suspended set (RC-4).
    pub suspended_set: BTreeSet<String>,
    /// The declared dispatchable item ids (`activate_run`'s answer).
    pub runnable: BTreeSet<String>,
    /// The declared `WorkSourceRecord`s `list`/`get` serve (S5.6).
    pub records: Vec<Json>,
    /// The capability record after the declared `probe` fold.
    pub caps: AdapterCapabilities,
}

impl FixtureAdapter {
    /// Parse + validate the fixture document — strict (unknown members fail).
    pub fn from_doc(doc: Json) -> Result<FixtureAdapter, FleetError> {
        let Json::Obj(o) = &doc else {
            return Err(FleetError::SchemaViolation {
                detail: "fixture doc must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "schema_version",
            "occurrences",
            "suspended",
            "activate_run",
            "records",
            "capabilities",
            "probe",
        ];
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
        let records = match o.get("records") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a.clone(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fixture records must be an array".into(),
                })
            }
        };
        let mut caps = match o.get("capabilities") {
            None | Some(Json::Null) => AdapterCapabilities {
                // The fixture is a poll-mode source: `poll` is declared,
                // `push_delivery_id` starts `unknown` (the probe table
                // resolves it — T-LCD-07's `unknown` leg).
                poll: crate::capabilities::CapState::Declared,
                ..AdapterCapabilities::all_unknown()
            },
            Some(c) => {
                AdapterCapabilities::from_json(c).map_err(|d| FleetError::SchemaViolation {
                    detail: format!("fixture capabilities: {d}"),
                })?
            }
        };
        // The declared probe answers — `{probe: {name: bool}}` resolves
        // each `unknown` member deterministically (never coerces a
        // `declared` member).
        if let Some(Json::Obj(p)) = o.get("probe") {
            for (name, v) in p {
                if let Json::Bool(b) = v {
                    caps.probe_with(name, *b);
                }
            }
        }
        Ok(FixtureAdapter {
            doc,
            occurrence_set,
            suspended_set,
            runnable,
            records,
            caps,
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
            return Err(FleetError::UnsupportedTrigger {
                trigger: trigger_kind,
            });
        }
        Ok(SourceOccurrence {
            occurrence_id: s("occurrence_id")?,
            trigger_kind,
            external_kind: o.get("kind").and_then(Json::as_str).map(str::to_string),
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
            actor: o
                .get("actor")
                .and_then(Json::as_str)
                .unwrap_or("fixture")
                .to_string(),
        })
    }
}

impl WorkSourceAdapter for FixtureAdapter {
    fn occurrences(&mut self, since: Option<u64>) -> Vec<SourceOccurrence> {
        self.occurrence_set
            .iter()
            .filter(|o| since.map(|s| o.observed_at_ms > s).unwrap_or(true))
            .cloned()
            .collect()
    }

    fn suspended(&mut self, source_id: &str) -> bool {
        self.suspended_set.contains(source_id)
    }

    fn activate_run(&mut self, candidates: &[String]) -> Vec<String> {
        // `activate.run` intersects the candidates with the declared set —
        // a fixture answer, never a heuristic.
        candidates
            .iter()
            .filter(|c| self.runnable.is_empty() || self.runnable.contains(*c))
            .cloned()
            .collect()
    }

    fn capabilities(&self) -> AdapterCapabilities {
        self.caps.clone()
    }

    fn list(&mut self, states: &[String]) -> Vec<Json> {
        self.records
            .iter()
            .filter(|r| {
                states.is_empty()
                    || r.get("state")
                        .and_then(Json::as_str)
                        .map(|s| states.iter().any(|w| w == s))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn get(&mut self, native_ids: &[String]) -> Vec<Json> {
        self.records
            .iter()
            .filter(|r| {
                r.get("native_id")
                    .and_then(Json::as_str)
                    .map(|id| native_ids.iter().any(|w| w == id))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}
