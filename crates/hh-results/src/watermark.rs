//! `WatermarkSet` — `{run_id → seq}` — the exact source prefixes a derived
//! object was projected over (§6.5 §1 "project()-pure at a recorded watermark
//! set"; ADR-0161 D4). Every read returns the set it was served at; the set is
//! part of the `view_hash` preimage so byte-repeatability is checkable.

use std::collections::BTreeMap;

use hh_wire::json::Json;

/// `{run_id → seq}` — the inclusive high-water mark per source run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatermarkSet {
    /// The per-run high watermarks (ordered — `BTreeMap` canonical order).
    pub runs: BTreeMap<String, u64>,
}

impl WatermarkSet {
    /// The empty set.
    pub fn new() -> WatermarkSet {
        WatermarkSet::default()
    }

    /// Record `run`'s high-water seq (max wins — a watermark never recedes).
    pub fn pin(&mut self, run_id: &str, seq: u64) {
        let e = self.runs.entry(run_id.to_string()).or_insert(seq);
        if seq > *e {
            *e = seq;
        }
    }

    /// `self` covers `other` — every `(run, seq)` in `other` is ≤ `self`'s
    /// pin for that run. A run absent from `self` never covers.
    pub fn covers(&self, other: &WatermarkSet) -> bool {
        other
            .runs
            .iter()
            .all(|(r, s)| self.runs.get(r).map(|w| s <= w).unwrap_or(false))
    }

    /// `self`'s pin for `run` (`None` = the set does not cover the run).
    pub fn get(&self, run_id: &str) -> Option<u64> {
        self.runs.get(run_id).copied()
    }

    /// The canonical JSON — `{"<run_id>": <seq>, …}` (BTreeMap order).
    pub fn to_json(&self) -> Json {
        Json::Obj(
            self.runs
                .iter()
                .map(|(r, s)| (r.clone(), Json::Int(*s as i64)))
                .collect(),
        )
    }

    /// Strict decode — every member must be a non-negative int.
    pub fn from_json(j: &Json) -> Option<WatermarkSet> {
        let Json::Obj(m) = j else {
            return None;
        };
        let mut runs = BTreeMap::new();
        for (k, v) in m {
            let s = v.as_int()?;
            if s < 0 {
                return None;
            }
            runs.insert(k.clone(), s as u64);
        }
        Some(WatermarkSet { runs })
    }
}
