//! `TrackerLogic` — the plugin-side surface (the half a real tracker
//! variant implements out of process; ADR-0180/0181's L5 rule). One
//! trait, one dispatch: the variant's `invoke` lowers to
//! [`dispatch`] — the single decode/encode boundary (CC7), where a
//! malformed input is the ABI's `SchemaViolation` and a domain refusal
//! is data (`{refused: "…"}` inside outputs, never a channel error).
//!
//! [`FixtureTracker`] is the reference logic — a `TrackerLogic` over the
//! pinned `hh.fleet.fixture/1` document (the HUMAN-H2 proxy lane: the
//! real tracker plugs the same ops; nothing here simulates it).

use hh_fleet::source::{FixtureAdapter, SourceOccurrence, WorkSourceAdapter};
use hh_wire::json::Json;

use crate::abi::{self, AbiCodecError};

/// `TrackerLogic` — the class's plugin-side contract. Every op answers
/// the canonical `Json` document the [`dispatch`] table wraps for the
/// wire. Implementations are deterministic records-in/records-out —
/// no ambient env/fs/net reach (the variant host's isolation owns that).
pub trait TrackerLogic {
    /// `capabilities` → the `AdapterCapabilities` record (tri-state).
    fn capabilities(&mut self) -> Json;
    /// `occurrences{since?}` → `[SourceOccurrence]` documents.
    fn occurrences(&mut self, since: Option<u64>) -> Json;
    /// `suspended{source_id}` → `bool`.
    fn suspended(&mut self, source_id: &str) -> Json;
    /// `activate_run{candidates[]}` → `[item_id]`.
    fn activate_run(&mut self, candidates: &[String]) -> Json;
    /// `list{states[]}` → `[WorkSourceRecord]`.
    fn list(&mut self, states: &[String]) -> Json;
    /// `get{native_ids[]}` → `[WorkSourceRecord]`.
    fn get(&mut self, native_ids: &[String]) -> Json;
}

/// `dispatch(logic, op, inputs)` — the one wire boundary. Returns the
/// wrapped outputs (`{result: "<canonical-json>"}` each).
pub fn dispatch(
    logic: &mut dyn TrackerLogic,
    operation: &str,
    inputs: &[Json],
) -> Result<Vec<Json>, AbiCodecError> {
    let doc = match operation {
        "capabilities" => logic.capabilities(),
        "occurrences" => {
            let args = abi::arg(inputs, 0);
            let since = match args.get("since") {
                None | Some(Json::Null) => None,
                Some(Json::Int(i)) => Some((*i).max(0) as u64),
                _ => {
                    return Err(AbiCodecError::Malformed {
                        detail: "occurrences.since must be an integer".into(),
                    })
                }
            };
            logic.occurrences(since)
        }
        "suspended" => {
            let args = abi::arg(inputs, 0);
            let Some(id) = args.get("source_id").and_then(Json::as_str) else {
                return Err(AbiCodecError::Malformed {
                    detail: "suspended requires source_id:string".into(),
                });
            };
            logic.suspended(id)
        }
        "activate_run" => {
            let args = abi::arg(inputs, 0);
            let c = abi::str_members(&args, "candidates")?;
            logic.activate_run(&c)
        }
        "list" => {
            let args = abi::arg(inputs, 0);
            let s = abi::str_members(&args, "states")?;
            logic.list(&s)
        }
        "get" => {
            let args = abi::arg(inputs, 0);
            let ids = abi::str_members(&args, "native_ids")?;
            logic.get(&ids)
        }
        other => {
            return Err(AbiCodecError::Malformed {
                detail: format!("unknown op {other}"),
            })
        }
    };
    Ok(vec![abi::wrap_output(&doc)])
}

/// `FixtureTracker` — the reference `TrackerLogic`: the pinned fixture
/// document drives every op (the HUMAN-H2 proxy; a real tracker variant
/// implements the same surface against its own client). `&mut` state —
/// the wire's `occurrences` is a drain-free read (the engine's durable
/// dedup owns idempotence; the fixture replay answers identically).
pub struct FixtureTracker {
    /// The wrapped fixture adapter.
    pub inner: FixtureAdapter,
}

impl FixtureTracker {
    /// `from_doc(doc)` — parse the pinned fixture document.
    pub fn from_doc(doc: Json) -> Result<FixtureTracker, hh_fleet::errors::FleetError> {
        Ok(FixtureTracker {
            inner: FixtureAdapter::from_doc(doc)?,
        })
    }

    /// The canonical `SourceOccurrence` documents.
    fn occ_docs(&mut self, since: Option<u64>) -> Vec<Json> {
        self.inner
            .occurrences(since)
            .iter()
            .map(SourceOccurrence::to_json)
            .collect()
    }
}

impl TrackerLogic for FixtureTracker {
    fn capabilities(&mut self) -> Json {
        self.inner.capabilities().to_json()
    }

    fn occurrences(&mut self, since: Option<u64>) -> Json {
        Json::Arr(self.occ_docs(since))
    }

    fn suspended(&mut self, source_id: &str) -> Json {
        Json::Bool(self.inner.suspended(source_id))
    }

    fn activate_run(&mut self, candidates: &[String]) -> Json {
        Json::Arr(
            self.inner
                .activate_run(candidates)
                .iter()
                .map(Json::str)
                .collect(),
        )
    }

    fn list(&mut self, states: &[String]) -> Json {
        Json::Arr(self.inner.list(states))
    }

    fn get(&mut self, native_ids: &[String]) -> Json {
        Json::Arr(self.inner.get(native_ids))
    }
}
